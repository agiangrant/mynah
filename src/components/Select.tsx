import { ChevronDown } from "lucide-react";

export interface Option<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
}

interface Props<T extends string> {
  value: T;
  options: Option<T>[];
  onChange: (v: T) => void;
  disabled?: boolean;
  width?: number;
  ariaLabel?: string;
}

export function Select<T extends string>({
  value,
  options,
  onChange,
  disabled,
  width,
  ariaLabel,
}: Props<T>) {
  return (
    <span className="select" style={width ? { width } : undefined}>
      <select
        value={value}
        disabled={disabled}
        aria-label={ariaLabel}
        onChange={(e) => onChange(e.target.value as T)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value} disabled={o.disabled}>
            {o.label}
          </option>
        ))}
      </select>
      <ChevronDown size={14} className="select-chevron" />
    </span>
  );
}
