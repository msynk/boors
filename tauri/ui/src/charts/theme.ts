// Chart colours come from the CSS tokens (styles/tokens.css), so charts follow the theme and colour preset.
import { settings } from '../state/settings';

export interface ChartTheme {
  surface: string;
  text: string;
  text2: string;
  text3: string;
  grid: string;
  axis: string;
  accent: string;
  up: string;
  down: string;
  upText: string;
  downText: string;
  flat: string;
  mid: string;
  upOverflow: string;
  downOverflow: string;
  inactive: string;
  hatch: string;
  volume: string;
  event: string;
  warning: string;
  font: string;
  dark: boolean;
}

export function readTheme(): ChartTheme {
  const style = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    surface: v('--surface', '#1a1a19'),
    text: v('--text', '#f0efec'),
    text2: v('--text-2', '#c3c2b7'),
    text3: v('--text-3', '#9b9a93'),
    grid: v('--grid', '#2c2c2a'),
    axis: v('--axis', '#383835'),
    accent: v('--accent', '#3987e5'),
    up: v('--up', '#0ca30c'),
    down: v('--down', '#e66767'),
    upText: v('--up-text', '#22b33a'),
    downText: v('--down-text', '#ef7a7a'),
    flat: v('--flat', '#9b9a93'),
    mid: v('--div-mid', '#383835'),
    upOverflow: v('--up-overflow', '#5fd36f'),
    downOverflow: v('--down-overflow', '#ff9a9a'),
    inactive: v('--inactive', '#4a4945'),
    hatch: v('--hatch', '#6b6a66'),
    volume: v('--volume', '#6f7680'),
    event: v('--event', '#9085e9'),
    warning: v('--status-warning', '#fab219'),
    font: v('--font-family', 'Vazirmatn, system-ui, sans-serif'),
    dark: document.documentElement.dataset.theme !== 'light',
  };
}

/** Reactive: changes when the theme or colour preset changes (charts re-read their colours then). */
export function themeKey(): string {
  const s = settings();
  return `${s.theme}:${s.colors}`;
}

let fontReady: Promise<unknown> | null = null;

/** Canvas charts measure text once: wait for Vazirmatn before the first draw. */
export function whenFontReady(): Promise<unknown> {
  if (!fontReady) {
    fontReady =
      typeof document !== 'undefined' && document.fonts
        ? Promise.race([Promise.all([document.fonts.load('400 12px Vazirmatn'), document.fonts.load('600 12px Vazirmatn')]), new Promise((r) => setTimeout(r, 1500))])
        : Promise.resolve();
  }
  return fontReady;
}

export function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  } catch {
    return false;
  }
}
