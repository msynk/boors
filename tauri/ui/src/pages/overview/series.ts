// Series helpers for the overview (pure, unit-tested).
import type { SeriesPointDto } from '../../api';

export interface Point {
  date: string;
  value: number;
}

/**
 * The index divided by the free-market dollar of the same (or the latest earlier) day, indexed to 100 at the first
 * point that has a dollar quote. One axis either way: never a dual-axis chart. Inputs are sorted by date.
 */
export function indexInDollars(index: readonly SeriesPointDto[], usd: readonly SeriesPointDto[]): Point[] {
  const out: Point[] = [];
  let j = 0;
  let rate: number | null = null;
  for (const point of index) {
    while (j < usd.length && usd[j].date <= point.date) rate = usd[j++].value;
    if (rate !== null && rate > 0) out.push({ date: point.date, value: point.value / rate });
  }
  const base = out[0]?.value;
  if (!base) return [];
  return out.map((p) => ({ date: p.date, value: (p.value / base) * 100 }));
}

/** The last `n` values of a series (for sparklines). */
export function tail(points: readonly { value: number }[] | undefined, n: number): number[] {
  if (!points || points.length === 0) return [];
  return points.slice(Math.max(0, points.length - n)).map((p) => p.value);
}
