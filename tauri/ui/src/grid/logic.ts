// Pure grid logic (sorting, quick search, CSV) over index arrays: rows are never copied or re-rendered wholesale; the
// grid only renders the slice of the order that is on screen.

export type SortDir = 'asc' | 'desc';
export type SortKind = 'number' | 'text';
export type SortValue = number | string | null | undefined;

export interface SortSpec {
  key: string;
  dir: SortDir;
}

export function identityOrder(length: number): number[] {
  const order = new Array<number>(length);
  for (let i = 0; i < length; i++) order[i] = i;
  return order;
}

/**
 * Row indices ordered by `value`. Nulls (and NaN) always sort last, whatever the direction; ties keep the incoming
 * order (stable). Numeric keys are extracted once into a Float64Array, so a 3,000-row sort is a few milliseconds.
 */
export function sortIndices<T>(
  rows: readonly T[],
  value: (row: T) => SortValue,
  dir: SortDir,
  kind: SortKind,
  collator?: Intl.Collator,
  base?: readonly number[],
): number[] {
  const order = base ? base.slice() : identityOrder(rows.length);
  const sign = dir === 'asc' ? 1 : -1;
  if (kind === 'number') {
    const keys = new Float64Array(rows.length);
    for (const i of order) {
      const v = value(rows[i]);
      keys[i] = typeof v === 'number' ? v : Number.NaN;
    }
    order.sort((a, b) => {
      const ka = keys[a];
      const kb = keys[b];
      const na = ka !== ka; // NaN
      const nb = kb !== kb;
      if (na || nb) return na === nb ? 0 : na ? 1 : -1;
      return ka === kb ? 0 : ka < kb ? -sign : sign;
    });
    return order;
  }
  const compare = (collator ?? new Intl.Collator('fa')).compare;
  const keys = new Array<string | null>(rows.length);
  for (const i of order) {
    const v = value(rows[i]);
    keys[i] = v === null || v === undefined || v === '' ? null : String(v);
  }
  order.sort((a, b) => {
    const ka = keys[a];
    const kb = keys[b];
    if (ka === null || kb === null) return ka === kb ? 0 : ka === null ? 1 : -1;
    return compare(ka, kb) * sign;
  });
  return order;
}

/** Indices ordered by precomputed numeric keys (NaN last in both directions; stable). */
export function sortByKeys(keys: Float64Array, dir: SortDir, base?: readonly number[]): number[] {
  const order = base ? base.slice() : identityOrder(keys.length);
  const sign = dir === 'asc' ? 1 : -1;
  order.sort((a, b) => {
    const ka = keys[a];
    const kb = keys[b];
    const na = ka !== ka;
    const nb = kb !== kb;
    if (na || nb) return na === nb ? 0 : na ? 1 : -1;
    return ka === kb ? 0 : ka < kb ? -sign : sign;
  });
  return order;
}

/**
 * Collation ranks of a text column (0 = first), NaN for empty values. Computing them once per dataset turns every
 * later sort on that column into a numeric sort.
 */
export function textRanks<T>(rows: readonly T[], value: (row: T) => SortValue, collator: Intl.Collator): Float64Array {
  const order = sortIndices(rows, value, 'asc', 'text', collator);
  const ranks = new Float64Array(rows.length).fill(Number.NaN);
  let rank = 0;
  let previous: string | null = null;
  for (const i of order) {
    const v = value(rows[i]);
    if (v === null || v === undefined || v === '') continue;
    const text = String(v);
    if (previous !== null && collator.compare(previous, text) !== 0) rank++;
    ranks[i] = rank;
    previous = text;
  }
  return ranks;
}

/** Keeps the indices whose precomputed (normalised) search key contains the (normalised) query. */
export function filterIndices(order: readonly number[], keys: readonly string[], query: string): number[] {
  if (!query) return order as number[];
  const out: number[] = [];
  for (const i of order) if (keys[i].includes(query)) out.push(i);
  return out;
}

export interface CsvColumn<T> {
  header: string;
  value: (row: T) => string | number | null | undefined;
}

function csvCell(value: string | number | null | undefined): string {
  if (value === null || value === undefined) return '';
  const text = typeof value === 'number' ? (Number.isFinite(value) ? String(value) : '') : value;
  return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

/** CSV with ASCII numbers (exports never use Persian digits), CRLF line ends and a BOM so Excel reads UTF-8. */
export function toCsv<T>(rows: readonly T[], order: readonly number[], columns: readonly CsvColumn<T>[], bom = true): string {
  const lines = [columns.map((c) => csvCell(c.header)).join(',')];
  for (const i of order) {
    const row = rows[i];
    lines.push(columns.map((c) => csvCell(c.value(row))).join(','));
  }
  return (bom ? '﻿' : '') + lines.join('\r\n') + '\r\n';
}

export function downloadText(filename: string, text: string, type = 'text/csv;charset=utf-8'): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

/**
 * The slice of rows to render for a scroll offset. The start snaps to blocks of `block` rows, so the DOM changes once
 * every few rows instead of on every frame: in between, scrolling is pure compositing over already-painted overscan.
 */
export function windowFor(scrollTop: number, viewport: number, rowHeight: number, total: number, overscan = 10, block = 8): { start: number; end: number } {
  const firstVisible = Math.floor(scrollTop / rowHeight);
  const start = Math.max(0, Math.floor(Math.max(0, firstVisible - overscan) / block) * block);
  const count = Math.ceil(viewport / rowHeight) + overscan * 2 + block;
  return { start, end: Math.min(total, start + count) };
}
