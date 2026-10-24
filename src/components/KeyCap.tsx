import { usePlatform } from "../lib/platform";
import { keyLabel, sortKeys } from "../lib/keys";

export function KeyCap({ name, size = "md" }: { name: string; size?: "sm" | "md" | "lg" }) {
  const platform = usePlatform();
  const l = keyLabel(name, platform);
  return (
    <kbd className={`keycap keycap-${size}`}>
      <span className="keycap-main">{l.main}</span>
      {l.sub ? <span className="keycap-sub">{l.sub}</span> : null}
    </kbd>
  );
}

export function KeyCombo({
  keys,
  size = "md",
  placeholder = "Not set",
}: {
  keys: string[];
  size?: "sm" | "md" | "lg";
  placeholder?: string;
}) {
  if (keys.length === 0) return <span className="keycombo-empty">{placeholder}</span>;
  return (
    <span className="keycombo">
      {sortKeys(keys).map((k, i) => (
        <span key={k} className="keycombo-item">
          {i > 0 ? <span className="keycombo-plus">+</span> : null}
          <KeyCap name={k} size={size} />
        </span>
      ))}
    </span>
  );
}
