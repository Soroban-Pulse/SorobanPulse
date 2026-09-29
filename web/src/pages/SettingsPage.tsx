/**
 * SettingsPage
 *
 * Lets the user configure:
 *   - Server URL       (required)
 *   - API key          (required)
 *   - Admin key        (optional — unlocks admin navigation)
 *   - "Remember on this device" toggle (opts into localStorage persistence)
 *
 * The "Test connection" button calls /health then /v1/meta with the
 * credentials currently typed in the form (not yet saved), so the user gets
 * immediate feedback before committing.
 */

import { type FormEvent, useId, useReducer, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useSettings } from "../context/SettingsContext.tsx";
import { createApiClient, ApiError } from "../api/client.ts";

// ─── Types ────────────────────────────────────────────────────────────────────

type TestState =
  | { status: "idle" }
  | { status: "testing" }
  | { status: "ok"; message: string }
  | { status: "auth_fail"; message: string }
  | { status: "unreachable"; message: string };

type FormState = {
  serverUrl: string;
  apiKey: string;
  adminKey: string;
  remember: boolean;
};

type FormAction =
  | { type: "set_server_url"; value: string }
  | { type: "set_api_key"; value: string }
  | { type: "set_admin_key"; value: string }
  | { type: "set_remember"; value: boolean }
  | { type: "reset"; payload: FormState };

function formReducer(state: FormState, action: FormAction): FormState {
  switch (action.type) {
    case "set_server_url": return { ...state, serverUrl: action.value };
    case "set_api_key":    return { ...state, apiKey: action.value };
    case "set_admin_key":  return { ...state, adminKey: action.value };
    case "set_remember":   return { ...state, remember: action.value };
    case "reset":          return action.payload;
    default:               return state;
  }
}

// ─── Test-connection helper ───────────────────────────────────────────────────

/**
 * Calls /health (unauthenticated) and /v1/meta (authenticated) against the
 * given serverUrl + apiKey, returning a TestState.
 *
 * We use a one-shot client so credentials are never written to storage before
 * the user explicitly saves.
 */
async function testConnection(
  serverUrl: string,
  apiKey: string,
): Promise<TestState> {
  const base = serverUrl.replace(/\/$/, "");

  // 1. /health — basic reachability (no auth required)
  try {
    const res = await fetch(`${base}/health`, { signal: AbortSignal.timeout(8_000) });
    if (!res.ok && res.status !== 401 && res.status !== 403) {
      return {
        status: "unreachable",
        message: `Server returned HTTP ${res.status} on /health`,
      };
    }
  } catch {
    return {
      status: "unreachable",
      message: "Could not reach the server. Check the URL and your network.",
    };
  }

  // 2. /v1/meta — authenticated probe
  const client = createApiClient({ baseUrl: base, apiKey });
  try {
    // /v1/meta is a lightweight endpoint that returns server metadata.
    // We use a raw fetch here because the typed client would need the path in schema.d.ts;
    // a direct fetch avoids schema coupling for this one-off probe.
    const res = await fetch(`${base}/v1/meta`, {
      headers: apiKey ? { "X-Api-Key": apiKey } : {},
      signal: AbortSignal.timeout(8_000),
    });

    if (res.status === 401 || res.status === 403) {
      return {
        status: "auth_fail",
        message: `Authentication failed (HTTP ${res.status}). Check your API key.`,
      };
    }
    if (!res.ok) {
      return {
        status: "unreachable",
        message: `Server returned HTTP ${res.status} on /v1/meta`,
      };
    }

    return { status: "ok", message: "Connected successfully." };
  } catch (err) {
    if (err instanceof ApiError && err.isUnauthorized) {
      return {
        status: "auth_fail",
        message: `Authentication failed (HTTP ${err.status}). Check your API key.`,
      };
    }
    return {
      status: "unreachable",
      message: "Could not reach the server. Check the URL and your network.",
    };
  }
}

// ─── Component ────────────────────────────────────────────────────────────────

export default function SettingsPage() {
  const settings = useSettings();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();

  // Where to redirect after a successful save (set by RequireConfig in App.tsx)
  const nextPath = searchParams.get("next") ?? "/";

  const [form, dispatch] = useReducer(formReducer, {
    serverUrl: settings.serverUrl,
    apiKey:    settings.apiKey,
    adminKey:  settings.adminKey,
    remember:  settings.remember,
  });

  const [testState, setTestState] = useReducer(
    (_: TestState, next: TestState) => next,
    { status: "idle" } as TestState,
  );

  const isTesting = testState.status === "testing";
  const formRef = useRef<HTMLFormElement>(null);

  // Auto-stable IDs for label/input pairing (accessibility)
  const idPrefix = useId();
  const ids = {
    serverUrl: `${idPrefix}-server-url`,
    apiKey:    `${idPrefix}-api-key`,
    adminKey:  `${idPrefix}-admin-key`,
    remember:  `${idPrefix}-remember`,
  };

  // ── Handlers ────────────────────────────────────────────────────────────────

  async function handleTest() {
    if (!form.serverUrl || !form.apiKey) return;
    setTestState({ status: "testing" });
    const result = await testConnection(form.serverUrl, form.apiKey);
    setTestState(result);
  }

  function handleSave(e: FormEvent) {
    e.preventDefault();
    settings.save(form);
    // Reset the test banner so it doesn't linger after saving new credentials.
    setTestState({ status: "idle" });
    // If we were redirected here by RequireConfig, go back to where we came from.
    if (!settings.isConfigured) {
      navigate(nextPath, { replace: true });
    }
  }

  function handleDiscard() {
    dispatch({
      type: "reset",
      payload: {
        serverUrl: settings.serverUrl,
        apiKey:    settings.apiKey,
        adminKey:  settings.adminKey,
        remember:  settings.remember,
      },
    });
    setTestState({ status: "idle" });
  }

  // ── Derived state ────────────────────────────────────────────────────────────

  const isDirty =
    form.serverUrl !== settings.serverUrl ||
    form.apiKey    !== settings.apiKey    ||
    form.adminKey  !== settings.adminKey  ||
    form.remember  !== settings.remember;

  const canTest = Boolean(form.serverUrl.trim() && form.apiKey.trim()) && !isTesting;

  // ── Render ───────────────────────────────────────────────────────────────────

  return (
    <div className="settings-page">
      <div className="page-header">
        <h1>Settings</h1>
        <p>Configure the server connection and API credentials for this dashboard.</p>
      </div>

      <form
        ref={formRef}
        className="settings-form card"
        onSubmit={handleSave}
        noValidate
      >
        {/* ── Server URL ───────────────────────────────────────────────────── */}
        <div className="settings-form__group">
          <label className="settings-form__label" htmlFor={ids.serverUrl}>
            Server URL <span className="settings-form__required" aria-hidden="true">*</span>
          </label>
          <input
            id={ids.serverUrl}
            type="url"
            className="settings-form__input"
            placeholder="https://api.sorobanpulse.example.com"
            value={form.serverUrl}
            onChange={(e) => dispatch({ type: "set_server_url", value: e.target.value })}
            required
            autoComplete="url"
            aria-describedby={`${ids.serverUrl}-hint`}
          />
          <p id={`${ids.serverUrl}-hint`} className="settings-form__hint">
            Base URL of the SorobanPulse API server, without a trailing slash.
          </p>
        </div>

        {/* ── API key ──────────────────────────────────────────────────────── */}
        <div className="settings-form__group">
          <label className="settings-form__label" htmlFor={ids.apiKey}>
            API Key <span className="settings-form__required" aria-hidden="true">*</span>
          </label>
          <PasswordInput
            id={ids.apiKey}
            value={form.apiKey}
            placeholder="sp_live_••••••••••••••••"
            autoComplete="current-password"
            describedBy={`${ids.apiKey}-hint`}
            onChange={(v) => dispatch({ type: "set_api_key", value: v })}
          />
          <p id={`${ids.apiKey}-hint`} className="settings-form__hint">
            Sent as <code>X-Api-Key</code>. Required to access all API endpoints.
          </p>
        </div>

        {/* ── Admin key ────────────────────────────────────────────────────── */}
        <div className="settings-form__group">
          <label className="settings-form__label" htmlFor={ids.adminKey}>
            Admin Key <span className="settings-form__optional">(optional)</span>
          </label>
          <PasswordInput
            id={ids.adminKey}
            value={form.adminKey}
            placeholder="sp_admin_••••••••••••••••"
            autoComplete="off"
            describedBy={`${ids.adminKey}-hint`}
            onChange={(v) => dispatch({ type: "set_admin_key", value: v })}
          />
          <p id={`${ids.adminKey}-hint`} className="settings-form__hint">
            Sent as <code>X-Admin-Key</code>. Unlocks admin navigation when set.
          </p>
        </div>

        {/* ── Remember on this device ──────────────────────────────────────── */}
        <div className="settings-form__group settings-form__group--inline">
          <input
            id={ids.remember}
            type="checkbox"
            className="settings-form__checkbox"
            checked={form.remember}
            onChange={(e) => dispatch({ type: "set_remember", value: e.target.checked })}
          />
          <label htmlFor={ids.remember} className="settings-form__inline-label">
            Remember on this device
          </label>
          <p className="settings-form__hint settings-form__hint--inline">
            Saves credentials in <code>localStorage</code>. Disable on shared computers.
          </p>
        </div>

        {/* ── Test connection result banner ────────────────────────────────── */}
        {testState.status !== "idle" && (
          <TestBanner state={testState} />
        )}

        {/* ── Actions ──────────────────────────────────────────────────────── */}
        <div className="settings-form__actions">
          <button
            type="button"
            className="btn settings-form__test-btn"
            onClick={handleTest}
            disabled={!canTest}
            aria-busy={isTesting}
          >
            {isTesting ? "Testing…" : "Test connection"}
          </button>

          <div className="settings-form__save-group">
            {isDirty && (
              <button
                type="button"
                className="btn"
                onClick={handleDiscard}
              >
                Discard
              </button>
            )}
            <button
              type="submit"
              className="btn btn--primary"
              disabled={!form.serverUrl.trim() || !form.apiKey.trim()}
            >
              Save settings
            </button>
          </div>
        </div>
      </form>

      {/* ── Danger zone ─────────────────────────────────────────────────────── */}
      {settings.isConfigured && (
        <div className="settings-danger card">
          <h2 className="settings-danger__title">Danger zone</h2>
          <p className="settings-danger__desc">
            Clears all saved credentials from this browser. You will be
            redirected to this page to configure a new connection.
          </p>
          <button
            type="button"
            className="btn btn--danger"
            onClick={settings.clear}
          >
            Clear saved credentials
          </button>
        </div>
      )}
    </div>
  );
}

// ─── PasswordInput ────────────────────────────────────────────────────────────

function PasswordInput({
  id,
  value,
  placeholder,
  autoComplete,
  describedBy,
  onChange,
}: {
  id: string;
  value: string;
  placeholder: string;
  autoComplete: string;
  describedBy: string;
  onChange: (v: string) => void;
}) {
  const [visible, toggleVisible] = useReducer((s: boolean) => !s, false);

  return (
    <div className="settings-form__password-wrap">
      <input
        id={id}
        type={visible ? "text" : "password"}
        className="settings-form__input settings-form__input--password"
        value={value}
        placeholder={placeholder}
        autoComplete={autoComplete}
        aria-describedby={describedBy}
        onChange={(e) => onChange(e.target.value)}
        spellCheck={false}
      />
      <button
        type="button"
        className="settings-form__reveal-btn"
        onClick={toggleVisible}
        aria-label={visible ? "Hide key" : "Show key"}
        tabIndex={-1}
      >
        {visible ? "🙈" : "👁"}
      </button>
    </div>
  );
}

// ─── TestBanner ───────────────────────────────────────────────────────────────

function TestBanner({ state }: { state: TestState }) {
  if (state.status === "idle") return null;

  if (state.status === "testing") {
    return (
      <div className="test-banner test-banner--loading" role="status" aria-live="polite">
        <span className="test-banner__dot" />
        Testing connection…
      </div>
    );
  }

  const map = {
    ok:          { cls: "test-banner--ok",          icon: "✓" },
    auth_fail:   { cls: "test-banner--auth-fail",   icon: "✕" },
    unreachable: { cls: "test-banner--unreachable", icon: "✕" },
  } as const;

  const { cls, icon } = map[state.status];

  return (
    <div
      className={`test-banner ${cls}`}
      role="alert"
      aria-live="assertive"
    >
      <span className="test-banner__icon" aria-hidden="true">{icon}</span>
      {state.message}
    </div>
  );
}
