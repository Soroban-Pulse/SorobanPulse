import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { DetailDrawer } from "../components/DetailDrawer";
import { LiveRegion } from "../components/LiveRegion";
import { Column, ResponsiveTable } from "../components/ResponsiveTable";
import { useFormat } from "../i18n/format";

/** Shape of an event pushed on GET /v1/events/stream (see src/models.rs). */
interface StreamEvent {
  id?: string;
  contract_id: string;
  event_type: string;
  tx_hash: string;
  ledger: number;
  timestamp?: string;
  created_at?: string;
  event_data?: unknown;
}

type ConnectionState = "connecting" | "open" | "error";

const MAX_EVENTS = 50;
/** New-event announcements are batched so screen readers are not flooded. */
const ANNOUNCE_INTERVAL_MS = 10_000;

function eventKey(e: StreamEvent) {
  return e.id ?? `${e.tx_hash}-${e.ledger}`;
}

export function LiveStreamPage() {
  const { t } = useTranslation();
  const format = useFormat();
  const [connection, setConnection] = useState<ConnectionState>("connecting");
  const [events, setEvents] = useState<StreamEvent[]>([]);
  const [paused, setPaused] = useState(false);
  const [buffered, setBuffered] = useState<StreamEvent[]>([]);
  const [announcement, setAnnouncement] = useState("");
  const [selected, setSelected] = useState<StreamEvent | null>(null);
  const pausedRef = useRef(paused);
  const unannounced = useRef(0);
  const tRef = useRef(t);
  pausedRef.current = paused;
  tRef.current = t;

  useEffect(() => {
    const source = new EventSource("/api/v1/events/stream");
    source.onopen = () => setConnection("open");
    source.onerror = () => setConnection(source.readyState === EventSource.CLOSED ? "error" : "connecting");
    source.onmessage = (message) => {
      let parsed: StreamEvent;
      try {
        parsed = JSON.parse(message.data);
      } catch {
        return;
      }
      if (pausedRef.current) {
        setBuffered((b) => [parsed, ...b].slice(0, MAX_EVENTS));
      } else {
        unannounced.current += 1;
        setEvents((e) => [parsed, ...e].slice(0, MAX_EVENTS));
      }
    };

    const timer = setInterval(() => {
      if (unannounced.current > 0) {
        setAnnouncement(tRef.current("live.newEvents", { count: unannounced.current }));
        unannounced.current = 0;
      }
    }, ANNOUNCE_INTERVAL_MS);

    return () => {
      source.close();
      clearInterval(timer);
    };
  }, []);

  const togglePaused = () => {
    if (paused) {
      setEvents((e) => [...buffered, ...e].slice(0, MAX_EVENTS));
      unannounced.current += buffered.length;
      setBuffered([]);
    }
    setPaused(!paused);
  };

  const columns: Column<StreamEvent>[] = [
    { id: "ledger", header: t("live.columns.ledger"), cell: (e) => format.number(e.ledger) },
    { id: "contract", header: t("live.columns.contract"), cell: (e) => <code className="mono">{e.contract_id}</code>, key: true },
    { id: "type", header: t("live.columns.type"), cell: (e) => e.event_type, key: true },
    {
      id: "time",
      header: t("live.columns.time"),
      cell: (e) => {
        const at = e.timestamp ?? e.created_at;
        return at ? <time dateTime={at}>{format.dateTime(at)}</time> : t("common.notAvailable");
      },
      key: true,
    },
  ];

  const connectionLabel =
    connection === "open" ? t("live.connected") : connection === "error" ? t("live.disconnected") : t("live.connecting");

  return (
    <div className="live-page">
      <div className="page-header">
        <h1>{t("live.heading")}</h1>
        <p className={`connection connection-${connection}`}>{connectionLabel}</p>
      </div>
      <LiveRegion message={announcement} />

      <div className="toolbar">
        <button type="button" aria-pressed={paused} onClick={togglePaused}>
          {paused ? t("live.resume") : t("live.pause")}
        </button>
        {paused && <p className="muted">{t("live.paused", { count: buffered.length })}</p>}
      </div>

      <ResponsiveTable
        caption={t("live.tableCaption")}
        columns={columns}
        rows={events}
        rowKey={eventKey}
        emptyText={t("live.empty")}
        onOpen={setSelected}
        openLabel={(e) => t("live.openDetail", { ledger: format.number(e.ledger) })}
      />

      <DetailDrawer open={selected !== null} title={t("live.detail.title")} onClose={() => setSelected(null)}>
        {selected && (
          <dl className="detail-list">
            <dt>{t("live.columns.contract")}</dt>
            <dd className="mono">{selected.contract_id}</dd>
            <dt>{t("live.columns.type")}</dt>
            <dd>{selected.event_type}</dd>
            <dt>{t("live.columns.ledger")}</dt>
            <dd>{format.number(selected.ledger)}</dd>
            <dt>{t("live.detail.txHash")}</dt>
            <dd className="mono">{selected.tx_hash}</dd>
            <dt>{t("live.detail.data")}</dt>
            <dd>
              <pre className="code-block" tabIndex={0}>
                {JSON.stringify(selected.event_data ?? null, null, 2)}
              </pre>
            </dd>
          </dl>
        )}
      </DetailDrawer>
    </div>
  );
}
