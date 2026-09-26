import { useState } from 'react';
import { CopyButton } from '../../components/CopyButton';
import { Modal } from '../../components/Modal';

/** One-time display of a new subscription's signing secret. */
export function SecretReveal({ secret, onClose }: { secret: string | null; onClose: () => void }) {
  const [acknowledged, setAcknowledged] = useState(false);

  const close = () => {
    setAcknowledged(false);
    onClose();
  };

  return (
    <Modal
      title="Save your signing secret"
      open={secret !== null}
      onClose={() => acknowledged && close()}
      footer={
        <button type="button" className="btn btn-primary" onClick={close} disabled={!acknowledged}>
          Done
        </button>
      }
    >
      <div className="stack">
        <p className="alert alert-warn" role="alert">
          This is the only time the secret will be shown. Store it in your secret manager now; if you lose it you
          will need to create a new subscription.
        </p>
        <div className="secret-box">
          <code className="mono">{secret}</code>
          {secret && <CopyButton value={secret} />}
        </div>
        <p className="help">
          Verify each delivery by computing HMAC-SHA256 of the raw request body with this secret and comparing it to the
          signature header.
        </p>
        <label className="field-check">
          <input type="checkbox" checked={acknowledged} onChange={(e) => setAcknowledged(e.target.checked)} />
          <span>I have stored the secret</span>
        </label>
      </div>
    </Modal>
  );
}
