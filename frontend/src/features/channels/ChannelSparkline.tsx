import { channelsApi } from '../../api/channels';
import { Sparkline } from '../../components/charts/Sparkline';
import { useAsync } from '../../lib/useAsync';

/** 7-day delivery trend for one channel; loads lazily per row. */
export function ChannelSparkline({ channelId, name }: { channelId: string; name: string }) {
  const { data, error, loading } = useAsync(() => channelsApi.deliveryStats(channelId, '7d'), [channelId]);

  if (loading) return <span className="skeleton skeleton-inline" aria-label="Loading delivery stats" />;
  if (error || !data) return <span className="muted">—</span>;

  const points = data.data ?? [];
  return (
    <Sparkline
      values={points.map((p) => p.sent)}
      failures={points.map((p) => p.failed)}
      labels={points.map((p) => new Date(p.bucket_start).toLocaleDateString())}
      ariaLabel={`Deliveries for ${name}, last 7 days`}
    />
  );
}
