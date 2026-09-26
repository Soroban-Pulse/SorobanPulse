import { useState, type ReactNode } from 'react';
import { Modal } from './Modal';
import { errorMessage } from '../api/client';

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  /** Require the user to type this text before confirming (for destructive actions). */
  typeToConfirm?: string;
  onConfirm: () => Promise<unknown>;
  onClose: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  danger,
  typeToConfirm,
  onConfirm,
  onClose,
}: ConfirmDialogProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [typed, setTyped] = useState('');

  const close = () => {
    setError(null);
    setTyped('');
    onClose();
  };

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
      close();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const blocked = typeToConfirm !== undefined && typed !== typeToConfirm;

  return (
    <Modal
      title={title}
      open={open}
      onClose={close}
      footer={
        <>
          <button type="button" className="btn" onClick={close} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className={danger ? 'btn btn-danger' : 'btn btn-primary'}
            onClick={run}
            disabled={busy || blocked}
          >
            {busy ? 'Working…' : confirmLabel}
          </button>
        </>
      }
    >
      <div className="stack">
        <div>{message}</div>
        {typeToConfirm !== undefined && (
          <label className="field">
            <span>
              Type <code>{typeToConfirm}</code> to confirm
            </span>
            <input value={typed} onChange={(e) => setTyped(e.target.value)} autoFocus />
          </label>
        )}
        {error && <p className="alert alert-error">{error}</p>}
      </div>
    </Modal>
  );
}
