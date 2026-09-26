import { contractsApi } from '../../api/contracts';
import type { ContractInfo } from '../../api/types';
import { CopyButton } from '../../components/CopyButton';
import { useAsync } from '../../lib/useAsync';

export function ContractHeader({ contractId }: { contractId: string }) {
  // Label/metadata are optional niceties: never block the page on them.
  const info = useAsync(() => contractsApi.info(contractId).catch((): ContractInfo => ({ contract_id: contractId })), [contractId]);
  const c = info.data;
  const metadata = Object.entries(c?.metadata ?? {}).filter(([, v]) => v !== null && typeof v !== 'object');

  return (
    <header className="contract-header">
      <div className="contract-title">
        <h1>{c?.label || 'Contract'}</h1>
        {c?.sac_asset && (
          <span className="badge badge-info" title={c.sac_asset.issuer ? `Issuer ${c.sac_asset.issuer}` : 'Native asset'}>
            SAC · {c.sac_asset.code}
          </span>
        )}
      </div>
      <div className="contract-id-row">
        <code className="mono contract-id">{contractId}</code>
        <CopyButton value={contractId} label="Copy ID" />
      </div>
      {metadata.length > 0 && (
        <dl className="meta-list">
          {metadata.map(([k, v]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>{String(v)}</dd>
            </div>
          ))}
        </dl>
      )}
    </header>
  );
}
