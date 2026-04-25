# Kubernetes Probes

Soroban Pulse exposes dedicated health check endpoints for Kubernetes liveness and readiness probes. These endpoints allow Kubernetes to manage pod lifecycle and traffic routing automatically.

## Probe Endpoints

### `/healthz/live` — Liveness Probe

Indicates whether the process is running. This endpoint performs minimal checks and should always respond quickly.

- **Status 200**: Process is alive
- **Status 503**: Process is in an unrecoverable state (rare)

**Response:**
```json
{ "status": "alive" }
```

**Use case:** Kubernetes uses this to detect and restart pods that have entered a deadlock or stall state.

### `/healthz/ready` — Readiness Probe

Indicates whether the pod is ready to accept traffic. This endpoint checks:
- Database connectivity
- Indexer health (not stalled)

- **Status 200**: Pod is ready to serve traffic
- **Status 503**: Pod is not ready (database unreachable, indexer stalled, or migrations still running)

**Response:**
```json
{ "status": "ok", "db": "ok", "indexer": "ok" }
```

**Use case:** Kubernetes uses this to route traffic only to healthy pods and to delay traffic during startup (before migrations complete).

## Deployment Configuration

The `k8s/deployment.yaml` includes probe configurations:

```yaml
livenessProbe:
  httpGet:
    path: /healthz/live
    port: 3000
  initialDelaySeconds: 10
  periodSeconds: 10
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /healthz/ready
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 10
  failureThreshold: 3
```

### Configuration Explanation

**Liveness Probe:**
- `initialDelaySeconds: 10` — Waits 10 seconds after pod startup before first check. This allows the process to initialize.
- `periodSeconds: 10` — Checks every 10 seconds.
- `failureThreshold: 3` — Restarts the pod after 3 consecutive failures (30 seconds of downtime).

**Readiness Probe:**
- `initialDelaySeconds: 5` — Waits 5 seconds after pod startup before first check. This is shorter than liveness to detect readiness quickly.
- `periodSeconds: 10` — Checks every 10 seconds.
- `failureThreshold: 3` — Removes the pod from the load balancer after 3 consecutive failures.

## Startup Sequence

1. **Pod starts** (t=0s)
2. **Process initializes** (t=0-2s)
   - Database connection pool is created
   - Migrations run (guarded by advisory lock)
   - Indexer attempts to acquire lock
3. **Liveness probe first check** (t=10s)
   - Should pass (process is running)
4. **Readiness probe first check** (t=5s)
   - May fail if migrations are still running
   - Will pass once migrations complete and indexer is healthy
5. **Traffic routed** (once readiness probe passes)

## Indexer Stall Detection

The readiness probe considers the indexer "stalled" if no successful poll has completed within the configured timeout. This is controlled by:

```bash
# Environment variable (default: 120 seconds)
INDEXER_STALL_TIMEOUT_SECS=120
```

If the indexer is stalled:
- `/healthz/ready` returns 503
- Pod is removed from the load balancer
- Liveness probe continues to pass (process is still running)
- After `failureThreshold * periodSeconds` (30 seconds), the pod is restarted

## Customization

To adjust probe behavior for your environment:

1. **Increase `initialDelaySeconds` for slow migrations:**
   ```yaml
   readinessProbe:
     initialDelaySeconds: 30  # Allow more time for migrations
   ```

2. **Decrease `periodSeconds` for faster failure detection:**
   ```yaml
   livenessProbe:
     periodSeconds: 5  # Check more frequently
   ```

3. **Adjust `failureThreshold` for tolerance:**
   ```yaml
   readinessProbe:
     failureThreshold: 5  # Allow 5 failures before removing from LB
   ```

## Monitoring

Monitor probe failures in your Kubernetes cluster:

```bash
# View pod events
kubectl describe pod <pod-name>

# Check probe history
kubectl get events --sort-by='.lastTimestamp'

# View logs for probe-related restarts
kubectl logs <pod-name> --previous
```

## Backward Compatibility

The `/health` endpoint remains available as a backward-compatible alias and mirrors `/healthz/ready` semantics:

- **Status 200**: DB reachable and indexer not stalled
- **Status 503**: DB unreachable or indexer stalled

New deployments should use `/healthz/live` and `/healthz/ready` for clarity.
