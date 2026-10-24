import { useState } from "react";
import { Plus } from "lucide-react";
import { Chip } from "./Chip";

interface Props {
  items: string[];
  onChange: (items: string[]) => void;
  placeholder?: string;
  /** lower-cases and trims; rejects duplicates */
  normalize?: (s: string) => string;
  emptyText?: string;
}

export function ChipList({
  items,
  onChange,
  placeholder = "Add…",
  normalize = (s) => s.trim(),
  emptyText,
}: Props) {
  const [draft, setDraft] = useState("");

  const commit = () => {
    const parts = draft
      .split(",")
      .map(normalize)
      .filter((p) => p.length > 0 && !items.includes(p));
    if (parts.length > 0) onChange([...items, ...parts]);
    setDraft("");
  };

  return (
    <div className="chiplist">
      {items.length === 0 && emptyText ? <span className="chiplist-empty">{emptyText}</span> : null}
      {items.map((it) => (
        <Chip key={it} onRemove={() => onChange(items.filter((x) => x !== it))}>
          {it}
        </Chip>
      ))}
      <span className="chiplist-input">
        <Plus size={12} />
        <input
          value={draft}
          placeholder={placeholder}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              commit();
            } else if (e.key === "Backspace" && draft === "" && items.length > 0) {
              onChange(items.slice(0, -1));
            }
          }}
          onBlur={() => draft.trim() && commit()}
          size={Math.max(placeholder.length, draft.length + 1)}
        />
      </span>
    </div>
  );
}
