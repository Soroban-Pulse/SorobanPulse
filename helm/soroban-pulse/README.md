# soroban-pulse Helm Chart

## Secret Management

The chart injects three sensitive values into the application via a Kubernetes Secret:

| Key | Description |
|-----|-------------|
| `DATABASE_URL` | PostgreSQL connection string (includes password) |
| `API_KEY` | API authentication key |
| `SMTP_PASSWORD` | SMTP password for email notifications (optional) |

### Default (chart-managed Secret)

By default the chart creates its own Secret from `values.yaml`:

```yaml
secrets:
  databaseUrl: "postgres://user:password@host:5432/db"
  apiKey: "my-api-key"
  smtpPassword: "my-smtp-password"  # omit if not using email
```

**Important:** Kubernetes Secrets are base64-encoded, not encrypted at rest by
default. Anyone with `helm get values` or `kubectl get secret` access can read
these values. Treat the chart-managed Secret as a convenience for development
and staging only.

### Production: bring your own Secret (`existingSecret`)

Set `existingSecret` to the name of a pre-created Secret. The chart will skip
Secret creation and reference your Secret instead:

```yaml
existingSecret: "soroban-pulse-credentials"
```

The referenced Secret must contain at minimum:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: soroban-pulse-credentials
type: Opaque
stringData:
  DATABASE_URL: "postgres://user:password@host:5432/db"
  API_KEY: "my-api-key"
  SMTP_PASSWORD: "my-smtp-password"  # optional
```

### Recommended tools for production

#### external-secrets + AWS Secrets Manager / GCP Secret Manager

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: soroban-pulse-credentials
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: aws-secretsmanager
    kind: ClusterSecretStore
  target:
    name: soroban-pulse-credentials
  data:
    - secretKey: DATABASE_URL
      remoteRef:
        key: soroban-pulse/database-url
    - secretKey: API_KEY
      remoteRef:
        key: soroban-pulse/api-key
```

Then in `values.yaml`:

```yaml
existingSecret: "soroban-pulse-credentials"
```

#### HashiCorp Vault (Agent Injector)

Annotate the pod to have the Vault Agent sidecar populate a Kubernetes Secret,
then point `existingSecret` at the resulting Secret name.

#### Sealed Secrets

Encrypt the Secret with `kubeseal` and commit the resulting `SealedSecret` to
Git. The in-cluster controller decrypts it at deploy time. Point `existingSecret`
at the resulting decrypted Secret name.

## Production customization

The chart exposes production controls in `values.yaml` without requiring template edits:

```yaml
replicaCount: 3
imagePullSecrets:
  - name: registry-credentials
podAnnotations:
  prometheus.io/scrape: "true"
  prometheus.io/port: "3000"
nodeSelector:
  kubernetes.io/os: linux
tolerations: []
affinity: {}
topologySpreadConstraints:
  - maxSkew: 1
    topologyKey: topology.kubernetes.io/zone
    whenUnsatisfiable: ScheduleAnyway
    labelSelector:
      matchLabels:
        app.kubernetes.io/name: soroban-pulse
extraEnv:
  - name: LOG_LEVEL
    value: info
```

The deployment uses a rolling-update strategy, a non-root runtime security context, a disabled service-account token mount, configurable scheduling constraints, and a configurable ServiceAccount with cloud-identity annotations. Set `serviceAccount.create: false` and provide `serviceAccount.name` when your platform provisions the workload identity externally. Set `existingSecret` in production so credentials come from an external secret manager rather than chart values.

## Installation

```bash
# Development (chart-managed secret — not for production)
helm install soroban-pulse ./helm/soroban-pulse \
  --set secrets.databaseUrl="postgres://user:pass@host/db" \
  --set secrets.apiKey="dev-key"

# Production (pre-created secret)
helm install soroban-pulse ./helm/soroban-pulse \
  --set existingSecret="soroban-pulse-credentials"
```
