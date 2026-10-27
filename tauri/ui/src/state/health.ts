// App-wide data freshness: MarketData/GetHealth polled every two minutes. Pages compare their own session with the
// expected session to show a stale banner, and the top bar shows a health chip (icon + label).
import { createSignal } from 'solid-js';
import { marketData, type MarketDataHealthDto } from '../api';

const [health, setHealth] = createSignal<MarketDataHealthDto | null>(null);
let timer: ReturnType<typeof setInterval> | undefined;

export async function refreshHealth(force = false): Promise<void> {
  try {
    setHealth(await marketData.getHealth({ force }));
  } catch {
    // Health is advisory: keep the last answer (or none) when it fails.
  }
}

export function useHealth() {
  if (timer === undefined) {
    void refreshHealth();
    timer = setInterval(() => void refreshHealth(true), 120_000);
  }
  return health;
}

/** The expected session from the health report, if any. */
export function expectedSession(): string | null {
  return health()?.expectedSession ?? null;
}

/** True when `session` is older than the session the node expected to have. */
export function isStale(session: string | null | undefined): boolean {
  const expected = expectedSession();
  return Boolean(session && expected && session < expected);
}
