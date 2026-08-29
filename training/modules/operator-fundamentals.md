# Operator Fundamentals - Training Module

**Duration**: 2-3 days  
**Level**: Beginner  
**Certification**: SorobanPulse Certified Operator (SPCO)

## Learning Objectives

By the end of this module, you will be able to:
- Deploy SorobanPulse on Kubernetes
- Configure basic settings and parameters
- Monitor system health and performance
- Perform routine maintenance tasks
- Troubleshoot common issues
- Implement backup and recovery procedures

## Prerequisites

- Basic Linux command line knowledge
- Understanding of Docker containers
- Familiarity with Kubernetes concepts
- Basic understanding of databases (PostgreSQL)

## Module Content

### Day 1: Introduction & Architecture

#### Session 1: Overview (2 hours)
- What is SorobanPulse?
- Use cases and benefits
- Architecture components
- Data flow and processing

#### Session 2: Installation (3 hours)
- System requirements
- Kubernetes cluster setup
- Deploying with Helm charts
- Deploying with kubectl
- Configuration files overview

**Lab 1**: Deploy SorobanPulse on local Kubernetes cluster (minikube/kind)

#### Session 3: Configuration (2 hours)
- Environment variables
- ConfigMaps and Secrets
- Database connection settings
- Stellar network configuration
- API settings

**Lab 2**: Configure SorobanPulse for test environment

### Day 2: Operations & Monitoring

#### Session 4: Monitoring Basics (2 hours)
- Health checks and readiness probes
- Prometheus metrics
- Grafana dashboards
- Alert configuration
- Log aggregation

**Lab 3**: Set up monitoring dashboards

#### Session 5: Database Operations (3 hours)
- PostgreSQL administration
- Connection pooling
- Query performance
- Index maintenance
- Backup strategies

**Lab 4**: Perform database maintenance tasks

#### Session 6: Scaling & Performance (2 hours)
- Horizontal pod autoscaling
- Resource requests and limits
- Load balancing
- Performance tuning
- Capacity planning

**Lab 5**: Configure autoscaling policies

### Day 3: Maintenance & Troubleshooting

#### Session 7: Routine Maintenance (2 hours)
- Rolling updates
- Configuration changes
- Log rotation
- Certificate renewal
- Data retention policies

**Lab 6**: Perform rolling update

#### Session 8: Backup & Recovery (2 hours)
- Backup strategies
- Database backups
- Point-in-time recovery
- Disaster recovery planning
- Testing recovery procedures

**Lab 7**: Create and restore from backup

#### Session 9: Troubleshooting (3 hours)
- Common issues and solutions
- Log analysis
- Performance debugging
- Network connectivity issues
- Pod failure scenarios

**Lab 8**: Troubleshooting scenarios

## Hands-On Labs

### Lab 1: Deploy SorobanPulse
```bash
# Create namespace
kubectl create namespace soroban-pulse

# Deploy with Helm
helm repo add soroban-pulse https://charts.soroban-pulse.example.com
helm install soroban-pulse soroban-pulse/soroban-pulse \
  --namespace soroban-pulse \
  --set postgresql.enabled=true

# Verify deployment
kubectl get pods -n soroban-pulse
kubectl logs -f deployment/soroban-pulse -n soroban-pulse
```

### Lab 2: Configure Environment
```yaml
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: soroban-pulse-config
  namespace: soroban-pulse
data:
  STELLAR_NETWORK: "testnet"
  LOG_LEVEL: "info"
  API_PORT: "8080"
```

### Lab 3: Set Up Monitoring
```bash
# Install Prometheus and Grafana
helm install prometheus prometheus-community/kube-prometheus-stack

# Import SorobanPulse dashboard
kubectl apply -f monitoring/grafana-dashboard.json
```

### Lab 4: Database Maintenance
```sql
-- Check database size
SELECT pg_size_pretty(pg_database_size('soroban_pulse'));

-- Analyze tables
ANALYZE ledgers;
ANALYZE transactions;

-- Reindex if needed
REINDEX TABLE ledgers;
```

### Lab 5: Configure Autoscaling
```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: soroban-pulse-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: soroban-pulse
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
```

### Lab 6: Rolling Update
```bash
# Update image version
kubectl set image deployment/soroban-pulse \
  soroban-pulse=soroban-pulse:v1.2.0 \
  -n soroban-pulse

# Watch rollout
kubectl rollout status deployment/soroban-pulse -n soroban-pulse

# Rollback if needed
kubectl rollout undo deployment/soroban-pulse -n soroban-pulse
```

### Lab 7: Backup & Restore
```bash
# Create database backup
kubectl exec -it postgresql-0 -n soroban-pulse -- \
  pg_dump -U postgres soroban_pulse > backup.sql

# Restore from backup
kubectl exec -i postgresql-0 -n soroban-pulse -- \
  psql -U postgres soroban_pulse < backup.sql
```

### Lab 8: Troubleshooting Exercise
**Scenario**: Application pods are crashing repeatedly

**Tasks**:
1. Check pod status and events
2. Review logs for errors
3. Verify resource limits
4. Check database connectivity
5. Identify and fix root cause

## Assessment

### Written Exam Topics
- Architecture and components (20%)
- Deployment procedures (20%)
- Configuration management (15%)
- Monitoring and alerting (15%)
- Database operations (15%)
- Troubleshooting (15%)

### Practical Assessment
- Deploy SorobanPulse from scratch
- Configure monitoring
- Perform backup and recovery
- Troubleshoot simulated issues
- Document procedures

## Study Resources

### Required Reading
- SorobanPulse Architecture Guide
- Deployment Documentation
- Operations Runbook
- Kubernetes Best Practices

### Recommended Reading
- PostgreSQL Administration Handbook
- Prometheus Monitoring Guide
- Grafana Dashboard Design
- Kubernetes Troubleshooting Guide

### Video Tutorials
- Installation walkthrough (30 min)
- Configuration deep dive (45 min)
- Monitoring setup (40 min)
- Troubleshooting techniques (60 min)

## Exam Preparation

### Sample Questions

1. **Which component is responsible for ingesting blockchain data?**
   - A) API Server
   - B) Event Processor
   - C) Ingestion Service
   - D) Database

2. **What is the recommended minimum replica count for production?**
   - A) 1
   - B) 2
   - C) 3
   - D) 5

3. **Which metric indicates database connection issues?**
   - A) http_requests_total
   - B) db_connection_errors
   - C) pod_cpu_usage
   - D) api_response_time

### Practice Scenarios
1. High CPU usage troubleshooting
2. Database connection pool exhaustion
3. Failed rolling update recovery
4. Monitoring alert configuration
5. Backup verification

## Next Steps

After completing this module and earning SPCO certification:
- Move to Developer Track for API integration skills
- Specialize in Security & Compliance
- Gain real-world operational experience
- Join the SorobanPulse operators community

## Support & Resources

- Lab Environment: https://labs.soroban-pulse.example.com
- Discussion Forum: https://forum.soroban-pulse.example.com/operators
- Instructor Office Hours: Tuesdays & Thursdays, 2-4 PM UTC
- Slack Channel: #training-operators
