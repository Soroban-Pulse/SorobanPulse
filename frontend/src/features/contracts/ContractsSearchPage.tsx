import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { request } from '../../api/client';
import type { ContractInfo, Paged } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { truncateMiddle } from '../../lib/format';
import { useAsync, useDebounced } from '../../lib/useAsync';
import { isContractId } from '../../lib/validation';

export function ContractsSearchPage() {
  const navigate = useNavigate();
  const [q, setQ] = useState('');
  const dq = useDebounced(q.trim());

  const results = useAsync(
    () =>
      // The server searches by contract ID prefix and requires at least 4 characters.
      dq.length >= 4
        ? request<Paged<ContractInfo>>('/v1/contracts/search', { query: { q: dq.toUpperCase(), limit: 20 } })
        : request<Paged<ContractInfo>>('/v1/contracts', { query: { limit: 20 } }),
    [dq],
  );

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (isContractId(q.trim())) navigate(`/contracts/${q.trim()}`);
  };

  const list = results.data?.data ?? [];

  return (
    <div className="page">
      <header className="page-header">
        <h1>Contracts</h1>
      </header>
      <form onSubmit={submit} className="filters" role="search">
        <input
          type="search"
          className="grow"
          placeholder="Paste a contract ID or type the first 4+ characters"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          aria-label="Contract ID or label"
          spellCheck={false}
          autoFocus
        />
        <button type="submit" className="btn btn-primary" disabled={!isContractId(q.trim())}>
          Open
        </button>
      </form>
      <section className="panel">
        <AsyncBoundary loading={results.loading} error={results.error} onRetry={results.reload} empty={list.length === 0} emptyMessage="No contracts found.">
          <ul className="link-list">
            {list.map((c) => (
              <li key={c.contract_id}>
                <Link to={`/contracts/${c.contract_id}`}>
                  <strong>{c.label || truncateMiddle(c.contract_id, 8)}</strong>
                  <span className="mono muted small">{c.contract_id}</span>
                </Link>
              </li>
            ))}
          </ul>
        </AsyncBoundary>
      </section>
    </div>
  );
}
