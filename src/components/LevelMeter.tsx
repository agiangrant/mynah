import { useEffect, useRef } from "react";

/**
 * Animated bar waveform driven by an RMS value.
 * `levelRef` is read every frame so parents can feed it from events without re-rendering.
 */
export function LevelMeter({
  levelRef,
  bars = 7,
  active = true,
  height = 28,
}: {
  levelRef: React.MutableRefObject<number>;
  bars?: number;
  active?: boolean;
  height?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const smooth = useRef<number[]>(Array.from({ length: bars }, () => 0));

  useEffect(() => {
    let raf = 0;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    // each bar gets a slightly different weight so the shape is organic, loudest in the middle
    const weights = Array.from({ length: bars }, (_, i) => {
      const c = (bars - 1) / 2;
      const d = Math.abs(i - c) / Math.max(c, 1);
      return 1 - d * 0.55;
    });
    const tick = () => {
      const el = ref.current;
      if (el) {
        // rms is ~0..0.3 for speech; map onto 0..1 with a soft curve
        const target = active ? Math.min(1, Math.pow(levelRef.current * 4, 0.7)) : 0;
        const t = Date.now() / 1000;
        for (let i = 0; i < bars; i++) {
          const jitter = active ? 0.08 * Math.sin(t * (6 + i) + i * 1.7) : 0;
          const goal = Math.max(0.08, (target + jitter) * weights[i]);
          const cur = smooth.current[i];
          smooth.current[i] = reduce ? goal : cur + (goal - cur) * (goal > cur ? 0.45 : 0.18);
          const bar = el.children[i] as HTMLElement | undefined;
          if (bar) bar.style.transform = `scaleY(${smooth.current[i].toFixed(3)})`;
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [bars, active, levelRef]);

  return (
    <div ref={ref} className={`levelmeter ${active ? "active" : ""}`} style={{ height }} aria-hidden>
      {Array.from({ length: bars }, (_, i) => (
        <span key={i} className="levelmeter-bar" />
      ))}
    </div>
  );
}
