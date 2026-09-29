import { useParams } from "react-router-dom";
import { EventList } from "../components/EventList";

/** Accounts are not a first-class entity in the index, so we full-text search event data for the address. */
export function AccountPage() {
  const { accountId = "" } = useParams();
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <div className="muted">{accountId.startsWith("M") ? "Muxed account" : "Account"}</div>
          <h1 className="mono break">{accountId}</h1>
        </div>
      </div>
      <div className="card">
        <h2>Events mentioning this account</h2>
        <EventList
          path="/v1/events"
          params={{ search: accountId, sort: "desc" }}
          emptyText="No indexed events reference this account."
        />
      </div>
    </div>
  );
}
