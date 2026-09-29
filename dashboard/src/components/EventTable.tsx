import { type Event } from "../api/eventTypes";
import { TruncatedText } from "./TruncatedText";

interface EventTableProps {
  events: Event[];
  onRowClick: (event: Event) => void;
  loading?: boolean;
}

export function EventTable({ events, onRowClick, loading = false }: EventTableProps) {
  if (loading) {
    return (
      <div className="event-table-loading">
        <p>Loading events…</p>
      </div>
    );
  }

  if (events.length === 0) {
    return null;
  }

  return (
    <div className="event-table-wrapper">
      <table className="event-table" role="grid" aria-label="Events">
        <thead>
          <tr>
            <th scope="col">Ledger</th>
            <th scope="col">Tx Hash</th>
            <th scope="col">Contract</th>
            <th scope="col">Type</th>
            <th scope="col">Timestamp</th>
          </tr>
        </thead>
        <tbody>
          {events.map((event) => (
            <tr
              key={event.id}
              className="event-table-row"
              onClick={() => onRowClick(event)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onRowClick(event);
                }
              }}
            >
              <td>{event.ledger.toLocaleString()}</td>
              <td>
                <TruncatedText text={event.txHash} maxChars={16} />
              </td>
              <td>
                <TruncatedText text={event.contractId} maxChars={12} />
              </td>
              <td>
                <span className={`event-type-badge event-type-badge--${event.eventType}`}>
                  {event.eventType}
                </span>
              </td>
              <td>{new Date(event.timestamp).toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
