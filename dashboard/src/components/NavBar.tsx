import { Link } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";

export function NavBar() {
  const { user, logout } = useAuth();

  return (
    <nav className="navbar">
      <div className="navbar-brand">SorobanPulse</div>
      {user && (
        <div className="navbar-links">
          <Link to="/">Status</Link>
          <Link to="/subscriptions">Subscriptions</Link>
          <Link to="/webhooks">Webhooks</Link>
          <button onClick={logout}>Sign out</button>
        </div>
      )}
    </nav>
  );
}
