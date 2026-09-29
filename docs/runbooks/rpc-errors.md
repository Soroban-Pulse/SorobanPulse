# RPC Errors Runbook

## Symptom
The Soroban Pulse indexer is experiencing a high rate of errors when calling the Soroban RPC endpoint. The `soroban_pulse_rpc_errors_total` metric is increasing rapidly, or the error rate exceeds 5% of total RPC calls.

## Likely Causes
1. **RPC endpoint downtime or degradation**: The Soroban RPC service is unavailable or responding slowly
2. **Network connectivity issues**: Network partition or high latency between indexer and RPC
3. **Rate limiting**: The RPC endpoint is rate-limiting requests from the indexer
4. **Invalid RPC configuration**: Incorrect `STELLAR_RPC_URL` or authentication issues
5. **RPC API changes**: Breaking changes in the RPC API that the indexer doesn't handle

## Investigation Steps

### 1. Check RPC endpoint status
```bash
# Test basic connectivity
curl -s https://soroban-testnet.stellar.org/health | jq .

# Check RPC version
curl -s -X POST https://soroban-testnet.stellar.org \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getVersion"}' | jq .
```

### 2. Review indexer logs for specific errors
```bash
kubectl logs -l app=soroban-pulse -c soroban-pulse --tail=200 | grep -i "rpc\|error" | tail -50
```

### 3. Check RPC error rate and types
```bash
# Error rate over last 5 minutes
promtool query instant 'rate(soroban_pulse_rpc_errors_total[5m])'

# Errors by type (if available)
promtool query instant 'soroban_pulse_rpc_errors_total'
```

### 4. Verify RPC configuration
```bash
kubectl get configmap soroban-pulse-config -o yaml | grep STELLAR_RPC_URL
```

### 5. Check network connectivity
```bash
# From the indexer pod
kubectl exec -it <pod-name> -- bash
curl -v https://soroban-testnet.stellar.org/health
```

## Resolution Steps

### If RPC endpoint is down
- Wait for the RPC endpoint to recover
- Switch to a backup RPC endpoint by updating `STELLAR_RPC_URL`
- Update the ConfigMap and restart the indexer pod

### If rate limiting is occurring
- Reduce the indexer poll frequency by increasing `INDEXER_POLL_INTERVAL_SECS`
- Contact the RPC provider to increase rate limits
- Implement exponential backoff in the indexer (if not already present)

### If network connectivity is the issue
- Check firewall rules and security groups
- Verify DNS resolution of the RPC endpoint
- Check for network latency with `ping` or `traceroute`

### If RPC API has changed
- Update the indexer to handle the new API response format
- Check the Stellar documentation for API changes
- Review the indexer code for hardcoded assumptions about RPC responses

## Prevention
- Monitor RPC endpoint health proactively
- Set up alerts for RPC error rate and latency
- Use multiple RPC endpoints with automatic failover
- Implement circuit breakers to fail fast on persistent RPC errors
- Test RPC endpoint changes in staging before production

## Protocol upgrades and version mismatches

SorobanPulse calls `getVersionInfo` periodically and logs `protocol_version`, `version` and
`commit_hash`. The result is exposed as `rpc_version` on `/v1/status` and as the
`soroban_pulse_rpc_info` Prometheus metric. Supported protocol range is defined by
`SUPPORTED_PROTOCOL_MIN`/`MAX` in `src/rpc_meta.rs`.

- WARN "RPC protocol N is outside supported range": upgrade SorobanPulse to a release that
  supports the new protocol, or point `STELLAR_RPC_URL` at a compatible RPC.
- Set `RPC_PROTOCOL_STRICT=true` to fail negotiation (logged as ERROR) on unsupported protocols.

## Retention gaps

If the checkpoint is older than the RPC's `oldestLedger`, an ERROR is logged, the
`soroban_pulse_indexer_gap_ledgers` gauge is set, and the range is stored in `indexer_gaps`
(visible in `/v1/status` and `GET /v1/admin/indexer/gaps`). Fill it with the backfill job
(see `docs/backfill.md`).
