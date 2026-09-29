import { Link } from "react-router-dom";

export default function NotFoundPage() {
  return (
    <div className="empty-state" style={{ marginTop: "4rem" }}>
      <div className="empty-state__icon">🔭</div>
      <h1 style={{ fontSize: "1.25rem", marginBottom: "0.5rem" }}>Page Not Found</h1>
      <p className="text-muted">That route doesn't exist.</p>
      <p className="mt-2">
        <Link to="/" className="btn btn--primary">
          ← Back to Overview
        </Link>
      </p>
    </div>
  );
}
