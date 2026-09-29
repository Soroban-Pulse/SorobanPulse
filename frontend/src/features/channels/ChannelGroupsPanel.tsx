import { channelsApi } from '../../api/channels';
import type { NotificationChannel } from '../../api/types';
import { AsyncBoundary } from '../../components/AsyncBoundary';
import { useAsync } from '../../lib/useAsync';
import { ChannelIcon } from './ChannelIcon';

export function ChannelGroupsPanel({ channels }: { channels: NotificationChannel[] }) {
  const groups = useAsync(() => channelsApi.groups(), []);
  const byId = new Map(channels.map((c) => [c.id, c]));

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Channel groups</h2>
      </header>
      <AsyncBoundary
        loading={groups.loading}
        error={groups.error}
        onRetry={groups.reload}
        empty={(groups.data?.data ?? []).length === 0}
        emptyMessage="No channel groups. Groups fan a notification out to several channels at once."
      >
        <ul className="group-list">
          {groups.data?.data.map((g) => (
            <li key={g.id} className="group-card">
              <div className="group-title">
                <strong>{g.name}</strong>
                <span className="muted">{g.channel_ids.length} channels</span>
              </div>
              {g.description && <p className="muted">{g.description}</p>}
              <ul className="chip-list">
                {g.channel_ids.map((id) => {
                  const c = byId.get(id);
                  return (
                    <li key={id} className="chip">
                      {c && <ChannelIcon type={c.channel_type} size={14} />}
                      {c ? c.name : <span className="mono">{id.slice(0, 8)}</span>}
                    </li>
                  );
                })}
              </ul>
            </li>
          ))}
        </ul>
      </AsyncBoundary>
    </section>
  );
}
