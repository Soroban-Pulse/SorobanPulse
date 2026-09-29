import { useParams } from "react-router-dom";
import { EventList } from "../components/EventList";

export function TxPage() {
  const { txHash = "" } = useParams();
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <div className="muted">Transaction</div>
          <h1 className="mono break">{txHash}</h1>
        </div>
      </div>
      <div className="card">
        <h2>Events emitted</h2>
        <EventList path={`/v1/events/tx/${txHash}`} emptyText="No indexed events for this transaction." />
      </div>
    </div>
  );
}
