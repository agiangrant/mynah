export function ProgressBar({
  value,
  indeterminate = false,
}: {
  /** 0..1 */
  value: number;
  indeterminate?: boolean;
}) {
  const pct = Math.max(0, Math.min(1, value)) * 100;
  return (
    <div
      className={`progress ${indeterminate ? "indeterminate" : ""}`}
      role="progressbar"
      aria-valuenow={indeterminate ? undefined : Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="progress-fill" style={indeterminate ? undefined : { width: `${pct}%` }} />
    </div>
  );
}

export function DotMeter({ value, max = 5, label }: { value: number; max?: number; label: string }) {
  return (
    <span className="dotmeter" title={`${label}: ${value}/${max}`} aria-label={`${label} ${value} of ${max}`}>
      <span className="dotmeter-label">{label}</span>
      <span className="dotmeter-dots">
        {Array.from({ length: max }, (_, i) => (
          <span key={i} className={`dot ${i < value ? "on" : ""}`} />
        ))}
      </span>
    </span>
  );
}
