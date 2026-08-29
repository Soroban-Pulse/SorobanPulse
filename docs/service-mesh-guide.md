# Service Mesh Integration Guide

## Overview

SorobanPulse supports integration with modern service mesh platforms to provide advanced traffic management, security, and observability features. This guide covers deployment with both Istio and Linkerd.

## Supported Service Meshes

### Istio
- **Version**: 1.20+
- **Features**: Full-featured service mesh with extensive customization
- **Best For**: Large-scale deployments, complex traffic management

### Linkerd
- **Version**: 2.14+
- **Features**: Lightweight, easy to use, minimal resource overhead
- **Best For**: Simplicity, performance-critical workloads

## Benefits

### Traffic Management
- **Circuit Breaking**: Automatic failure detection and isolation
- **Retries**: Intelligent retry policies for transient failures
- **Timeouts**: Configurable request timeouts
- **Load Balancing**: Advanced load balancing algorithms
- **Traffic Splitting**: Canary deployments and A/B testing

### Security
- **mTLS**: Automatic mutual TLS between services
- **Authorization**: Fine-grained access control policies
- **Certificate Management**: Automatic certificate rotation
- **Encryption**: End-to-end encryption in transit

### Observability
- **Distributed Tracing**: Request flow visualization
- **Metrics**: Rich telemetry data
- **Service Graph**: Visual service topology
- **Traffic Analysis**: Real-time traffic monitoring

## Istio Deployment

### Prerequisites

```bash
# Install Istio CLI
curl -L https://istio.io/downloadIstio | sh -
cd istio-*
export PATH=$PWD/bin:$PATH

# Verify installation
istioctl version
```

### Install Istio

```bash
# Install Istio with demo profile
istioctl install --set profile=demo -y

# Enable automatic sidecar injection
kubectl label namespace default istio-injection=enabled

# Verify installation
kubectl get pods -n istio-system
```

### Deploy SorobanPulse with Istio

```bash
# Apply Istio configurations
kubectl apply -f k8s/istio-gateway.yaml
kubectl apply -f k8s/istio-virtualservice.yaml
kubectl apply -f k8s/istio-destinationrule.yaml
kubectl apply -f k8s/istio-peerauthentication.yaml

# Deploy application
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml

# Verify mesh injection
kubectl get pods -l app=soroban-pulse -o jsonpath='{.items[0].spec.containers[*].name}'
# Should show: soroban-pulse istio-proxy
```

### Configuration Examples

#### Circuit Breaking

```yaml
# Automatic failure detection
apiVersion: networking.istio.io/v1beta1
kind: DestinationRule
metadata:
  name: soroban-pulse-circuit-breaker
spec:
  host: soroban-pulse-service
  trafficPolicy:
    outlierDetection:
      consecutive5xxErrors: 5
      interval: 30s
      baseEjectionTime: 30s
      maxEjectionPercent: 50
```

#### Retry Policy

```yaml
# Intelligent retries
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: soroban-pulse-retries
spec:
  http:
  - retries:
      attempts: 3
      perTryTimeout: 2s
      retryOn: 5xx,reset,connect-failure
```

#### Canary Deployment

```yaml
# Traffic splitting for canary
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: soroban-pulse-canary
spec:
  http:
  - match:
    - headers:
        canary:
          exact: "true"
    route:
    - destination:
        host: soroban-pulse-service
        subset: canary
      weight: 100
  - route:
    - destination:
        host: soroban-pulse-service
        subset: stable
      weight: 90
    - destination:
        host: soroban-pulse-service
        subset: canary
      weight: 10
```

### Monitoring with Kiali

```bash
# Install Kiali
kubectl apply -f k8s/observability-kiali.yaml

# Access Kiali dashboard
kubectl port-forward svc/kiali 20001:20001 -n observability

# Open browser
open http://localhost:20001
```

## Linkerd Deployment

### Prerequisites

```bash
# Install Linkerd CLI
curl -fsL https://run.linkerd.io/install | sh
export PATH=$PATH:$HOME/.linkerd2/bin

# Verify installation
linkerd version
```

### Install Linkerd

```bash
# Pre-flight check
linkerd check --pre

# Install Linkerd CRDs
linkerd install --crds | kubectl apply -f -

# Install Linkerd control plane
linkerd install | kubectl apply -f -

# Verify installation
linkerd check
```

### Deploy SorobanPulse with Linkerd

```bash
# Inject Linkerd proxy
kubectl get deploy soroban-pulse -o yaml \
  | linkerd inject - \
  | kubectl apply -f -

# Apply Linkerd configurations
kubectl apply -f k8s/linkerd-serviceprofile.yaml
kubectl apply -f k8s/linkerd-trafficsplit.yaml
kubectl apply -f k8s/linkerd-server.yaml

# Verify mesh injection
kubectl get pods -l app=soroban-pulse -o jsonpath='{.items[0].spec.containers[*].name}'
# Should show: soroban-pulse linkerd-proxy
```

### Configuration Examples

#### Service Profile

```yaml
# Per-route metrics and retries
apiVersion: linkerd.io/v1alpha2
kind: ServiceProfile
metadata:
  name: soroban-pulse-service.default.svc.cluster.local
spec:
  routes:
  - name: get_ledger
    condition:
      method: GET
      pathRegex: /api/v1/ledgers/.*
    timeout: 10s
    isRetryable: true
```

#### Traffic Split

```yaml
# Canary deployment
apiVersion: split.smi-spec.io/v1alpha1
kind: TrafficSplit
metadata:
  name: soroban-pulse-canary
spec:
  service: soroban-pulse-service
  backends:
  - service: soroban-pulse-stable
    weight: 90
  - service: soroban-pulse-canary
    weight: 10
```

### Monitoring with Linkerd Viz

```bash
# Install Linkerd Viz extension
linkerd viz install | kubectl apply -f -

# Access dashboard
linkerd viz dashboard

# View metrics
linkerd viz stat deploy/soroban-pulse

# View live traffic
linkerd viz tap deploy/soroban-pulse
```

## Distributed Tracing

### Jaeger Integration

```bash
# Deploy Jaeger
kubectl apply -f k8s/observability-jaeger.yaml

# Configure application to send traces
export JAEGER_ENDPOINT=http://jaeger-collector.observability:14268/api/traces

# Access Jaeger UI
kubectl port-forward svc/jaeger-query 16686:16686 -n observability
open http://localhost:16686
```

### OpenTelemetry

```bash
# Deploy OpenTelemetry Collector
kubectl apply -f k8s/observability-telemetry.yaml

# Configure application
export OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector.observability:4317
export OTEL_SERVICE_NAME=soroban-pulse
```

## Performance Tuning

### Resource Limits

```yaml
# Sidecar resource configuration
spec:
  containers:
  - name: istio-proxy  # or linkerd-proxy
    resources:
      requests:
        cpu: 100m
        memory: 128Mi
      limits:
        cpu: 2000m
        memory: 1024Mi
```

### Connection Pooling

```yaml
# Optimize connection pooling
trafficPolicy:
  connectionPool:
    tcp:
      maxConnections: 1000
    http:
      http1MaxPendingRequests: 1024
      http2MaxRequests: 1024
      maxRequestsPerConnection: 10
```

## Security Hardening

### mTLS Configuration

```bash
# Verify mTLS is enabled
istioctl authn tls-check soroban-pulse-pod.default

# Should show: STATUS: AUTO_MTLS
```

### Authorization Policies

```yaml
# Restrict access to specific services
apiVersion: security.istio.io/v1beta1
kind: AuthorizationPolicy
metadata:
  name: soroban-pulse-authz
spec:
  selector:
    matchLabels:
      app: soroban-pulse
  rules:
  - from:
    - source:
        principals: ["cluster.local/ns/default/sa/allowed-service"]
    to:
    - operation:
        methods: ["GET", "POST"]
        paths: ["/api/*"]
```

## Troubleshooting

### Common Issues

**Issue**: Pods not starting after mesh injection
```bash
# Check sidecar logs
kubectl logs pod-name -c istio-proxy
kubectl logs pod-name -c linkerd-proxy

# Verify mesh installation
istioctl analyze
linkerd check
```

**Issue**: High latency after mesh deployment
```bash
# Check proxy resource usage
kubectl top pods

# Review traffic policies
kubectl describe virtualservice soroban-pulse-vs
kubectl describe serviceprofile soroban-pulse-service
```

**Issue**: mTLS connection failures
```bash
# Verify certificates
istioctl proxy-config secret pod-name

# Check peer authentication
kubectl get peerauthentication
```

## Migration Guide

### From No Mesh to Service Mesh

1. **Preparation**
   - Review current architecture
   - Identify critical services
   - Plan rollout strategy

2. **Install Service Mesh**
   - Choose Istio or Linkerd
   - Install control plane
   - Verify installation

3. **Gradual Rollout**
   - Start with non-critical services
   - Enable sidecar injection per namespace
   - Monitor metrics and logs

4. **Enable Features**
   - Start with observability only
   - Gradually enable mTLS
   - Add traffic management policies

5. **Production Rollout**
   - Apply to production namespace
   - Monitor closely
   - Have rollback plan ready

## Best Practices

1. **Start Simple**: Begin with basic features, add complexity gradually
2. **Monitor Everything**: Use built-in observability tools extensively
3. **Test Thoroughly**: Test mesh features in staging before production
4. **Plan for Failure**: Have rollback procedures documented
5. **Resource Planning**: Account for sidecar overhead in capacity planning
6. **Security First**: Enable mTLS and authorization policies early
7. **Documentation**: Document all custom configurations and policies

## Resources

- [Istio Documentation](https://istio.io/latest/docs/)
- [Linkerd Documentation](https://linkerd.io/2/overview/)
- [Service Mesh Comparison](https://servicemesh.es/)
- [SorobanPulse Training](../training/README.md)

## Support

For service mesh integration support:
- GitHub Issues: https://github.com/Soroban-Pulse/SorobanPulse/issues
- Community Slack: #service-mesh
- Email: support@soroban-pulse.example.com
