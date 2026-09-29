import { useEffect, useState } from 'react';
import { ApiError, errorMessage, loadSettings } from '../../api/client';
import { subscriptionsApi } from '../../api/subscriptions';
import type { Subscription, SubscriptionInput } from '../../api/types';
import { Modal } from '../../components/Modal';
import { generateSecret, parseList } from '../../lib/format';
import { isContractId, validateCallbackUrl } from '../../lib/validation';

const EVENT_TYPES = ['contract', 'diagnostic', 'system'] as const;

interface Props {
  /** null = closed, 'new' = create, Subscription = edit */
  target: Subscription | 'new' | null;
  onClose: () => void;
  /** `secret` is set only on create: the one time it can be shown. */
  onSaved: (sub: Subscription, secret: string | null) => void;
}

type Errors = Partial<Record<keyof SubscriptionInput | 'form', string>>;

export function SubscriptionForm({ target, onClose, onSaved }: Props) {
  const editing = target !== null && target !== 'new';
  const [callbackUrl, setCallbackUrl] = useState('');
  const [contracts, setContracts] = useState('');
  const [eventTypes, setEventTypes] = useState<string[]>([]);
  const [fromLedger, setFromLedger] = useState('0');
  const [mode, setMode] = useState<'single' | 'batch'>('single');
  const [batchSize, setBatchSize] = useState('100');
  const [batchTimeout, setBatchTimeout] = useState('5000');
  const [useSecret, setUseSecret] = useState(true);
  const [secret, setSecret] = useState('');
  const [errors, setErrors] = useState<Errors>({});
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (target === null) return;
    setErrors({});
    if (target === 'new') {
      setCallbackUrl('');
      setContracts('');
      setEventTypes([]);
      setFromLedger('0');
      setMode('single');
      setBatchSize('100');
      setBatchTimeout('5000');
      setUseSecret(true);
      setSecret(generateSecret());
    } else {
      setCallbackUrl(target.callback_url);
      setContracts((target.contract_ids ?? []).join(', '));
      setEventTypes(target.event_types ?? []);
      setFromLedger(String(target.from_ledger));
      setMode(target.subscription_type);
      setBatchSize(String(target.batch_size));
      setBatchTimeout(String(target.batch_timeout_ms));
    }
  }, [target]);

  // Validate as the user types the URL, using the same rules the server applies.
  const urlError = callbackUrl ? validateCallbackUrl(callbackUrl, { requireHttps: loadSettings().requireHttpsCallbacks }) : null;

  const validate = (): Errors => {
    const e: Errors = {};
    const u = validateCallbackUrl(callbackUrl, { requireHttps: loadSettings().requireHttpsCallbacks });
    if (u) e.callback_url = u;
    const bad = parseList(contracts).filter((c) => !isContractId(c));
    if (bad.length) e.contract_ids = `Not a Soroban contract ID (C… 56 chars): ${bad.join(', ')}`;
    const fl = Number(fromLedger);
    if (!Number.isInteger(fl) || fl < 0) e.from_ledger = 'from_ledger must be non-negative';
    if (mode === 'batch') {
      const bs = Number(batchSize);
      const bt = Number(batchTimeout);
      if (!Number.isInteger(bs) || bs < 1 || bs > 1000) e.batch_size = 'batch_size must be between 1 and 1000';
      if (!Number.isInteger(bt) || bt < 100 || bt > 60000) e.batch_timeout_ms = 'batch_timeout_ms must be between 100 and 60000';
    }
    return e;
  };

  const submit = async (ev: React.FormEvent) => {
    ev.preventDefault();
    const e = validate();
    setErrors(e);
    if (Object.keys(e).length) return;

    const input: SubscriptionInput = {
      callback_url: callbackUrl.trim(),
      from_ledger: Number(fromLedger),
      subscription_type: mode,
      batch_size: mode === 'batch' ? Number(batchSize) : undefined,
      batch_timeout_ms: mode === 'batch' ? Number(batchTimeout) : undefined,
      contract_ids: parseList(contracts),
      event_types: eventTypes,
      secret: !editing && useSecret ? secret : undefined,
    };

    setBusy(true);
    try {
      if (editing) {
        const { secret: _omit, ...patch } = input;
        void _omit;
        const saved = await subscriptionsApi.update(target.id, patch);
        onSaved({ ...target, ...patch, ...saved }, null);
      } else {
        const saved = await subscriptionsApi.create(input);
        // Prefer a server-issued secret; otherwise the one we generated and sent.
        onSaved(saved, saved.secret ?? input.secret ?? null);
      }
    } catch (err) {
      if (err instanceof ApiError && Object.keys(err.fieldErrors).length) {
        setErrors({ ...err.fieldErrors, form: undefined } as Errors);
      } else {
        setErrors({ form: errorMessage(err) });
      }
    } finally {
      setBusy(false);
    }
  };

  const fieldErr = (k: keyof Errors) => errors[k] && <small className="field-error">{errors[k]}</small>;

  return (
    <Modal
      title={editing ? 'Edit subscription' : 'New webhook subscription'}
      open={target !== null}
      onClose={onClose}
      wide
      footer={
        <>
          <button type="button" className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button type="submit" form="subscription-form" className="btn btn-primary" disabled={busy}>
            {busy ? 'Saving…' : editing ? 'Save changes' : 'Create subscription'}
          </button>
        </>
      }
    >
      <form id="subscription-form" className="form-grid" onSubmit={submit} noValidate>
        <div className="field span-2">
          <label htmlFor="sub-url">
            Callback URL<span className="req" aria-hidden="true"> *</span>
          </label>
          <input
            id="sub-url"
            type="url"
            value={callbackUrl}
            onChange={(e) => setCallbackUrl(e.target.value)}
            placeholder="https://example.com/soroban/webhook"
            spellCheck={false}
            aria-invalid={Boolean(errors.callback_url || urlError)}
          />
          {errors.callback_url ? fieldErr('callback_url') : urlError && <small className="field-error">{urlError}</small>}
          <small className="help">Must be publicly reachable. Private, loopback and link-local addresses are rejected.</small>
        </div>

        <div className="field span-2">
          <label htmlFor="sub-contracts">Contracts</label>
          <input
            id="sub-contracts"
            value={contracts}
            onChange={(e) => setContracts(e.target.value)}
            placeholder="Leave empty to receive events from every contract"
            spellCheck={false}
          />
          {fieldErr('contract_ids')}
        </div>

        <fieldset className="field">
          <legend>Event types</legend>
          {EVENT_TYPES.map((t) => (
            <label key={t} className="field-check">
              <input
                type="checkbox"
                checked={eventTypes.includes(t)}
                onChange={(e) => setEventTypes((xs) => (e.target.checked ? [...xs, t] : xs.filter((x) => x !== t)))}
              />
              <span>{t}</span>
            </label>
          ))}
          <small className="help">None selected = all types.</small>
        </fieldset>

        <div className="field">
          <label htmlFor="sub-ledger">Start from ledger</label>
          <input
            id="sub-ledger"
            type="number"
            min={0}
            value={fromLedger}
            onChange={(e) => setFromLedger(e.target.value)}
            disabled={editing}
          />
          {fieldErr('from_ledger')}
          <small className="help">Existing events from this ledger onward are queued for delivery.</small>
        </div>

        <fieldset className="field span-2">
          <legend>Delivery mode</legend>
          <label className="field-check">
            <input type="radio" name="mode" checked={mode === 'single'} onChange={() => setMode('single')} />
            <span>One request per event</span>
          </label>
          <label className="field-check">
            <input type="radio" name="mode" checked={mode === 'batch'} onChange={() => setMode('batch')} />
            <span>Batch events</span>
          </label>
          {mode === 'batch' && (
            <div className="inline-fields">
              <label className="field">
                <span>Max events per batch</span>
                <input type="number" min={1} max={1000} value={batchSize} onChange={(e) => setBatchSize(e.target.value)} />
                {fieldErr('batch_size')}
              </label>
              <label className="field">
                <span>Flush after (ms)</span>
                <input type="number" min={100} max={60000} step={100} value={batchTimeout} onChange={(e) => setBatchTimeout(e.target.value)} />
                {fieldErr('batch_timeout_ms')}
              </label>
            </div>
          )}
        </fieldset>

        {!editing && (
          <fieldset className="field span-2">
            <legend>Signing secret</legend>
            <label className="field-check">
              <input type="checkbox" checked={useSecret} onChange={(e) => setUseSecret(e.target.checked)} />
              <span>Sign deliveries with an HMAC secret (recommended)</span>
            </label>
            {useSecret && (
              <div className="secret-row">
                <code className="mono secret-preview">{secret.slice(0, 12)}••••••••</code>
                <button type="button" className="btn btn-small" onClick={() => setSecret(generateSecret())}>
                  Regenerate
                </button>
              </div>
            )}
            <small className="help">The full secret is shown once after creation.</small>
          </fieldset>
        )}

        {errors.form && (
          <p className="alert alert-error span-2" role="alert">
            {errors.form}
          </p>
        )}
      </form>
    </Modal>
  );
}
