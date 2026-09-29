interface StatTileProps {
  label: string;
  value: string;
}

export function StatTile({ label, value }: StatTileProps) {
  return (
    <div className="stat-tile">
      <dt className="stat-tile-label">{label}</dt>
      <dd className="stat-tile-value">{value}</dd>
    </div>
  );
}
