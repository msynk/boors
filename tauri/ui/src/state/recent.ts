// Recently opened symbols (for the command palette's empty state). localStorage, try/catch-wrapped.
import { createSignal } from 'solid-js';

export interface RecentSymbol {
  insCode: string;
  symbol: string;
  name: string | null;
}

const KEY = 'boors.recent.v1';

function load(): RecentSymbol[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    return Array.isArray(parsed) ? parsed.filter((r): r is RecentSymbol => typeof r?.insCode === 'string' && typeof r?.symbol === 'string').slice(0, 8) : [];
  } catch {
    return [];
  }
}

const [recent, setRecent] = createSignal<RecentSymbol[]>(load());

export { recent };

export function rememberSymbol(entry: RecentSymbol): void {
  const next = [entry, ...recent().filter((r) => r.insCode !== entry.insCode)].slice(0, 8);
  setRecent(next);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // ignore
  }
}
