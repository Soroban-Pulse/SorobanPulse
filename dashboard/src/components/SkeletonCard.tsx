interface SkeletonCardProps {
  width?: string;
  height?: string;
}

export function SkeletonCard({ width = "100%", height = "80px" }: SkeletonCardProps) {
  return (
    <div
      className="skeleton skeleton-card"
      style={{ width, height }}
      role="status"
      aria-label="Loading"
    />
  );
}
