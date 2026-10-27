// Trading calendar (Saturday–Wednesday, Nowruz closed) and anchored random series for indices and macro quotes.
import { isoToUtc, utcToIso } from '../format/format';
import { normal, type Rng } from './rng';

/** The mock's latest session: the live sample of docs/development-plan.md §9.3 (Saturday 1405/07/25). */
export const SESSION = '2026-10-17';
export const HISTORY_START = '2019-03-23';

const DAY = 86_400_000;

function isNowruz(iso: string): boolean {
  const md = iso.slice(5);
  return md >= '03-20' && md <= '04-02';
}

/** Iranian trading days: Saturday (6) to Wednesday (3), closed for Nowruz and a few fixed holidays. */
export function isTradingDay(iso: string): boolean {
  const dow = new Date(isoToUtc(iso)).getUTCDay();
  if (dow === 4 || dow === 5) return false;
  if (isNowruz(iso)) return false;
  const md = iso.slice(5);
  return md !== '02-11' && md !== '06-04';
}

export function tradingDays(from: string, to: string): string[] {
  const out: string[] = [];
  for (let t = isoToUtc(from); t <= isoToUtc(to); t += DAY) {
    const iso = utcToIso(t);
    if (isTradingDay(iso)) out.push(iso);
  }
  return out;
}

/** Days with a free-market quote (tgju publishes Saturday–Thursday). */
export function quoteDays(from: string, to: string): string[] {
  const out: string[] = [];
  for (let t = isoToUtc(from); t <= isoToUtc(to); t += DAY) {
    const iso = utcToIso(t);
    if (new Date(t).getUTCDay() !== 5) out.push(iso);
  }
  return out;
}

let tradingCache: string[] | null = null;
export function allTradingDays(): string[] {
  tradingCache ??= tradingDays(HISTORY_START, SESSION);
  return tradingCache;
}

/**
 * A series through fixed anchors (date → level): log-linear between anchors plus a Brownian bridge, so the level hits
 * every anchor exactly while wandering realistically in between.
 */
export function anchoredSeries(days: readonly string[], anchors: readonly [string, number][], sigma: number, rng: Rng): number[] {
  const values = new Array<number>(days.length);
  let a = 0;
  for (let i = 0; i < days.length; i++) {
    while (a < anchors.length - 2 && days[i] > anchors[a + 1][0]) a++;
    const [d0, v0] = anchors[a];
    const [d1, v1] = anchors[a + 1];
    const t0 = isoToUtc(d0);
    const t1 = isoToUtc(d1);
    const k = Math.min(1, Math.max(0, (isoToUtc(days[i]) - t0) / (t1 - t0 || 1)));
    values[i] = Math.log(v0) + (Math.log(v1) - Math.log(v0)) * k;
  }
  // Bridge noise between consecutive anchors.
  let start = 0;
  for (let s = 0; s < anchors.length - 1; s++) {
    let end = start;
    while (end < days.length - 1 && days[end + 1] <= anchors[s + 1][0]) end++;
    const n = end - start;
    if (n > 1) {
      const walk = new Array<number>(n + 1).fill(0);
      for (let i = 1; i <= n; i++) walk[i] = walk[i - 1] + normal(rng, 0, sigma);
      for (let i = 0; i <= n; i++) values[start + i] += walk[i] - (walk[n] * i) / n;
    }
    start = end;
  }
  return values.map(Math.exp);
}
