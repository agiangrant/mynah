import type { Platform } from "./platform";

export interface KeyLabel {
  main: string;
  /** small qualifier such as "left" / "right" */
  sub?: string;
}

const PUNCT: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  LeftBracket: "[",
  RightBracket: "]",
  SemiColon: ";",
  Quote: "'",
  BackSlash: "\\",
  IntlBackslash: "\\",
  Comma: ",",
  Dot: ".",
  Slash: "/",
  BackQuote: "`",
  KpMinus: "Num -",
  KpPlus: "Num +",
  KpMultiply: "Num *",
  KpDivide: "Num /",
  KpReturn: "Num Enter",
  KpDelete: "Num Del",
};

/** Pretty label for an rdev key name, with per-platform symbols. */
export function keyLabel(key: string, platform: Platform): KeyLabel {
  const mac = platform === "macos";
  const side = (k: string): string | undefined =>
    k.endsWith("Left") ? "left" : k.endsWith("Right") ? "right" : undefined;

  if (key === "MetaLeft" || key === "MetaRight") {
    return { main: mac ? "⌘" : platform === "windows" ? "Win" : "Super", sub: side(key) };
  }
  if (key === "Alt") return { main: mac ? "⌥" : "Alt", sub: mac ? "left" : undefined };
  if (key === "AltGr") return { main: mac ? "⌥" : "AltGr", sub: mac ? "right" : undefined };
  if (key === "ControlLeft" || key === "ControlRight") {
    return { main: mac ? "⌃" : "Ctrl", sub: side(key) };
  }
  if (key === "ShiftLeft" || key === "ShiftRight") {
    return { main: mac ? "⇧" : "Shift", sub: side(key) };
  }
  switch (key) {
    case "Function":
      return { main: "fn" };
    case "Space":
      return { main: "Space" };
    case "Return":
      return { main: mac ? "↩" : "Enter" };
    case "Escape":
      return { main: "esc" };
    case "Tab":
      return { main: mac ? "⇥" : "Tab" };
    case "Backspace":
      return { main: mac ? "⌫" : "Backspace" };
    case "Delete":
      return { main: mac ? "⌦" : "Del" };
    case "CapsLock":
      return { main: mac ? "⇪" : "Caps" };
    case "LeftArrow":
      return { main: "←" };
    case "RightArrow":
      return { main: "→" };
    case "UpArrow":
      return { main: "↑" };
    case "DownArrow":
      return { main: "↓" };
    case "Home":
      return { main: mac ? "↖" : "Home" };
    case "End":
      return { main: mac ? "↘" : "End" };
    case "PageUp":
      return { main: mac ? "⇞" : "PgUp" };
    case "PageDown":
      return { main: mac ? "⇟" : "PgDn" };
    case "Insert":
      return { main: "Ins" };
    case "PrintScreen":
      return { main: "PrtSc" };
    case "ScrollLock":
      return { main: "ScrLk" };
    case "NumLock":
      return { main: "NumLk" };
    case "Pause":
      return { main: "Pause" };
  }
  if (PUNCT[key]) return { main: PUNCT[key] };
  let m = /^Key([A-Z])$/.exec(key);
  if (m) return { main: m[1] };
  m = /^Num(\d)$/.exec(key);
  if (m) return { main: m[1] };
  m = /^Kp(\d)$/.exec(key);
  if (m) return { main: `Num ${m[1]}` };
  m = /^F(\d{1,2})$/.exec(key);
  if (m) return { main: `F${m[1]}` };
  m = /^Unknown\((\d+)\)$/.exec(key);
  if (m) return { main: `Key ${m[1]}` };
  return { main: key };
}

const MODIFIER_ORDER = [
  "Function",
  "ControlLeft",
  "ControlRight",
  "Alt",
  "AltGr",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
];

/** Stable, human order: modifiers first in the conventional order, then the rest. */
export function sortKeys(keys: string[]): string[] {
  return [...keys].sort((a, b) => {
    const ia = MODIFIER_ORDER.indexOf(a);
    const ib = MODIFIER_ORDER.indexOf(b);
    if (ia !== -1 || ib !== -1) {
      if (ia === -1) return 1;
      if (ib === -1) return -1;
      return ia - ib;
    }
    return a.localeCompare(b);
  });
}

export function shortcutText(keys: string[], platform: Platform): string {
  return sortKeys(keys)
    .map((k) => {
      const l = keyLabel(k, platform);
      return l.sub ? `${l.main} (${l.sub})` : l.main;
    })
    .join(platform === "macos" ? "" : " + ");
}
