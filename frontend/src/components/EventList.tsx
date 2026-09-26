import { Fragment, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { api, errorMessage, qs } from "../lib/api";
import { formatDateTime, formatNumber, shortId } from "../lib/format";
import type { EventPage, EventRecord } from "../lib/types";
import { JsonTree } from "./JsonTree";

type Params = Record<string, string | number | undefined>;

/**
 * Paginated event table. `path` is the list endpoint (e.g. /v1/events) and
 * `params` its filters; cursor pagination is handled here.
 */
export function EventList({ path, params, emptyText = "No events found." }: { path: string; params?: Params; emptyText?: string }) {
  const [events, setEvents] = useState<EventRecord[]>([]);
  const [cursor, setCursor] = useState<string | null | undefined>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const key = path + JSON.stringify(params ?? {});

  const load = async (after?: string) => {
    setLoading(true);
    setError(null);
    try {
      const page = await api<EventPage>(`${path}${qs({ limit: 25, ...params, cursor: after })}`);
      setEvents((prev) => (after ? [...prev, ...(page.data ?? [])] : page.data ?? []));
      setCursor(page.next_cursor);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setEvents([]);
    setCursor(undefined);
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  if (error && !events.length) return <div className="notice danger">{error}</div>;
  if (!loading && !events.length) return <div className="empty">{emptyText}</div>;

  return (
    <div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Ledger</th>
              <th>Time</th>
              <th>Type</th>
              <th>Contract</th>
              <th>Transaction</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {events.map((ev) => (
              <Fragment key={ev.id}>
                <tr>
                  <td>
                    <Link to={`/explorer?ledger=${ev.ledger}`}>{formatNumber(ev.ledger)}</Link>
                  </td>
                  <td className="muted">{formatDateTime(ev.timestamp)}</td>
                  <td>
                    <span className="badge neutral">{ev.event_type}</span>
                  </td>
                  <td className="mono">
                    <Link to={`/contracts/${ev.contract_id}`} title={ev.contract_id}>
                      {shortId(ev.contract_id)}
                    </Link>
                  </td>
                  <td className="mono">
                    <Link to={`/tx/${ev.tx_hash}`} title={ev.tx_hash}>
                      {shortId(ev.tx_hash)}
                    </Link>
                  </td>
                  <td>
                    <button
                      type="button"
                      className="ghost"
                      aria-expanded={expanded === ev.id}
                      onClick={() => setExpanded(expanded === ev.id ? null : ev.id)}
                    >
                      {expanded === ev.id ? "Hide" : "Data"}
                    </button>
                  </td>
                </tr>
                {expanded === ev.id && (
                  <tr>
                    <td colSpan={6}>
                      <JsonTree value={ev.event_data_decoded ?? ev.event_data} />
                    </td>
                  </tr>
                )}
              </Fragment>
            ))}
          </tbody>
        </table>
      </div>
      {error && <div className="notice danger">{error}</div>}
      <div style={{ marginTop: 12, textAlign: "center" }}>
        {loading ? (
          <span className="muted">Loading…</span>
        ) : cursor ? (
          <button type="button" onClick={() => load(cursor)}>
            Load more
          </button>
        ) : null}
      </div>
    </div>
  );
}
