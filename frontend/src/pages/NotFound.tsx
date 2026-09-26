import { Link } from "react-router-dom";

export function NotFound() {
  return (
    <div className="empty">
      <h1>Page not found</h1>
      <p>
        Try the search box above, or go back to the <Link to="/explorer">explorer</Link>.
      </p>
    </div>
  );
}
