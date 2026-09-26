const TONES: Record<string, 'ok' | 'warn' | 'bad' | 'muted' | 'info'> = {
  active: 'ok',
  healthy: 'ok',
  success: 'ok',
  delivered: 'ok',
  paused: 'warn',
  retrying: 'warn',
  pending: 'info',
  disabled: 'muted',
  cancelled: 'muted',
  unknown: 'muted',
  unhealthy: 'bad',
  failed: 'bad',
};

export function StatusBadge({ status, label }: { status: string; label?: string }) {
  const tone = TONES[status] ?? 'muted';
  return (
    <span className={`badge badge-${tone}`}>
      <span className="badge-dot" aria-hidden="true" />
      {label ?? status}
    </span>
  );
}

export function HttpStatus({ code }: { code: number | null | undefined }) {
  if (code === null || code === undefined) return <span className="muted">—</span>;
  const tone = code >= 200 && code < 300 ? 'ok' : code >= 500 || code === 0 ? 'bad' : 'warn';
  return <span className={`badge badge-${tone} mono`}>{code}</span>;
}
