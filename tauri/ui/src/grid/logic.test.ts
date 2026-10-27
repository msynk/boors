import { describe, expect, it } from 'vitest';
import { filterIndices, sortByKeys, sortIndices, textRanks, toCsv, windowFor } from './logic';

interface Row {
  symbol: string;
  value: number | null;
}

const rows: Row[] = [
  { symbol: 'تاپیکو', value: 3 },
  { symbol: 'پارس', value: null },
  { symbol: 'بپاس', value: 10 },
  { symbol: 'آریا', value: 3 },
  { symbol: 'خودرو', value: -2 },
];

const collator = new Intl.Collator('fa');

describe('sortIndices', () => {
  it('sorts numbers both ways with nulls last and ties stable', () => {
    expect(sortIndices(rows, (r) => r.value, 'desc', 'number')).toEqual([2, 0, 3, 4, 1]);
    expect(sortIndices(rows, (r) => r.value, 'asc', 'number')).toEqual([4, 0, 3, 2, 1]);
  });

  it('sorts Persian text by collation (آ, ب, پ, ت, خ), not code points', () => {
    const order = sortIndices(rows, (r) => r.symbol, 'asc', 'text', collator).map((i) => rows[i].symbol);
    expect(order).toEqual(['آریا', 'بپاس', 'پارس', 'تاپیکو', 'خودرو']);
  });

  it('ranks text once and sorts by rank identically', () => {
    const ranks = textRanks(rows, (r) => r.symbol, collator);
    expect(sortByKeys(ranks, 'asc')).toEqual(sortIndices(rows, (r) => r.symbol, 'asc', 'text', collator));
    expect(sortByKeys(ranks, 'desc')).toEqual(sortIndices(rows, (r) => r.symbol, 'desc', 'text', collator));
  });

  it('sorts 3,100 rows well inside the 50 ms budget', () => {
    const big = Array.from({ length: 3100 }, (_, i) => ({ symbol: `s${(i * 7919) % 3100}`, value: Math.sin(i) * 1e6 }));
    const started = performance.now();
    for (let k = 0; k < 10; k++) sortIndices(big, (r) => r.value, k % 2 ? 'asc' : 'desc', 'number');
    expect((performance.now() - started) / 10).toBeLessThan(50);
  });
});

describe('quick search and windowing', () => {
  it('keeps the sorted order while filtering on precomputed keys', () => {
    const keys = rows.map((r) => r.symbol);
    expect(filterIndices([2, 0, 3, 4, 1], keys, 'پ')).toEqual([2, 0, 1]);
    expect(filterIndices([1, 0], keys, '')).toEqual([1, 0]);
  });

  it('snaps the rendered window to row blocks with overscan', () => {
    expect(windowFor(0, 600, 28, 3000)).toEqual({ start: 0, end: 22 + 20 + 8 });
    const a = windowFor(28 * 100, 600, 28, 3000);
    const b = windowFor(28 * 103, 600, 28, 3000);
    expect(a.start % 8).toBe(0);
    expect(a.start).toBe(b.start); // small scrolls do not change the DOM
    expect(a.start).toBeLessThanOrEqual(100 - 10);
    expect(windowFor(28 * 2990, 600, 28, 3000).end).toBe(3000);
  });
});

describe('CSV export', () => {
  it('quotes, keeps ASCII numbers and starts with a BOM', () => {
    const csv = toCsv(rows, [0, 1], [
      { header: 'Symbol', value: (r: Row) => r.symbol },
      { header: 'Value, rial', value: (r: Row) => r.value },
      { header: 'Note', value: () => 'say "hi"' },
    ]);
    expect(csv.startsWith('﻿')).toBe(true);
    const lines = csv.slice(1).split('\r\n');
    expect(lines[0]).toBe('Symbol,"Value, rial",Note');
    expect(lines[1]).toBe('تاپیکو,3,"say ""hi"""');
    expect(lines[2]).toBe('پارس,,"say ""hi"""');
  });
});
