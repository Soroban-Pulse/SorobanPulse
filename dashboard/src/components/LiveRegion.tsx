/**
 * Visually hidden polite live region. Screen readers announce changes to
 * `message` once the user is idle, without moving focus. Keep messages short
 * and summarised (e.g. "3 new events") rather than announcing every update.
 */
export function LiveRegion({ message }: { message: string }) {
  return (
    <p className="visually-hidden" role="status" aria-live="polite" aria-atomic="true">
      {message}
    </p>
  );
}
