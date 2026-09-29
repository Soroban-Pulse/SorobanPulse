interface SkeletonChartProps {
  height?: string;
}

export function SkeletonChart({ height = "300px" }: SkeletonChartProps) {
  return (
    <div className="skeleton skeleton-chart" style={{ height }} role="status" aria-label="Loading chart">
      <div className="skeleton-chart-bars">
        {Array.from({ length: 12 }).map((_, i) => (
          <div
            key={i}
            className="skeleton-chart-bar"
            style={{ height: `${20 + Math.random() * 80}%` }}
          />
        ))}
      </div>
    </div>
  );
}
