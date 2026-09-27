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

## Web dashboard

The image bundles the web dashboard and serves it at `/ui` on the service port. It is on by default:

| Value | Default | Description |
|---|---|---|
| `dashboard.enabled` | `true` | Sets `SERVE_DASHBOARD`. Set `false` to serve the API only. |
| `dashboard.connectSrc` | `[]` | Extra API origins the dashboard may call (added to its CSP `connect-src`). |

```bash
helm upgrade --install pulse ./helm/soroban-pulse --set dashboard.enabled=false
```

The dashboard shell is public (no API key needed to load it). Its data requests go through the normal
authenticated API, so users enter an API key in the dashboard's **Settings** tab when `API_KEY` is set.

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

## Observability

### ServiceMonitor (Prometheus Operator)

The chart can create a `ServiceMonitor` so Prometheus automatically discovers and scrapes the `/metrics` endpoint. This requires the [Prometheus Operator](https://github.com/prometheus-operator/prometheus-operator) CRDs to be present in the cluster (installed by `kube-prometheus-stack` or standalone).

| Value | Default | Description |
|---|---|---|
| `serviceMonitor.enabled` | `false` | Create the ServiceMonitor resource |
| `serviceMonitor.namespace` | `""` | Namespace to deploy into (defaults to release namespace) |
| `serviceMonitor.path` | `/metrics` | Scrape path |
| `serviceMonitor.interval` | `30s` | Scrape interval |
| `serviceMonitor.scrapeTimeout` | `10s` | Per-scrape timeout (must be ≤ interval) |
| `serviceMonitor.additionalLabels` | `{}` | Extra labels — use to match the Prometheus Operator `serviceMonitorSelector` |
| `serviceMonitor.relabelings` | `[]` | Prometheus relabeling rules applied before ingestion |
| `serviceMonitor.metricRelabelings` | `[]` | Metric relabeling rules applied after scraping |

```yaml
# Minimal — works with kube-prometheus-stack defaults
serviceMonitor:
  enabled: true
  additionalLabels:
    release: kube-prometheus-stack   # must match your Prometheus serviceMonitorSelector
```

### PrometheusRule

The chart ships all alerting rules from [`docs/alerts.yml`](../../docs/alerts.yml) as a `PrometheusRule` resource. The rules cover indexer lag, RPC errors, HTTP SLOs, database pool exhaustion, replica sync, SLO burn rates, and feature-flag rollback events.

| Value | Default | Description |
|---|---|---|
| `prometheusRule.enabled` | `false` | Create the PrometheusRule resource |
| `prometheusRule.namespace` | `""` | Namespace to deploy into (defaults to release namespace) |
| `prometheusRule.additionalLabels` | `{}` | Extra labels — use to match the Prometheus Operator `ruleSelector` |

```yaml
prometheusRule:
  enabled: true
  additionalLabels:
    release: kube-prometheus-stack   # must match your Prometheus ruleSelector
```

Alert rule summary:

| Alert | Severity | Description |
|---|---|---|
| `IndexerLagHigh` | warning | Indexer lag > 100 ledgers for 5 min |
| `IndexerLagCritical` | critical | Indexer lag > 500 ledgers for 10 min |
| `IndexerStall` | critical | No poll in 120 s |
| `HighRPCErrorRate` | critical | RPC error rate > 5% over 5 min |
| `DBPoolExhaustion` | critical | Connection pool at maximum for 1 min |
| `HighHTTPErrorRate` | critical | HTTP 5xx rate > 1% over 5 min |
| `P99LatencySLOBreach` | critical | p99 latency > 200 ms SLO |
| `IndexerRPCErrors` | warning | RPC error rate > 0.1 err/s |
| `IndexerNoEventsIndexed` | warning | No events indexed for 15 min |
| `HTTPRequestLatencyHigh` | warning | p95 latency > 1 s |
| `PodMemoryNearLimit` | warning | Memory > 90% of 512 Mi limit |
| `UnusedIndexesDetected` | warning | Unused DB indexes detected for 24 h |
| `MatviewRefreshTimeout` | warning | Matview refresh lock timeout |
| `NotificationDeliverySLABreach` | critical | Notification p95 > 30 s |
| `NotificationDeliveryLatencyHigh` | warning | Notification p99 > 60 s |
| `ReplicaLagHigh` | warning | Replica replay lag > 30 s |
| `ReplicaLagCritical` | critical | Replica replay lag > 120 s |
| `ReplicaLagBytes` | warning | Replica WAL lag > 100 MiB |
| `ReplicaDown` | critical | Zero streaming replicas connected |
| `SLOBudgetBurnRateFast` | critical | Burn rate > 14.4× |
| `SLOBudgetBurnRateSlow` | warning | Burn rate > 6× |
| `SLOErrorBudgetLow` | warning | Error budget < 10% remaining |
| `SLOCompletionBelowTarget` | warning | Completion ratio < 95% |
| `SLOSeverelyBreached` | critical | Completion ratio < 50% |
| `FeatureFlagAutoRollback` | warning | Feature flag auto-rollback triggered |
| `HighErrorRateRollbackRisk` | warning | Error rate approaching rollback threshold |

### Grafana dashboard ConfigMap

The chart bundles the pre-built Grafana dashboard (`dashboards/soroban-pulse-dashboard.json`) and can create a `ConfigMap` that the [Grafana sidecar](https://github.com/grafana/helm-charts/tree/main/charts/grafana#sidecar-for-dashboards) watches. When the sidecar detects a ConfigMap with the configured label it automatically loads the dashboard without requiring a Grafana restart.

| Value | Default | Description |
|---|---|---|
| `grafana.dashboardConfigMap.enabled` | `false` | Create the ConfigMap |
| `grafana.dashboardConfigMap.namespace` | `""` | Namespace to deploy into (defaults to release namespace) |
| `grafana.dashboardConfigMap.sidecarLabel` | `grafana_dashboard` | Label key the Grafana sidecar watches |
| `grafana.dashboardConfigMap.sidecarLabelValue` | `"1"` | Label value |
| `grafana.dashboardConfigMap.additionalLabels` | `{}` | Extra labels added to the ConfigMap |
| `grafana.dashboardConfigMap.annotations` | `{}` | Annotations — use `grafana_dashboard_folder` to place the dashboard in a subfolder |

```yaml
grafana:
  dashboardConfigMap:
    enabled: true
    # Put the dashboard in a named folder inside Grafana
    annotations:
      grafana_dashboard_folder: SorobanPulse
```

> The `sidecarLabel` / `sidecarLabelValue` must match `grafana.sidecar.dashboards.label` /
> `grafana.sidecar.dashboards.labelValue` in your Grafana Helm release. The defaults
> (`grafana_dashboard: "1"`) match the upstream `grafana` chart defaults.

**Manual import (no sidecar):** In Grafana go to **Dashboards → Import**, upload
`docs/grafana-dashboard.json`, select your Prometheus datasource, and click **Import**.

## Network Policy

The chart can create a `NetworkPolicy` that enforces traffic isolation at the pod level. Requires a CNI plugin that enforces `NetworkPolicy` resources (Calico, Cilium, Weave Net, etc.).

**What it does:**

- **Ingress** — allows inbound traffic only from the ingress controller pods and Prometheus pods on the service port (`targetPort`, default `3000`). All other inbound traffic is denied.
- **Egress** — allows outbound traffic only to PostgreSQL (default port `5432`), the Stellar Soroban RPC endpoint over HTTPS (default port `443`), and DNS (UDP/TCP port `53`). All other outbound traffic is denied.

| Value | Default | Description |
|---|---|---|
| `networkPolicy.enabled` | `false` | Create the NetworkPolicy resource |
| `networkPolicy.ingressController.namespaceSelector` | `{matchLabels: {kubernetes.io/metadata.name: ingress-nginx}}` | Namespace selector for the ingress controller |
| `networkPolicy.ingressController.podSelector` | `{matchLabels: {app.kubernetes.io/name: ingress-nginx}}` | Pod selector for the ingress controller |
| `networkPolicy.prometheus.namespaceSelector` | `{matchLabels: {kubernetes.io/metadata.name: monitoring}}` | Namespace selector for Prometheus |
| `networkPolicy.prometheus.podSelector` | `{matchLabels: {app.kubernetes.io/name: prometheus}}` | Pod selector for Prometheus |
| `networkPolicy.egress.postgresPort` | `5432` | PostgreSQL port |
| `networkPolicy.egress.rpcPort` | `443` | Stellar RPC HTTPS port |
| `networkPolicy.egress.extraPorts` | `[]` | Additional egress ports (e.g. SMTP) |

```yaml
networkPolicy:
  enabled: true
  # Adjust to match your ingress controller (below shows ingress-nginx in its own namespace)
  ingressController:
    namespaceSelector:
      matchLabels:
        kubernetes.io/metadata.name: ingress-nginx
    podSelector:
      matchLabels:
        app.kubernetes.io/name: ingress-nginx
  # Adjust to match your Prometheus installation
  prometheus:
    namespaceSelector:
      matchLabels:
        kubernetes.io/metadata.name: monitoring
    podSelector:
      matchLabels:
        app.kubernetes.io/name: prometheus
  egress:
    postgresPort: 5432
    rpcPort: 443
    # Allow SMTP if email notifications are enabled
    extraPorts:
      - port: 587
        protocol: TCP
```

> **Note:** If your Postgres or RPC endpoint has a non-standard port, update `postgresPort` / `rpcPort` accordingly. Use `extraPorts` for any other external service the pod needs to reach (SMTP, Slack webhooks via port 443 is already covered by `rpcPort`).
