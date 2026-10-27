// A tiny inline SVG line (no chart library): trend only, no axes. The tile beside it carries the exact numbers.
import { createMemo } from 'solid-js';
import type { Tone } from '../format/format';

export function sparkPath(values: number[], width: number, height: number, pad = 2): string {
  const finite = values.filter((v) => Number.isFinite(v));
  if (finite.length < 2) return '';
  let min = Infinity;
  let max = -Infinity;
  for (const v of finite) {
    if (v < min) min = v;
    if (v > max) max = v;
  }
  const span = max - min || 1;
  const step = (width - pad * 2) / (values.length - 1);
  let d = '';
  let pen = false;
  values.forEach((v, i) => {
    if (!Number.isFinite(v)) {
      pen = false;
      return;
    }
    const x = pad + i * step;
    const y = pad + (height - pad * 2) * (1 - (v - min) / span);
    d += `${pen ? 'L' : 'M'}${x.toFixed(1)} ${y.toFixed(1)}`;
    pen = true;
  });
  return d;
}

export function Sparkline(props: { values: number[]; tone: Tone; label: string; width?: number; height?: number }) {
  const w = () => props.width ?? 120;
  const h = () => props.height ?? 28;
  const d = createMemo(() => sparkPath(props.values, w(), h()));
  return (
    <svg class={`spark ${props.tone}`} viewBox={`0 0 ${w()} ${h()}`} preserveAspectRatio="none" role="img" aria-label={props.label}>
      <path d={d()} fill="none" stroke="currentColor" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linejoin="round" />
    </svg>
  );
}
