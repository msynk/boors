// User settings: theme, language, digits, up/down colour preset and density. Persisted in localStorage (every access
// wrapped in try/catch: storage can be blocked) and mirrored onto <html> attributes that the CSS tokens key off.
import { createSignal } from 'solid-js';
import { configureFormat, type Lang } from '../format/format';
import { setLanguage } from '../i18n';

export type Theme = 'dark' | 'light';
export type Digits = 'auto' | 'persian' | 'latin';
export type ColorPreset = 'iran' | 'cb';
export type Density = 'pro' | 'lite';

export interface Settings {
  theme: Theme;
  lang: Lang;
  digits: Digits;
  colors: ColorPreset;
  density: Density;
}

const STORAGE_KEY = 'boors.settings.v1';

function narrowScreen(): boolean {
  try {
    return typeof window !== 'undefined' && window.matchMedia('(max-width: 700px)').matches;
  } catch {
    return false;
  }
}

export function defaultSettings(): Settings {
  return { theme: 'dark', lang: 'fa', digits: 'auto', colors: 'iran', density: narrowScreen() ? 'lite' : 'pro' };
}

export function parseSettings(raw: unknown): Settings {
  const d = defaultSettings();
  if (!raw || typeof raw !== 'object') return d;
  const s = raw as Partial<Record<keyof Settings, unknown>>;
  return {
    theme: s.theme === 'light' ? 'light' : s.theme === 'dark' ? 'dark' : d.theme,
    lang: s.lang === 'en' ? 'en' : s.lang === 'fa' ? 'fa' : d.lang,
    digits: s.digits === 'persian' || s.digits === 'latin' || s.digits === 'auto' ? s.digits : d.digits,
    colors: s.colors === 'cb' ? 'cb' : s.colors === 'iran' ? 'iran' : d.colors,
    density: s.density === 'lite' ? 'lite' : s.density === 'pro' ? 'pro' : d.density,
  };
}

function load(): Settings {
  try {
    const text = localStorage.getItem(STORAGE_KEY);
    return parseSettings(text ? JSON.parse(text) : null);
  } catch {
    return defaultSettings();
  }
}

function save(settings: Settings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Private window or blocked storage: the setting lasts until reload.
  }
}

export function usesPersianDigits(s: Settings): boolean {
  return s.digits === 'auto' ? s.lang === 'fa' : s.digits === 'persian';
}

/** Mirrors public/boot.js, plus the format and i18n configuration. */
export function applySettings(s: Settings): void {
  configureFormat({ lang: s.lang, persianDigits: usesPersianDigits(s) });
  setLanguage(s.lang);
  if (typeof document === 'undefined') return;
  const html = document.documentElement;
  html.lang = s.lang;
  html.dir = s.lang === 'fa' ? 'rtl' : 'ltr';
  html.dataset.theme = s.theme;
  html.dataset.colors = s.colors;
  html.dataset.density = s.density;
}

const initial = load();
applySettings(initial);

const [settings, setSettings] = createSignal<Settings>(initial);

export { settings };

export function updateSettings(patch: Partial<Settings>): void {
  const next = { ...settings(), ...patch };
  applySettings(next);
  save(next);
  setSettings(next);
}

/** Changes whenever text must be re-rendered (language or digits): the shell keys its routes on it. */
export function localeKey(): string {
  const s = settings();
  return `${s.lang}:${usesPersianDigits(s) ? 'fa' : 'latn'}`;
}

export const toggleTheme = () => updateSettings({ theme: settings().theme === 'dark' ? 'light' : 'dark' });
export const toggleLanguage = () => updateSettings({ lang: settings().lang === 'fa' ? 'en' : 'fa' });
export const toggleDigits = () => updateSettings({ digits: usesPersianDigits(settings()) ? 'latin' : 'persian' });
export const toggleColors = () => updateSettings({ colors: settings().colors === 'iran' ? 'cb' : 'iran' });
export const toggleDensity = () => updateSettings({ density: settings().density === 'pro' ? 'lite' : 'pro' });
