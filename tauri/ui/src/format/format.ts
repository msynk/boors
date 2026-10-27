// Port of boilerplate MarketFormat.cs: number, percentage and date conventions of the market pages.
//
// - Up/down follow the Iranian convention and always carry a sign and an arrow, so meaning never rests on colour.
// - همت (hemat) = 10^13 rial, billion toman = 10^10 rial, million toman = 10^7 rial.
// - Signed runs are left-to-right isolated (U+2066 … U+2069) so an RTL page cannot reorder "▲ +2.50%" into "2.50%+ ▲".
// - Dates are Jalali (yyyy/MM/dd) under fa and ISO under en.
// - Persian digits are a display option only; exports and storage stay ASCII.
//
// Pure module (no Solid): the shell remounts the routes when language or digits change.

export type Lang = 'fa' | 'en';
export type Tone = 'up' | 'down' | 'flat';

export const RIALS_PER_HEMAT = 1e13;
export const RIALS_PER_BILLION_TOMAN = 1e10;
export const RIALS_PER_MILLION_TOMAN = 1e7;

export const DASH = '–';
export const MINUS = '−';
export const LRI = '⁦';
export const PDI = '⁩';

interface FormatConfig {
  lang: Lang;
  persianDigits: boolean;
}

const config: FormatConfig = { lang: 'fa', persianDigits: false };

export function configureFormat(next: Partial<FormatConfig>): void {
  if (next.lang !== undefined) config.lang = next.lang;
  if (next.persianDigits !== undefined) config.persianDigits = next.persianDigits;
  dateCache.clear();
  dateTimeCache.clear();
}

export function formatConfig(): Readonly<FormatConfig> {
  return config;
}

const PERSIAN_DIGITS = ['۰', '۱', '۲', '۳', '۴', '۵', '۶', '۷', '۸', '۹'];

/** ASCII digits → Persian digits (when enabled). Leaves everything else alone. */
export function localDigits(text: string): string {
  if (!config.persianDigits) return text;
  let out = '';
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    out += c >= 48 && c <= 57 ? PERSIAN_DIGITS[c - 48] : text[i];
  }
  return out;
}

/** Digits plus the Persian thousands (٬), decimal (٫) and percent (٪) signs, for formatted numbers only. */
function localNumber(text: string): string {
  if (!config.persianDigits) return text;
  let out = '';
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    const c = text.charCodeAt(i);
    if (c >= 48 && c <= 57) out += PERSIAN_DIGITS[c - 48];
    else if (ch === ',') out += '٬';
    else if (ch === '.') out += '٫';
    else if (ch === '%') out += '٪';
    else out += ch;
  }
  return out;
}

/** Persian/Arabic-Indic digits and separators → ASCII (for parsing user input). */
export function asciiDigits(text: string): string {
  return text
    .replace(/[۰-۹]/g, (d) => String(d.charCodeAt(0) - 0x06f0))
    .replace(/[٠-٩]/g, (d) => String(d.charCodeAt(0) - 0x0660))
    .replace(/٬/g, ',')
    .replace(/٫/g, '.');
}

const isNum = (v: number | null | undefined): v is number => typeof v === 'number' && Number.isFinite(v);

/**
 * Rounds half away from zero (like .NET's "N" format), then groups thousands with ',' and uses '.' for decimals.
 * Returns ASCII; callers localise. The minus sign is U+2212.
 */
export function fixedAscii(value: number, decimals: number, group = true): string {
  const factor = 10 ** decimals;
  const scaled = Math.round(Number((Math.abs(value) * factor).toPrecision(15)));
  const negative = value < 0 && scaled !== 0;
  let intPart: string;
  let fracPart = '';
  if (decimals > 0) {
    const s = scaled.toString().padStart(decimals + 1, '0');
    intPart = s.slice(0, s.length - decimals);
    fracPart = s.slice(s.length - decimals);
  } else {
    intPart = scaled.toString();
  }
  if (group && intPart.length > 3) {
    let grouped = '';
    let count = 0;
    for (let i = intPart.length - 1; i >= 0; i--) {
      grouped = intPart[i] + grouped;
      if (++count % 3 === 0 && i > 0) grouped = ',' + grouped;
    }
    intPart = grouped;
  }
  return (negative ? MINUS : '') + intPart + (decimals > 0 ? '.' + fracPart : '');
}

/** A number with `decimals` places ("N0", "N1", "N2" in .NET terms), or "–". */
export function num(value: number | null | undefined, decimals = 0): string {
  return isNum(value) ? localNumber(fixedAscii(value, decimals)) : DASH;
}

export const price = (value: number | null | undefined) => num(value, 0);
export const integer = (value: number | null | undefined) => num(value, 0);
export const ratio = (value: number | null | undefined) => num(value, 2);
export const hemat = (rials: number | null | undefined) => (isNum(rials) ? num(rials / RIALS_PER_HEMAT, 2) : DASH);
export const billionToman = (rials: number | null | undefined) => (isNum(rials) ? num(rials / RIALS_PER_BILLION_TOMAN, 1) : DASH);
export const millionToman = (rials: number | null | undefined) => (isNum(rials) ? num(rials / RIALS_PER_MILLION_TOMAN, 0) : DASH);

/** An unsigned percentage: 45.2 → "45.2%". */
export function percent(value: number | null | undefined, decimals = 1): string {
  return isNum(value) ? localNumber(fixedAscii(value, decimals) + '%') : DASH;
}

/** A number with an explicit sign: "+1,234" / "−1,234" / "0". */
export function signed(value: number | null | undefined, decimals = 0): string {
  if (!isNum(value)) return DASH;
  const body = fixedAscii(value, decimals);
  return localNumber(body.startsWith(MINUS) || /^0(\.0*)?$/.test(body) ? body : '+' + body);
}

/** Direction of a percentage change, with MarketFormat's ±0.005 dead band. */
export function tone(percentValue: number | null | undefined): Tone {
  if (!isNum(percentValue)) return 'flat';
  if (percentValue > 0.005) return 'up';
  if (percentValue < -0.005) return 'down';
  return 'flat';
}

/** Direction of a money flow (any non-zero amount counts). */
export function flowTone(value: number | null | undefined): Tone {
  if (!isNum(value) || value === 0) return 'flat';
  return value > 0 ? 'up' : 'down';
}

/** "▲ +2.50%", "▼ −1.91%", "● 0.00%" or "–" — without bidi isolates (render it inside an LTR <bdi>). */
export function changeText(percentValue: number | null | undefined, decimals = 2): string {
  if (!isNum(percentValue)) return DASH;
  const t = tone(percentValue);
  if (t === 'up') return '▲ +' + localNumber(fixedAscii(percentValue, decimals) + '%');
  if (t === 'down') return '▼ ' + localNumber(fixedAscii(percentValue, decimals) + '%');
  return '● ' + localNumber(fixedAscii(0, decimals) + '%');
}

/** changeText wrapped in LRI…PDI for plain-text contexts (titles, canvas labels, aria). */
export function change(percentValue: number | null | undefined, decimals = 2): string {
  const text = changeText(percentValue, decimals);
  return text === DASH ? DASH : LRI + text + PDI;
}

/** A fraction (0.25 = 25%) as a signed change. */
export function fractionChangeText(fraction: number | null | undefined, decimals = 2): string {
  return changeText(isNum(fraction) ? fraction * 100 : null, decimals);
}

export function fractionTone(fraction: number | null | undefined): Tone {
  return tone(isNum(fraction) ? fraction * 100 : null);
}

// ---- dates -------------------------------------------------------------------------------------------------------

const jalaliFormatter = new Intl.DateTimeFormat('fa-IR-u-ca-persian-nu-latn', { year: 'numeric', month: '2-digit', day: '2-digit', timeZone: 'UTC' });
const jalaliMonthFormatter = new Intl.DateTimeFormat('fa-IR-u-ca-persian-nu-latn', { month: 'long', timeZone: 'UTC' });
const gregorianMonthFormatter = new Intl.DateTimeFormat('en-GB', { month: 'short', timeZone: 'UTC' });

export interface DateParts {
  year: number;
  month: number;
  day: number;
}

/** Milliseconds since the epoch at UTC midnight of a yyyy-MM-dd string (NaN when malformed). */
export function isoToUtc(iso: string): number {
  const y = Number(iso.slice(0, 4));
  const m = Number(iso.slice(5, 7));
  const d = Number(iso.slice(8, 10));
  if (!y || !m || !d) return Number.NaN;
  return Date.UTC(y, m - 1, d);
}

export function utcToIso(ms: number): string {
  const date = new Date(ms);
  const y = date.getUTCFullYear();
  const m = date.getUTCMonth() + 1;
  const d = date.getUTCDate();
  return `${y}-${m < 10 ? '0' : ''}${m}-${d < 10 ? '0' : ''}${d}`;
}

const jalaliCache = new Map<string, DateParts>();

/** Jalali year/month/day of a yyyy-MM-dd date (cached; Intl's Persian calendar). */
export function jalali(iso: string): DateParts {
  const key = iso.slice(0, 10);
  let parts = jalaliCache.get(key);
  if (!parts) {
    const ms = isoToUtc(key);
    parts = { year: 0, month: 0, day: 0 };
    if (Number.isFinite(ms)) {
      for (const p of jalaliFormatter.formatToParts(new Date(ms))) {
        if (p.type === 'year') parts.year = Number(p.value);
        else if (p.type === 'month') parts.month = Number(p.value);
        else if (p.type === 'day') parts.day = Number(p.value);
      }
    }
    if (jalaliCache.size > 20_000) jalaliCache.clear();
    jalaliCache.set(key, parts);
  }
  return parts;
}

const pad2 = (n: number) => (n < 10 ? '0' + n : String(n));

const dateCache = new Map<string, string>();

/** Jalali yyyy/MM/dd under fa, ISO yyyy-MM-dd under en, "–" for null. */
export function date(iso: string | null | undefined): string {
  if (!iso) return DASH;
  const key = iso.slice(0, 10);
  let text = dateCache.get(key);
  if (text === undefined) {
    if (config.lang === 'fa') {
      const j = jalali(key);
      text = j.year ? localDigits(`${j.year}/${pad2(j.month)}/${pad2(j.day)}`) : DASH;
    } else {
      text = key;
    }
    dateCache.set(key, text);
  }
  return text;
}

/** Short day label for dense axes: Jalali "MM/dd" under fa, "MM-dd" under en. */
export function shortDate(iso: string): string {
  if (config.lang === 'fa') {
    const j = jalali(iso);
    return localDigits(`${pad2(j.month)}/${pad2(j.day)}`);
  }
  return iso.slice(5, 10);
}

const jalaliMonthNames = new Map<number, string>();

/** The month name of a date: Jalali (مهر) under fa, Gregorian (Oct) under en. */
export function monthName(iso: string): string {
  if (config.lang === 'fa') {
    const j = jalali(iso);
    let name = jalaliMonthNames.get(j.month);
    if (!name) {
      name = jalaliMonthFormatter.format(new Date(isoToUtc(iso)));
      jalaliMonthNames.set(j.month, name);
    }
    return name;
  }
  return gregorianMonthFormatter.format(new Date(isoToUtc(iso)));
}

export function yearLabel(iso: string): string {
  return config.lang === 'fa' ? localDigits(String(jalali(iso).year)) : iso.slice(0, 4);
}

const dateTimeCache = new Map<string, string>();

/** A DateTimeOffset in the viewer's local time: "1405/07/25 13:35" (fa) or "2026-10-17 13:35" (en). */
export function dateTime(value: string | null | undefined): string {
  if (!value) return DASH;
  let text = dateTimeCache.get(value);
  if (text === undefined) {
    const at = new Date(value);
    if (Number.isNaN(at.getTime())) return DASH;
    const localIso = `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())}`;
    text = `${date(localIso)} ${localDigits(`${pad2(at.getHours())}:${pad2(at.getMinutes())}`)}`;
    if (dateTimeCache.size > 2000) dateTimeCache.clear();
    dateTimeCache.set(value, text);
  }
  return text;
}

/** The local calendar date (yyyy-MM-dd) of a DateTimeOffset. */
export function localIsoDate(value: string): string {
  const at = new Date(value);
  return `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())}`;
}

/** Whole seconds between two DateTimeOffsets as "12 s" / "3 min". */
export function duration(from: string, to: string | null): string | null {
  if (!to) return null;
  const seconds = Math.max(0, (new Date(to).getTime() - new Date(from).getTime()) / 1000);
  if (seconds < 120) return localDigits(`${Math.round(seconds)}`) + (config.lang === 'fa' ? ' ثانیه' : ' s');
  return localDigits(`${Math.round(seconds / 60)}`) + (config.lang === 'fa' ? ' دقیقه' : ' min');
}

/** yyyy-MM-dd of `iso` shifted by whole calendar months/years (end-of-month clamped). */
export function addCalendar(iso: string, months: number): string {
  const y = Number(iso.slice(0, 4));
  const m = Number(iso.slice(5, 7)) - 1;
  const d = Number(iso.slice(8, 10));
  const target = new Date(Date.UTC(y, m + months, 1));
  const lastDay = new Date(Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0)).getUTCDate();
  target.setUTCDate(Math.min(d, lastDay));
  return utcToIso(target.getTime());
}

export function addDays(iso: string, days: number): string {
  return utcToIso(isoToUtc(iso) + days * 86_400_000);
}

// ---- text ----------------------------------------------------------------------------------------------------------

/** Arabic yeh/kaf → Persian, strips ZWNJ/tatweel/diacritics, folds case: the key for symbol search. */
export function normalizeSearch(text: string): string {
  return text
    .replace(/[يى]/g, 'ی')
    .replace(/ك/g, 'ک')
    .replace(/ة/g, 'ه')
    .replace(/[أإآ]/g, 'ا')
    .replace(/‌|‍|ـ|[ً-ٟ]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();
}

/** Display normalisation for names that come with Arabic letters from TSETMC. */
export function persianText(text: string | null | undefined): string {
  if (!text) return '';
  return text.replace(/[يى]/g, 'ی').replace(/ك/g, 'ک').trim();
}
