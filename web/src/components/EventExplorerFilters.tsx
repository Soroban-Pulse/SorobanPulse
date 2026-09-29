import { useRef } from "react";
import type { ExplorerParams } from "../pages/EventsPage.tsx";
import type { EventType } from "../api/hooks.ts";

interface Props {
  params: ExplorerParams;
  hasActiveFilters: boolean;
  onChange: (key: keyof ExplorerParams, value: string) => void;
  onReset: () => void;
}

/**
 * Filter bar for the Event Explorer.
 *
 * Renders six filters in a responsive flex row:
 *   Contract ID | Event type | From ledger | To ledger | Topic 0 | Full-text search
 *
 * Each input is controlled via URL search params (lifted to the parent).
 * Pressing Enter in any text input triggers no extra action — the parent
 * query re-fires automatically on every keystroke via React state.
 *
 * The "Clear filters" button resets all filter fields while preserving
 * column-visibility and density preferences.
 */
export function EventExplorerFilters({ params, hasActiveFilters, onChange, onReset }: Props) {
  const contractRef = useRef<HTMLInputElement>(null);

  return (
    <div className="explorer-filters card" role="search" aria-label="Event filters">
      <div className="explorer-filters__row">

        {/* Contract ID */}
        <div className="explorer-filters__field explorer-filters__field--wide">
          <label className="explorer-filters__label" htmlFor="ef-contract">
            Contract ID
          </label>
          <input
            ref={contractRef}
            id="ef-contract"
            type="text"
            className="explorer-filters__input"
            placeholder="C…"
            value={params.contract_id}
            onChange={(e) => onChange("contract_id", e.target.value)}
            autoComplete="off"
            spellCheck={false}
            aria-label="Filter by contract ID"
          />
        </div>

        {/* Event type */}
        <div className="explorer-filters__field explorer-filters__field--narrow">
          <label className="explorer-filters__label" htmlFor="ef-type">
            Type
          </label>
          <select
            id="ef-type"
            className="explorer-filters__select"
            value={params.event_type}
            onChange={(e) => onChange("event_type", e.target.value as EventType | "")}
            aria-label="Filter by event type"
          >
            <option value="">All types</option>
            <option value="contract">contract</option>
            <option value="diagnostic">diagnostic</option>
            <option value="system">system</option>
          </select>
        </div>

        {/* From ledger */}
        <div className="explorer-filters__field explorer-filters__field--narrow">
          <label className="explorer-filters__label" htmlFor="ef-from-ledger">
            From ledger
          </label>
          <input
            id="ef-from-ledger"
            type="number"
            min={0}
            step={1}
            className="explorer-filters__input"
            placeholder="0"
            value={params.from_ledger}
            onChange={(e) => onChange("from_ledger", e.target.value)}
            aria-label="From ledger (inclusive)"
          />
        </div>

        {/* To ledger */}
        <div className="explorer-filters__field explorer-filters__field--narrow">
          <label className="explorer-filters__label" htmlFor="ef-to-ledger">
            To ledger
          </label>
          <input
            id="ef-to-ledger"
            type="number"
            min={0}
            step={1}
            className="explorer-filters__input"
            placeholder="latest"
            value={params.to_ledger}
            onChange={(e) => onChange("to_ledger", e.target.value)}
            aria-label="To ledger (inclusive)"
          />
        </div>

        {/* Topic 0 */}
        <div className="explorer-filters__field">
          <label className="explorer-filters__label" htmlFor="ef-topic">
            Topic 0
          </label>
          <input
            id="ef-topic"
            type="text"
            className="explorer-filters__input"
            placeholder="e.g. transfer"
            value={params.topic_sym}
            onChange={(e) => onChange("topic_sym", e.target.value)}
            autoComplete="off"
            spellCheck={false}
            aria-label="Filter by first topic symbol"
          />
        </div>

        {/* Full-text search */}
        <div className="explorer-filters__field explorer-filters__field--wide">
          <label className="explorer-filters__label" htmlFor="ef-search">
            Search
          </label>
          <input
            id="ef-search"
            type="search"
            className="explorer-filters__input"
            placeholder="Search event data…"
            value={params.search}
            onChange={(e) => onChange("search", e.target.value)}
            aria-label="Full-text search in event data"
          />
        </div>

        {/* Clear button — only shown when filters are active */}
        {hasActiveFilters && (
          <div className="explorer-filters__field explorer-filters__field--action">
            <span className="explorer-filters__label" aria-hidden="true">&nbsp;</span>
            <button
              className="btn explorer-filters__clear-btn"
              onClick={onReset}
              aria-label="Clear all filters"
            >
              ✕ Clear
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
