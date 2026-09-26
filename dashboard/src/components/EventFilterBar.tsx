import { useState, useEffect } from "react";
import { type EventFilterParams } from "../api/eventTypes";

interface EventFilterBarProps {
  filters: EventFilterParams;
  onChange: (filters: EventFilterParams) => void;
  onSearch: () => void;
}

export function EventFilterBar({ filters, onChange, onSearch }: EventFilterBarProps) {
  const [localFilters, setLocalFilters] = useState<EventFilterParams>(filters);

  // Sync with external filters on change
  useEffect(() => {
    setLocalFilters(filters);
  }, [filters]);

  const update = (key: string, value: string | number | boolean | undefined) => {
    const next = { ...localFilters, [key]: value };
    setLocalFilters(next as EventFilterParams);
    onChange(next as EventFilterParams);
  };

  const handleReset = () => {
    const reset: EventFilterParams = {};
    setLocalFilters(reset);
    onChange(reset);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      onSearch();
    }
  };

  return (
    <div className="event-filter-bar" role="search" aria-label="Event filters">
      <div className="event-filter-row">
        <div className="event-filter-group">
          <label htmlFor="filter-search" className="event-filter-label">
            Search
          </label>
          <input
            id="filter-search"
            type="text"
            className="event-filter-input"
            placeholder="Full-text search…"
            value={localFilters.search ?? ""}
            onChange={(e) => update("search", e.target.value || undefined)}
            onKeyDown={handleKeyDown}
          />
        </div>

        <div className="event-filter-group">
          <label htmlFor="filter-type" className="event-filter-label">
            Type
          </label>
          <select
            id="filter-type"
            className="event-filter-select"
            value={localFilters.eventType ?? ""}
            onChange={(e) =>
              update("eventType", e.target.value === "" ? undefined : (e.target.value as "contract" | "diagnostic" | "system"))
            }
          >
            <option value="">All</option>
            <option value="contract">Contract</option>
            <option value="diagnostic">Diagnostic</option>
            <option value="system">System</option>
          </select>
        </div>

        <div className="event-filter-group">
          <label htmlFor="filter-contract" className="event-filter-label">
            Contract
          </label>
          <input
            id="filter-contract"
            type="text"
            className="event-filter-input"
            placeholder="Contract ID (min 4 chars)"
            value={localFilters.contractId ?? ""}
            onChange={(e) => update("contractId", e.target.value || undefined)}
            onKeyDown={handleKeyDown}
          />
        </div>

        <div className="event-filter-group">
          <label htmlFor="filter-ledger-from" className="event-filter-label">
            Ledger
          </label>
          <div className="event-filter-range">
            <input
              id="filter-ledger-from"
              type="number"
              className="event-filter-input event-filter-input--range"
              placeholder="From"
              value={localFilters.fromLedger ?? ""}
              onChange={(e) =>
                update("fromLedger", e.target.value ? Number(e.target.value) : undefined)
              }
            />
            <span className="event-filter-range-sep">→</span>
            <input
              id="filter-ledger-to"
              type="number"
              className="event-filter-input event-filter-input--range"
              placeholder="To"
              value={localFilters.toLedger ?? ""}
              onChange={(e) =>
                update("toLedger", e.target.value ? Number(e.target.value) : undefined)
              }
            />
          </div>
        </div>

        <div className="event-filter-group">
          <label htmlFor="filter-topic" className="event-filter-label">
            Topic
          </label>
          <input
            id="filter-topic"
            type="text"
            className="event-filter-input"
            placeholder="Topic value"
            value={localFilters.topic ?? ""}
            onChange={(e) => update("topic", e.target.value || undefined)}
            onKeyDown={handleKeyDown}
          />
        </div>
      </div>

      <div className="event-filter-actions">
        <button className="btn btn-primary" onClick={onSearch}>
          Apply
        </button>
        <button className="btn btn-secondary" onClick={handleReset}>
          Reset
        </button>
      </div>
    </div>
  );
}
