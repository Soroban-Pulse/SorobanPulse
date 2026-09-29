import { useState } from 'react';
import { loadSettings, saveSettings, type ApiSettings } from '../../api/client';
import { useToast } from '../../components/Toast';

export function SettingsPage() {
  const toast = useToast();
  const [s, setS] = useState<ApiSettings>(loadSettings);

  const save = (e: React.FormEvent) => {
    e.preventDefault();
    saveSettings(s);
    toast('Settings saved', 'ok');
  };

  return (
    <div className="page narrow">
      <header className="page-header">
        <h1>Settings</h1>
      </header>
      <form className="panel form-grid single" onSubmit={save}>
        <div className="field">
          <label htmlFor="set-base">API base URL</label>
          <input
            id="set-base"
            value={s.baseUrl}
            onChange={(e) => setS({ ...s, baseUrl: e.target.value })}
            placeholder="Empty = same origin (dev server proxies to :3000)"
          />
        </div>
        <div className="field">
          <label htmlFor="set-key">API key</label>
          <input id="set-key" type="password" autoComplete="off" value={s.apiKey} onChange={(e) => setS({ ...s, apiKey: e.target.value })} />
          <small className="help">Sent as a Bearer token on regular routes when the server sets API_KEY.</small>
        </div>
        <div className="field">
          <label htmlFor="set-admin">Admin API key</label>
          <input id="set-admin" type="password" autoComplete="off" value={s.adminApiKey} onChange={(e) => setS({ ...s, adminApiKey: e.target.value })} />
          <small className="help">Needed for channels, maintenance windows and the dead-letter queue.</small>
        </div>
        <label className="field-check">
          <input type="checkbox" checked={s.requireHttpsCallbacks} onChange={(e) => setS({ ...s, requireHttpsCallbacks: e.target.checked })} />
          <span>Server runs in production or staging (callback URLs must be HTTPS)</span>
        </label>
        <p className="help">Keys are stored in this browser's localStorage only.</p>
        <div>
          <button type="submit" className="btn btn-primary">
            Save
          </button>
        </div>
      </form>
    </div>
  );
}
