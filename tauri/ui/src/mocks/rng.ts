// Deterministic randomness for fixtures: the same seed always yields the same market.

export type Rng = () => number;

/** mulberry32: a tiny, fast, well-distributed 32-bit PRNG. */
export function mulberry32(seed: number): Rng {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** FNV-1a hash of a string, for per-instrument seeds. */
export function hashString(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export function rngFor(key: string): Rng {
  return mulberry32(hashString(key));
}

/** Standard normal via Box–Muller. */
export function normal(rng: Rng, mean = 0, sd = 1): number {
  const u = Math.max(rng(), 1e-12);
  const v = rng();
  return mean + sd * Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}

export function lognormal(rng: Rng, median: number, sigma: number): number {
  return median * Math.exp(normal(rng, 0, sigma));
}

export function uniform(rng: Rng, min: number, max: number): number {
  return min + (max - min) * rng();
}

export function int(rng: Rng, min: number, max: number): number {
  return Math.floor(uniform(rng, min, max + 1));
}

export function pick<T>(rng: Rng, items: readonly T[]): T {
  return items[Math.floor(rng() * items.length)];
}

export function shuffle<T>(rng: Rng, items: T[]): T[] {
  for (let i = items.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [items[i], items[j]] = [items[j], items[i]];
  }
  return items;
}

export const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

/** A 17-digit decimal id (above 2^53, like real TSETMC InsCodes). */
export function insCodeFrom(rng: Rng): string {
  const head = Math.floor(rng() * 8.9e7) + 1e7;
  const tail = Math.floor(rng() * 1e9);
  return `${head}${String(tail).padStart(9, '0')}`;
}
