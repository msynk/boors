import { describe, expect, it } from 'vitest';
import { indexInDollars, tail } from './series';

describe('TEDPIX in dollars', () => {
  it('divides by the same-or-earlier dollar quote and indexes to 100 at the start', () => {
    const index = [
      { date: '2026-01-03', value: 100 },
      { date: '2026-01-04', value: 110 },
      { date: '2026-01-05', value: 121 },
    ];
    const usd = [
      { date: '2026-01-01', value: 10 },
      { date: '2026-01-04', value: 11 },
    ];
    const points = indexInDollars(index, usd);
    expect(points.map((p) => p.date)).toEqual(['2026-01-03', '2026-01-04', '2026-01-05']);
    expect(points[0].value).toBeCloseTo(100);
    expect(points[1].value).toBeCloseTo(100); // +10% in rial, +10% dollar
    expect(points[2].value).toBeCloseTo(110); // dollar unchanged since the 4th
  });

  it('skips index points before the first quote and returns nothing without quotes', () => {
    expect(indexInDollars([{ date: '2026-01-01', value: 5 }], [{ date: '2026-01-02', value: 1 }])).toEqual([]);
    expect(indexInDollars([{ date: '2026-01-01', value: 5 }], [])).toEqual([]);
  });

  it('takes the tail of a series for sparklines', () => {
    expect(tail([{ value: 1 }, { value: 2 }, { value: 3 }], 2)).toEqual([2, 3]);
    expect(tail(undefined, 5)).toEqual([]);
  });
});
