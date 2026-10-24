interface Segment<T extends string> {
  value: T;
  label: string;
}

interface Props<T extends string> {
  value: T;
  segments: Segment<T>[];
  onChange: (v: T) => void;
  ariaLabel?: string;
}

export function SegmentedControl<T extends string>({ value, segments, onChange, ariaLabel }: Props<T>) {
  return (
    <div className="segmented" role="radiogroup" aria-label={ariaLabel}>
      {segments.map((s) => (
        <button
          key={s.value}
          type="button"
          role="radio"
          aria-checked={s.value === value}
          className={`segment ${s.value === value ? "active" : ""}`}
          onClick={() => onChange(s.value)}
        >
          {s.label}
        </button>
      ))}
    </div>
  );
}
