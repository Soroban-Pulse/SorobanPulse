import { useEffect, useState } from "react";
import { updateSettings, useSettings } from "../lib/settings";
import { Dialog } from "./Dialog";

export function SettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const settings = useSettings();
  const [draft, setDraft] = useState(settings);
  useEffect(() => {
    if (open) setDraft(settings);
  }, [open, settings]);

  const save = (e: React.FormEvent) => {
    e.preventDefault();
    updateSettings({ ...draft, apiBaseUrl: draft.apiBaseUrl.trim() });
    onClose();
  };

  return (
    <Dialog open={open} onClose={onClose} title="Settings">
      <form onSubmit={save} className="stack">
        <label className="field">
          API base URL
          <input
            value={draft.apiBaseUrl}
            placeholder="Same origin"
            onChange={(e) => setDraft({ ...draft, apiBaseUrl: e.target.value })}
          />
          <span className="hint">Leave empty to use the same origin (or the dev proxy).</span>
        </label>
        <label className="field">
          API key
          <input
            type="password"
            autoComplete="off"
            value={draft.apiKey}
            onChange={(e) => setDraft({ ...draft, apiKey: e.target.value })}
          />
          <span className="hint">Sent as a Bearer token when the server has API_KEY set.</span>
        </label>
        <label className="field">
          Admin key
          <input
            type="password"
            autoComplete="off"
            value={draft.adminKey}
            onChange={(e) => setDraft({ ...draft, adminKey: e.target.value })}
          />
          <span className="hint">Required for the admin console and the SLO report. Stored only in this browser.</span>
        </label>
        <div className="dialog-actions">
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="primary">
            Save
          </button>
        </div>
      </form>
    </Dialog>
  );
}
