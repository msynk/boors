// Colour maths for diverging scales: mixing in OKLab so the ramp is perceptually even and the midpoint stays grey.

type Rgb = [number, number, number];

export function hexToRgb(hex: string): Rgb {
  let h = hex.trim().replace('#', '');
  if (h.length === 3) h = h.split('').map((c) => c + c).join('');
  const n = Number.parseInt(h.slice(0, 6), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: Rgb): string {
  const c = (v: number) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0');
  return `#${c(r)}${c(g)}${c(b)}`;
}

const toLinear = (v: number) => {
  const x = v / 255;
  return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
};
const toSrgb = (v: number) => 255 * (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055);

function rgbToOklab([r, g, b]: Rgb): Rgb {
  const lr = toLinear(r);
  const lg = toLinear(g);
  const lb = toLinear(b);
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function oklabToRgb([L, a, b]: Rgb): Rgb {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    toSrgb(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    toSrgb(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    toSrgb(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

/** `a` → `b` at t ∈ [0, 1], interpolated in OKLab. */
export function mix(a: string, b: string, t: number): string {
  const x = rgbToOklab(hexToRgb(a));
  const y = rgbToOklab(hexToRgb(b));
  const k = Math.max(0, Math.min(1, t));
  return rgbToHex(oklabToRgb([x[0] + (y[0] - x[0]) * k, x[1] + (y[1] - x[1]) * k, x[2] + (y[2] - x[2]) * k]));
}

/** WCAG relative luminance (0 = black, 1 = white). */
export function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map(toLinear);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a: string, b: string): number {
  const x = luminance(a);
  const y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** Black or white ink, whichever reads better on `background`. */
export function inkOn(background: string, dark = '#0b0b0b', light = '#f7f7f5'): string {
  return contrast(background, dark) >= contrast(background, light) ? dark : light;
}

/**
 * A symmetric diverging scale: value / limit ∈ [−1, 1] maps grey → up or grey → down. Beyond the limit (reopening
 * auctions) the overflow colours apply. Light steps near zero keep small moves visibly different from none.
 */
export function divergingColor(value: number, limit: number, poles: { up: string; down: string; mid: string; upOverflow: string; downOverflow: string }): string {
  if (!Number.isFinite(value) || limit <= 0) return poles.mid;
  const t = value / limit;
  if (t > 1.1) return poles.upOverflow;
  if (t < -1.1) return poles.downOverflow;
  const magnitude = Math.min(1, Math.abs(t));
  if (magnitude < 0.02) return poles.mid;
  // Ease so that a third of the limit is already clearly coloured.
  const k = 0.18 + 0.82 * Math.sqrt(magnitude);
  return mix(poles.mid, t > 0 ? poles.up : poles.down, k);
}
