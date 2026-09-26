import type { ChannelType } from '../../api/types';

// Generic glyphs (not vendor logos) so each type is recognisable at a glance.
const PATHS: Record<ChannelType, string> = {
  slack: 'M9 3v18M15 3v18M3 9h18M3 15h18',
  discord: 'M4 5h16v11H9l-5 4z',
  telegram: 'M3 11l18-7-4 16-5-5-3 4v-6l9-7',
  email: 'M3 6h18v12H3zM3 6l9 7 9-7',
  sms: 'M7 3h10v18H7zM11 18h2',
  pagerduty: 'M12 3a6 6 0 016 6v5l2 3H4l2-3V9a6 6 0 016-6zM10 20h4',
  github: 'M6 3v12M6 15a3 3 0 100 6 3 3 0 000-6zM18 9a3 3 0 100-6 3 3 0 000 6zM18 9c0 6-12 3-12 6',
  webhook: 'M10 14a4 4 0 005.7 0l3-3a4 4 0 00-5.7-5.7l-1 1M14 10a4 4 0 00-5.7 0l-3 3a4 4 0 005.7 5.7l1-1',
};

export function ChannelIcon({ type, size = 18 }: { type: ChannelType; size?: number }) {
  return (
    <span className={`channel-icon channel-icon-${type}`} aria-hidden="true">
      <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
        <path d={PATHS[type] ?? PATHS.webhook} />
      </svg>
    </span>
  );
}
