# Azure Deployment Runbook (VM + Azure Database for PostgreSQL + Application Gateway)

Deploys SorobanPulse as a Docker container on a self-managed **Azure VM**
inside a VNet, backed by **Azure Database for PostgreSQL — Flexible Server**,
and fronted by an **Application Gateway** terminating TLS. There is no
managed/serverless Azure option documented in
[docs/deployment-platforms.md](../deployment-platforms.md) today — this is
the primary Azure path for SorobanPulse.

This repo's `terraform/` directory only contains AWS modules — there is no
Azure Terraform here, so every step below uses the `az` CLI directly.

## Prerequisites

- **Accounts** — an Azure subscription with permission to create resource
  groups, VNets, VMs, Azure Database for PostgreSQL, and Application Gateway
  resources.
- **CLI tools** — Azure CLI (`az version`), `docker`, `psql` for verification.
- **Credentials** — `az login` (or a service principal via `az login
  --service-principal`) with Contributor on the target subscription/resource
  group.
- **DNS** — a domain/subdomain you control, to point at the Application
  Gateway's public IP and to bind a TLS certificate to.
- **A TLS certificate** — Application Gateway needs a certificate (PFX) to
  terminate TLS; either bring your own (e.g. from your CA or Let's Encrypt)
  or store one in Azure Key Vault and reference it.
- **Repository assets used by this runbook**:
  - Root [`Dockerfile`](../../Dockerfile) — builds the `soroban-pulse` image.
  - [`.env.example`](../../.env.example) — full environment variable reference.

## Architecture

```
                              Internet
                                 │
                        TLS cert (PFX/Key Vault)
                                 │
                     ┌───────────────────────┐
                     │  Application Gateway   │   dedicated subnet
                     │  (Standard_v2, WAF opt)│
                     └───────────┬────────────┘
                                 │ HTTP:3000 (health probe GET /healthz/ready)
                     ┌───────────▼────────────┐
                     │  Azure VM               │   app subnet, no public IP
                     │  docker run soroban-pulse│
                     └───────────┬────────────┘
                                 │ 5432 (VNet integration / private access)
                     ┌───────────▼────────────┐
                     │  Azure DB for PostgreSQL│   delegated subnet
                     │  Flexible Server        │
                     └─────────────────────────┘
```

- The Application Gateway is the only internet-facing component; it
  terminates TLS and forwards to a backend pool containing the VM's private
  IP on port 3000. See
  [docs/deployment.md § TLS Termination](../deployment.md#tls-termination) for
  the general comparison of TLS termination options.
- The VM has **no public IP**; outbound internet access (Soroban RPC, apt
  installs) goes through Azure's default outbound access or a NAT Gateway you
  attach to the subnet.
- Azure Database for PostgreSQL Flexible Server is deployed with **VNet
  integration** (private access), reachable only from within the VNet — not
  over the public internet.
- NSGs on the app subnet allow only the Application Gateway subnet's range on
  3000; the database subnet allows only the app subnet's range on 5432.

## Deployment Steps

### 1. Create the resource group and VNet

```bash
RG=soroban-pulse-rg
LOCATION=eastus

az group create --name "$RG" --location "$LOCATION"

az network vnet create \
  --resource-group "$RG" --name soroban-pulse-vnet \
  --address-prefix 10.1.0.0/16 \
  --subnet-name appgw-subnet --subnet-prefix 10.1.1.0/24

az network vnet subnet create \
  --resource-group "$RG" --vnet-name soroban-pulse-vnet \
  --name app-subnet --address-prefix 10.1.2.0/24

az network vnet subnet create \
  --resource-group "$RG" --vnet-name soroban-pulse-vnet \
  --name db-subnet --address-prefix 10.1.3.0/24 \
  --delegations Microsoft.DBforPostgreSQL/flexibleServers
```

### 2. Create NSGs

```bash
az network nsg create --resource-group "$RG" --name soroban-pulse-app-nsg

az network nsg rule create \
  --resource-group "$RG" --nsg-name soroban-pulse-app-nsg \
  --name allow-appgw-3000 --priority 100 \
  --source-address-prefixes 10.1.1.0/24 --destination-port-ranges 3000 \
  --access Allow --protocol Tcp --direction Inbound

az network vnet subnet update \
  --resource-group "$RG" --vnet-name soroban-pulse-vnet \
  --name app-subnet --network-security-group soroban-pulse-app-nsg
```

### 3. Create Azure Database for PostgreSQL Flexible Server

```bash
az postgres flexible-server create \
  --resource-group "$RG" --name soroban-pulse-db \
  --location "$LOCATION" \
  --vnet soroban-pulse-vnet --subnet db-subnet \
  --sku-name Standard_D2ds_v5 --tier GeneralPurpose \
  --storage-size 32 --version 16 \
  --admin-user soroban_admin --admin-password "CHANGE_ME" \
  --high-availability Disabled

az postgres flexible-server db create \
  --resource-group "$RG" --server-name soroban-pulse-db \
  --database-name soroban_pulse
```

`Standard_D2ds_v5` (2 vCPU / 8 GiB) is a reasonable production starting
point; `Standard_B1ms` (Burstable, 1 vCPU / 2 GiB) is sufficient for
staging/dev.

### 4. Build and push the image

Push to Azure Container Registry (or any registry reachable from the VM):

```bash
az acr create --resource-group "$RG" --name sorobanpulseacr --sku Basic
az acr login --name sorobanpulseacr

docker build -t soroban-pulse .
docker tag soroban-pulse:latest sorobanpulseacr.azurecr.io/soroban-pulse:latest
docker push sorobanpulseacr.azurecr.io/soroban-pulse:latest
```

### 5. Create the VM with a cloud-init script

`Standard_B2s` (2 vCPU / 4 GiB, Burstable) comfortably runs the app
container; move to `Standard_D2s_v5` for sustained production load.

```bash
DB_HOST=$(az postgres flexible-server show \
  --resource-group "$RG" --name soroban-pulse-db \
  --query "fullyQualifiedDomainName" -o tsv)

cat > cloud-init.yaml <<EOF
#cloud-config
package_update: true
packages:
  - docker.io
runcmd:
  - systemctl enable --now docker
  - az acr login --name sorobanpulseacr
  - docker pull sorobanpulseacr.azurecr.io/soroban-pulse:latest
  - |
    cat > /etc/soroban-pulse.env <<ENV
    DATABASE_URL=postgres://soroban_admin:CHANGE_ME@${DB_HOST}:5432/soroban_pulse
    STELLAR_RPC_URL=https://soroban-testnet.stellar.org
    PORT=3000
    RUST_LOG=info
    RUST_LOG_FORMAT=json
    DB_MAX_CONNECTIONS=10
    RATE_LIMIT_PER_MINUTE=60
    START_LEDGER=0
    ENV
  - docker run -d --name soroban-pulse --restart unless-stopped
      --env-file /etc/soroban-pulse.env -p 3000:3000
      sorobanpulseacr.azurecr.io/soroban-pulse:latest
EOF

az vm create \
  --resource-group "$RG" --name soroban-pulse-app \
  --image Ubuntu2204 \
  --size Standard_B2s \
  --vnet-name soroban-pulse-vnet --subnet app-subnet \
  --nsg soroban-pulse-app-nsg \
  --public-ip-address "" \
  --assign-identity \
  --custom-data cloud-init.yaml \
  --generate-ssh-keys
```

Grant the VM's managed identity `AcrPull` on the registry so it can pull the
image without embedded credentials:

```bash
VM_IDENTITY=$(az vm show --resource-group "$RG" --name soroban-pulse-app \
  --query identity.principalId -o tsv)
ACR_ID=$(az acr show --name sorobanpulseacr --query id -o tsv)

az role assignment create --assignee "$VM_IDENTITY" --role AcrPull --scope "$ACR_ID"
```

### 6. Create the Application Gateway with TLS

```bash
az network public-ip create \
  --resource-group "$RG" --name soroban-pulse-pip --sku Standard --allocation-method Static

VM_PRIVATE_IP=$(az vm list-ip-addresses --resource-group "$RG" --name soroban-pulse-app \
  --query "[0].virtualMachine.network.privateIpAddresses[0]" -o tsv)

az network application-gateway create \
  --resource-group "$RG" --name soroban-pulse-appgw \
  --location "$LOCATION" \
  --sku Standard_v2 --capacity 2 \
  --vnet-name soroban-pulse-vnet --subnet appgw-subnet \
  --public-ip-address soroban-pulse-pip \
  --servers "$VM_PRIVATE_IP" \
  --frontend-port 443 \
  --cert-file soroban-pulse.pfx --cert-password "CHANGE_ME" \
  --http-settings-port 3000 --http-settings-protocol Http

az network application-gateway probe create \
  --resource-group "$RG" --gateway-name soroban-pulse-appgw \
  --name soroban-pulse-probe \
  --protocol Http --host-name-from-http-settings true \
  --path /healthz/ready --interval 30 --timeout 5 --threshold 2

az network application-gateway http-settings update \
  --resource-group "$RG" --gateway-name soroban-pulse-appgw \
  --name appGatewayBackendHttpSettings --probe soroban-pulse-probe
```

Point your DNS `A` record for `pulse.example.com` at the Standard public IP:

```bash
az network public-ip show --resource-group "$RG" --name soroban-pulse-pip \
  --query ipAddress -o tsv
```

## Verification

```bash
# 1. Backend health as seen by the gateway
az network application-gateway show-backend-health \
  --resource-group "$RG" --name soroban-pulse-appgw

# 2. Health endpoint through the Application Gateway
curl -sf "https://pulse.example.com/healthz/ready" | jq .
curl -sf "https://pulse.example.com/healthz/live"

# 3. Smoke-test a real API endpoint
curl -sf "https://pulse.example.com/v1/events?limit=1" | jq .

# 4. Database connectivity (run from the VM)
az vm run-command invoke \
  --resource-group "$RG" --name soroban-pulse-app \
  --command-id RunShellScript \
  --scripts 'psql "$DATABASE_URL" -c "SELECT 1;"'

# 5. Indexer progress
curl -sf "https://pulse.example.com/metrics" | grep soroban_pulse_indexer_current_ledger
```

## Rollback

- **Bad app deploy**: run the previous image tag on the VM and restart:
  ```bash
  az vm run-command invoke \
    --resource-group "$RG" --name soroban-pulse-app \
    --command-id RunShellScript \
    --scripts 'docker pull sorobanpulseacr.azurecr.io/soroban-pulse:<previous-tag> &&
               docker stop soroban-pulse && docker rm soroban-pulse &&
               docker run -d --name soroban-pulse --restart unless-stopped
               --env-file /etc/soroban-pulse.env -p 3000:3000
               sorobanpulseacr.azurecr.io/soroban-pulse:<previous-tag>'
  ```
  For zero-downtime, create a second VM from the previous image, add its
  private IP to the Application Gateway backend pool, confirm it's healthy,
  then remove the bad one.
- **Bad DB migration**: SorobanPulse applies migrations automatically on
  startup — follow
  [docs/deployment.md § Migration Strategy](../deployment.md#migration-strategy)
  before rolling the app back so the old binary isn't pointed at a newer
  schema.
- **Abandoning the deployment**: delete the Application Gateway, VM, Flexible
  Server, and VNet, in that order
  (`az network application-gateway delete`, `az vm delete`,
  `az postgres flexible-server delete`, `az network vnet delete`), or simply
  `az group delete --name "$RG"` to remove everything. Take a database backup
  first if data must be retained (`az postgres flexible-server backup`).

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `show-backend-health` reports `Unhealthy` | NSG blocks Application Gateway subnet → app subnet on 3000, or app not listening | Confirm the NSG rule's source is `10.1.1.0/24` (the `appgw-subnet` range); `az vm run-command invoke ... --scripts 'docker logs soroban-pulse --tail 100'` |
| Application Gateway returns `502` | Backend pool points at the wrong/old private IP, or the container crashed | `az network application-gateway show --resource-group "$RG" --name soroban-pulse-appgw --query "backendAddressPools"`; check container logs |
| Health probe never turns healthy | Probe path/port mismatch, or `start_period` too short for cold start | Confirm the probe uses `/healthz/ready` on port 3000 (matches `--http-settings-port 3000`); the Dockerfile's own healthcheck uses a 30 s `start_period` — the gateway probe interval/threshold should tolerate at least that long |
| App can't reach the database | Subnet delegation missing on `db-subnet`, or VNet integration not enabled on the Flexible Server | `az postgres flexible-server show --resource-group "$RG" --name soroban-pulse-db --query "network"`; confirm `delegatedSubnetResourceId` is set |
| `docker pull` fails on the VM with `unauthorized` | VM's managed identity isn't granted `AcrPull`, or the role assignment hasn't propagated yet (~1 min) | Re-check `az role assignment list --assignee "$VM_IDENTITY"`; re-run `az acr login` on the VM |
| High connection count on the Flexible Server | `DB_MAX_CONNECTIONS` too high for the tier, or a leak | See [docs/runbooks/db-pool-exhaustion.md](../runbooks/db-pool-exhaustion.md); `Standard_D2ds_v5` defaults to `max_connections` ≈ 200 |

For anything beyond the deployment itself (indexer lag, RPC errors, webhook
failures once the service is live), use
[docs/runbooks/operator-runbook.md](../runbooks/operator-runbook.md).

### Cost note

`Standard_B2s` VM (~$30/mo) + `Standard_D2ds_v5` Flexible Server (~$115/mo)
+ `Standard_v2` Application Gateway (~$25/mo + capacity units) ≈ **$170/mo**.
Use `Standard_B1ms`/Burstable tiers for non-production environments to bring
this down substantially — see [Azure Pricing Calculator](https://azure.microsoft.com/pricing/calculator/)
for exact, region-specific numbers.
