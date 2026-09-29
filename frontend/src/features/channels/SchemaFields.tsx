import { parseList } from '../../lib/format';
import type { FieldSchema } from './schema';

interface Props {
  fields: FieldSchema[];
  value: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
  errors: Record<string, string>;
  /** When editing, secrets are write-only: blank means "keep the stored value". */
  editing: boolean;
}

/** Render a list of schema fields as labelled inputs with inline errors. */
export function SchemaFields({ fields, value, onChange, errors, editing }: Props) {
  const set = (key: string, v: unknown) => onChange({ ...value, [key]: v });

  return (
    <>
      {fields.map((f) => {
        const id = `cfg-${f.key}`;
        const err = errors[f.key];
        const describedBy = [err && `${id}-err`, f.help && `${id}-help`].filter(Boolean).join(' ') || undefined;
        const common = {
          id,
          'aria-invalid': err ? true : undefined,
          'aria-describedby': describedBy,
        };

        if (f.kind === 'boolean') {
          return (
            <label key={f.key} className="field field-check">
              <input
                type="checkbox"
                {...common}
                checked={Boolean(value[f.key])}
                onChange={(e) => set(f.key, e.target.checked)}
              />
              <span>{f.label}</span>
            </label>
          );
        }

        let input: JSX.Element;
        switch (f.kind) {
          case 'select':
            input = (
              <select {...common} value={String(value[f.key] ?? '')} onChange={(e) => set(f.key, e.target.value)}>
                {f.options?.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            );
            break;
          case 'textarea':
            input = (
              <textarea
                {...common}
                rows={4}
                placeholder={f.placeholder}
                value={String(value[f.key] ?? '')}
                onChange={(e) => set(f.key, e.target.value)}
              />
            );
            break;
          case 'list':
            input = (
              <input
                {...common}
                placeholder={f.placeholder}
                defaultValue={Array.isArray(value[f.key]) ? (value[f.key] as string[]).join(', ') : ''}
                onChange={(e) => set(f.key, parseList(e.target.value))}
              />
            );
            break;
          case 'number':
            input = (
              <input
                {...common}
                type="number"
                value={value[f.key] === undefined ? '' : String(value[f.key])}
                onChange={(e) => set(f.key, e.target.value === '' ? undefined : Number(e.target.value))}
              />
            );
            break;
          default:
            input = (
              <input
                {...common}
                type={f.kind === 'secret' ? 'password' : f.kind === 'url' ? 'url' : 'text'}
                autoComplete={f.kind === 'secret' ? 'new-password' : 'off'}
                spellCheck={false}
                placeholder={f.kind === 'secret' && editing ? '•••••• (unchanged)' : f.placeholder}
                value={String(value[f.key] ?? '')}
                onChange={(e) => set(f.key, e.target.value)}
              />
            );
        }

        return (
          <div key={f.key} className="field">
            <label htmlFor={id}>
              {f.label}
              {f.required && <span className="req" aria-hidden="true"> *</span>}
            </label>
            {input}
            {f.help && (
              <small id={`${id}-help`} className="help">
                {f.help}
              </small>
            )}
            {err && (
              <small id={`${id}-err`} className="field-error">
                {err}
              </small>
            )}
          </div>
        );
      })}
    </>
  );
}
