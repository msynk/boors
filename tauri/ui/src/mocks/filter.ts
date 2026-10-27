// A development-only stand-in for the server's TSETMC filter evaluator (the real one is a sandboxed interpreter in the
// Rust core). It understands the common variables, (ct).*, pd1…qo5, [ih][n].* and the code mode, enough to exercise the
// UI: syntax errors become a 400 problem, per-instrument failures become filterErrors.
import type { MarketWatchEntryDto } from '../api/types';
import { ProblemError } from '../api/transport';
import { normal, rngFor } from './rng';
import type { MockSnapshot } from './universe';

type Row = Record<string, unknown>;
type Predicate = (v: Row, ih: unknown, is: unknown) => unknown;

const SCALARS: Record<string, string> = {
  l18: 'l18',
  l30: 'l30',
  tno: 'tno',
  tvol: 'tvol',
  tval: 'tval',
  py: 'py',
  pf: 'pf',
  pmin: 'pmin',
  pmax: 'pmax',
  pl: 'pl',
  plc: 'plc',
  plp: 'plp',
  pc: 'pc',
  pcc: 'pcc',
  pcp: 'pcp',
  eps: 'eps',
  pe: 'pe',
  tmin: 'tmin',
  tmax: 'tmax',
  z: 'z',
  mv: 'mv',
  bvol: 'bvol',
  cs: 'cs',
  buyop: 'buyop',
  predtran: 'predtran',
};

/** Rewrites TSETMC syntax into plain JavaScript over a row object `v`. */
export function translate(filter: string): string {
  return filter
    .replace(/\(ct\)\s*\.\s*(\w+)/g, (_, name: string) => `v.ct.${name}`)
    .replace(/\[ih\]/g, 'ih')
    .replace(/\[is(\d+)\]/g, (_, n: string) => `is[${n}]`)
    .replace(/\((pd|zd|qd|po|zo|qo)([1-5])\)/g, (_, k: string, n: string) => `v.${k}${n}`)
    .replace(/\((\w+)\)/g, (match, name: string) => (SCALARS[name] ? `v.${SCALARS[name]}` : match));
}

export function compile(filter: string): Predicate {
  const body = translate(filter);
  try {
    return new Function('v', 'ih', 'is', `"use strict"; return (${body});`) as Predicate;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new ProblemError('Invalid filter', `SyntaxError: ${message}`, 400);
  }
}

function rowFor(e: MarketWatchEntryDto, s: MockSnapshot): Row {
  const queueBuy = e.queue === 'buy';
  const queueSell = e.queue === 'sell';
  const bid = queueBuy ? (e.allowedHigh ?? e.close) : Math.round(e.close * 0.995);
  const ask = queueSell ? (e.allowedLow ?? e.close) : Math.round(e.close * 1.005);
  const row: Row = {
    l18: e.symbol,
    l30: e.name,
    tno: e.trades,
    tvol: e.volume,
    tval: e.value,
    py: s.yesterday,
    pf: s.open,
    pmin: s.low,
    pmax: s.high,
    pl: e.last,
    plc: e.last - s.yesterday,
    plp: e.lastChangePercent ?? 0,
    pc: e.close,
    pcc: e.close - s.yesterday,
    pcp: e.changePercent ?? 0,
    pe: e.priceEarnings ?? 0,
    eps: e.priceEarnings ? e.close / e.priceEarnings : 0,
    tmin: e.allowedLow ?? 0,
    tmax: e.allowedHigh ?? 0,
    z: e.marketCap && e.close ? e.marketCap / e.close : 0,
    mv: e.marketCap ?? 0,
    bvol: Math.max(1, Math.round(e.volume / Math.max(0.3, 1 + normal(rngFor('bvol' + e.insCode), 0.4, 0.8)))),
    cs: Number(e.sectorCode ?? 0),
    buyop: 0,
    predtran: 0,
    ct: s.hasFlows
      ? {
          Buy_I_Volume: s.buyIndividualVolume,
          Sell_I_Volume: s.sellIndividualVolume,
          Buy_N_Volume: Math.max(0, e.volume - s.buyIndividualVolume),
          Sell_N_Volume: Math.max(0, e.volume - s.sellIndividualVolume),
          Buy_CountI: s.buyIndividualCount,
          Sell_CountI: s.sellIndividualCount,
          Buy_CountN: Math.round(s.buyIndividualCount / 40),
          Sell_CountN: Math.round(s.sellIndividualCount / 40),
        }
      : undefined,
  };
  for (let n = 1; n <= 5; n++) {
    row[`pd${n}`] = n === 1 ? bid : Math.round(bid * (1 - 0.002 * (n - 1)));
    row[`po${n}`] = n === 1 ? ask : Math.round(ask * (1 + 0.002 * (n - 1)));
    row[`qd${n}`] = queueSell && n === 1 ? 0 : Math.round(e.volume / (20 * n)) + (queueBuy && n === 1 ? Math.round((e.queueValue ?? 0) / Math.max(1, bid)) : 0);
    row[`qo${n}`] = queueBuy && n === 1 ? 0 : Math.round(e.volume / (22 * n)) + (queueSell && n === 1 ? Math.round((e.queueValue ?? 0) / Math.max(1, ask)) : 0);
    row[`zd${n}`] = Math.max(0, Math.round((row[`qd${n}`] as number) / 5000));
    row[`zo${n}`] = Math.max(0, Math.round((row[`qo${n}`] as number) / 5000));
  }
  return row;
}

/** A short closing-price history for [ih] (previous day first), without generating full bars for every row. */
function ihFor(e: MarketWatchEntryDto, s: MockSnapshot): unknown[] | undefined {
  if (e.instrumentClass === 'Option' || e.instrumentClass === 'Debt') return undefined;
  const rng = rngFor('ih' + e.insCode);
  const length = rng() < 0.04 ? 6 : 60;
  const out: unknown[] = [];
  let close = s.yesterday;
  for (let k = 0; k < length; k++) {
    out.push({ PClosing: Math.round(close), PDrCotVal: Math.round(close), ZTotTran: 100, QTotTran5J: e.volume, QTotCap: e.value, PriceMin: close * 0.98, PriceMax: close * 1.02, PriceYesterday: close, PriceFirst: close });
    close = close / (1 + normal(rng, 0.002, 0.018));
  }
  return out;
}

export interface FilterOutcome {
  entries: MarketWatchEntryDto[];
  errors: string[];
}

export function applyFilter(filter: string, rows: { entry: MarketWatchEntryDto; snap: MockSnapshot }[]): FilterOutcome {
  const predicate = compile(filter);
  const entries: MarketWatchEntryDto[] = [];
  const errors: string[] = [];
  const is = new Array(91).fill(0);
  const started = Date.now();
  for (const { entry, snap } of rows) {
    if (Date.now() - started > 1500) {
      throw new ProblemError('Filter timed out', 'The filter ran longer than the 1.5 s budget and was stopped.', 400);
    }
    try {
      if (predicate(rowFor(entry, snap), ihFor(entry, snap), is)) entries.push(entry);
    } catch (error) {
      errors.push(`${entry.symbol}: ${error instanceof Error ? `${error.name}: ${error.message}` : String(error)}`);
    }
  }
  return { entries, errors };
}
