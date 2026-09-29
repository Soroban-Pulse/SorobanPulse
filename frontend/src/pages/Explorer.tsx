import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { EventList } from "../components/EventList";

/**
 * Event explorer. Supports `?ledger=N` (from global search ledger routing),
 * `?search=text` (full-text search over event_data), `?contract_id=` and
 * `?event_type=`.
 */
export function Explorer() {
  const [params, setParams] = useSearchParams();
  const ledger = params.get("ledger") ?? "";
  const search = params.get("search") ?? "";
  const contractId = params.get("contract_id") ?? "";
  const eventType = params.get("event_type") ?? "";
  const [draft, setDraft] = useState({ ledger, search, contractId, eventType });
  // Re-sync the form when the URL changes (e.g. a new global search).
  useEffect(() => setDraft({ ledger, search, contractId, eventType }), [ledger, search, contractId, eventType]);

  const apply = (e: React.FormEvent) => {
    e.preventDefault();
    const next = new URLSearchParams();
    if (draft.ledger.trim()) next.set("ledger", draft.ledger.trim());
    if (draft.search.trim()) next.set("search", draft.search.trim());
    if (draft.contractId.trim()) next.set("contract_id", draft.contractId.trim());
    if (draft.eventType) next.set("event_type", draft.eventType);
    setParams(next);
  };

  const title = ledger
    ? `Events in ledger ${Number(ledger).toLocaleString()}`
    : search
      ? `Search results for “${search}”`
      : "Event explorer";

  return (
    <div className="stack">
      <div className="page-head">
        <h1>{title}</h1>
      </div>
      <form className="card row" onSubmit={apply} key={params.toString()}>
        <label className="field">
          Ledger
          <input
            inputMode="numeric"
            defaultValue={ledger}
            onChange={(e) => setDraft({ ...draft, ledger: e.target.value })}
            style={{ width: 130 }}
          />
        </label>
        <label className="field" style={{ flex: 1, minWidth: 180 }}>
          Full-text search
          <input defaultValue={search} onChange={(e) => setDraft({ ...draft, search: e.target.value })} />
        </label>
        <label className="field" style={{ flex: 1, minWidth: 180 }}>
          Contract ID
          <input
            className="mono"
            defaultValue={contractId}
            onChange={(e) => setDraft({ ...draft, contractId: e.target.value })}
          />
        </label>
        <label className="field">
          Type
          <select defaultValue={eventType} onChange={(e) => setDraft({ ...draft, eventType: e.target.value })}>
            <option value="">All</option>
            <option value="contract">contract</option>
            <option value="diagnostic">diagnostic</option>
            <option value="system">system</option>
          </select>
        </label>
        <button type="submit" className="primary">
          Apply
        </button>
      </form>
      <div className="card">
        <EventList
          path="/v1/events"
          params={{
            from_ledger: ledger || undefined,
            to_ledger: ledger || undefined,
            search: search || undefined,
            contract_id: contractId || undefined,
            event_type: eventType || undefined,
            sort: "desc",
          }}
        />
      </div>
    </div>
  );
}
