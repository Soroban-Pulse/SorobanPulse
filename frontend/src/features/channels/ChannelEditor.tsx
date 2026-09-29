import { useEffect, useState } from 'react';
import { ApiError, errorMessage } from '../../api/client';
import { channelsApi } from '../../api/channels';
import type { ChannelType, NotificationChannel } from '../../api/types';
import { Modal } from '../../components/Modal';
import { parseList } from '../../lib/format';
import { isContractId } from '../../lib/validation';
import { ChannelIcon } from './ChannelIcon';
import { CHANNEL_FORMS } from './forms';
import { CHANNEL_TYPES, SCHEMAS, defaultConfig, validateConfig } from './schema';

export type EditorMode =
  | { kind: 'create' }
  | { kind: 'edit'; channel: NotificationChannel }
  | { kind: 'clone'; channel: NotificationChannel };

interface Props {
  mode: EditorMode | null;
  onClose: () => void;
  onSaved: (channel: NotificationChannel, opts: { test: boolean }) => void;
}

function secretKeys(type: ChannelType): string[] {
  return SCHEMAS[type].fields.filter((f) => f.kind === 'secret').map((f) => f.key);
}

function withoutSecrets(type: ChannelType, config: Record<string, unknown>): Record<string, unknown> {
  const out = { ...config };
  for (const k of secretKeys(type)) delete out[k];
  return out;
}

export function ChannelEditor({ mode, onClose, onSaved }: Props) {
  const editing = mode?.kind === 'edit';
  const source = mode && mode.kind !== 'create' ? mode.channel : null;

  const [type, setType] = useState<ChannelType>('slack');
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [tags, setTags] = useState('');
  const [contracts, setContracts] = useState('');
  const [config, setConfig] = useState<Record<string, unknown>>(defaultConfig('slack'));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Reset form state whenever the dialog opens for a different target.
  useEffect(() => {
    if (!mode) return;
    setErrors({});
    setFormError(null);
    if (!source) {
      setType('slack');
      setName('');
      setDescription('');
      setTags('');
      setContracts('');
      setConfig(defaultConfig('slack'));
      return;
    }
    setType(source.channel_type);
    setName(mode.kind === 'clone' ? `${source.name}-copy` : source.name);
    setDescription(source.description ?? '');
    setTags(source.tags.join(', '));
    setContracts(source.contract_filter.join(', '));
    // Secrets are write-only: never prefill them, and a clone must re-enter them.
    setConfig({ ...defaultConfig(source.channel_type), ...withoutSecrets(source.channel_type, source.config) });
  }, [mode, source]);

  const changeType = (t: ChannelType) => {
    setType(t);
    setConfig(defaultConfig(t));
    setErrors({});
  };

  const submit = async (test: boolean) => {
    const contractList = parseList(contracts);
    // On edit, blank secrets mean "keep": merge over the stored config instead of replacing it.
    let payloadConfig = Object.fromEntries(Object.entries(config).filter(([, v]) => v !== '' && v !== undefined));
    if (editing && source) payloadConfig = { ...source.config, ...payloadConfig };

    const errs: Record<string, string> = validateConfig(type, payloadConfig, { editing });
    if (!name.trim()) errs.name = 'Name is required';
    const badContracts = contractList.filter((c) => !isContractId(c));
    if (badContracts.length) errs.contract_filter = `Not a contract ID: ${badContracts.join(', ')}`;
    setErrors(errs);
    if (Object.keys(errs).length) return;

    const payload = {
      name: name.trim(),
      channel_type: type,
      config: payloadConfig,
      description: description.trim() || undefined,
      tags: parseList(tags),
      contract_filter: contractList,
    };

    setBusy(true);
    setFormError(null);
    try {
      const saved =
        editing && source ? await channelsApi.update(source.id, payload) : await channelsApi.create(payload);
      onSaved({ ...(source ?? {}), ...payload, ...saved } as NotificationChannel, { test });
    } catch (e) {
      if (e instanceof ApiError && Object.keys(e.fieldErrors).length) {
        // Server errors that name a config key land on that input.
        const mapped: Record<string, string> = {};
        for (const [k, v] of Object.entries(e.fieldErrors)) mapped[k.replace(/^config\./, '')] = v;
        setErrors(mapped);
      }
      setFormError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const Form = CHANNEL_FORMS[type];
  const title = mode?.kind === 'edit' ? `Edit ${source?.name}` : mode?.kind === 'clone' ? `Clone ${source?.name}` : 'New channel';

  return (
    <Modal
      title={title}
      open={mode !== null}
      onClose={onClose}
      wide
      footer={
        <>
          <button type="button" className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button type="button" className="btn" onClick={() => submit(true)} disabled={busy}>
            Save and send test
          </button>
          <button type="button" className="btn btn-primary" onClick={() => submit(false)} disabled={busy}>
            {busy ? 'Saving…' : editing ? 'Save changes' : 'Create channel'}
          </button>
        </>
      }
    >
      <form
        className="form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          submit(false);
        }}
      >
        <fieldset className="type-picker" disabled={editing}>
          <legend>Channel type</legend>
          {CHANNEL_TYPES.map((t) => (
            <label key={t} className={t === type ? 'type-option selected' : 'type-option'}>
              <input type="radio" name="channel_type" value={t} checked={t === type} onChange={() => changeType(t)} />
              <ChannelIcon type={t} />
              <span>{SCHEMAS[t].label}</span>
            </label>
          ))}
        </fieldset>

        <div className="field">
          <label htmlFor="ch-name">
            Name<span className="req" aria-hidden="true"> *</span>
          </label>
          <input id="ch-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="ops-alerts" />
          {errors.name && <small className="field-error">{errors.name}</small>}
        </div>
        <div className="field">
          <label htmlFor="ch-desc">Description</label>
          <input id="ch-desc" value={description} onChange={(e) => setDescription(e.target.value)} />
        </div>
        <div className="field">
          <label htmlFor="ch-tags">Tags</label>
          <input id="ch-tags" value={tags} onChange={(e) => setTags(e.target.value)} placeholder="prod, defi" />
        </div>
        <div className="field">
          <label htmlFor="ch-contracts">Only for contracts</label>
          <input
            id="ch-contracts"
            value={contracts}
            onChange={(e) => setContracts(e.target.value)}
            placeholder="Leave empty for all contracts"
            spellCheck={false}
          />
          {errors.contract_filter && <small className="field-error">{errors.contract_filter}</small>}
        </div>

        <fieldset className="config-fields">
          <legend>{SCHEMAS[type].label} settings</legend>
          <Form value={config} onChange={setConfig} errors={errors} editing={editing} />
        </fieldset>

        {formError && (
          <p className="alert alert-error" role="alert">
            {formError}
          </p>
        )}
        <button type="submit" hidden />
      </form>
    </Modal>
  );
}
