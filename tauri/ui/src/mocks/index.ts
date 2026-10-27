// Mock transport (development only: `npm run dev:mock`, i.e. VITE_MOCK=1). Serves every route of the contract from
// deterministic fixtures with realistic latency. Scenarios for checking page states: add ?mock=stale|empty|error|slow
// to the URL once (remembered for the tab; ?mock=normal resets).
import type { ApiRequest, Transport } from '../api/transport';
import { abortError, ProblemError } from '../api/transport';
import type { InstrumentSearchResultDto, MarketWatchRequestDto, MoverDto, ScreenRowDto } from '../api/types';
import { normalizeSearch } from '../format/format';
import { SESSION } from './calendar';
import { applyFilter } from './filter';
import { history, indexHistory, letters, macroHistory, metrics, quote, shareholders, since, tedpixReturns } from './history';
import { appInfo, getJobs, health, runJob, syncPull } from './ops';
import { clamp, lognormal, normal, rngFor, uniform } from './rng';
import { EQUITY_CLASSES, breadth, getUniverse, overview, sectors, type MockClass } from './universe';

type Scenario = 'normal' | 'stale' | 'empty' | 'error' | 'slow';

function scenario(): Scenario {
  try {
    const fromUrl = new URLSearchParams(location.search).get('mock');
    if (fromUrl) sessionStorage.setItem('boors.mock', fromUrl);
    const value = sessionStorage.getItem('boors.mock');
    return value === 'stale' || value === 'empty' || value === 'error' || value === 'slow' ? value : 'normal';
  } catch {
    return 'normal';
  }
}

function delay(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(abortError());
    const id = setTimeout(resolve, ms);
    signal?.addEventListener(
      'abort',
      () => {
        clearTimeout(id);
        reject(abortError());
      },
      { once: true },
    );
  });
}

const notFound = (what: string) => new ProblemError('Not Found', what, 404);

function classKey(value: string): string {
  return value.replace(/_/g, '').toLowerCase();
}

function search(text: string, take: number): InstrumentSearchResultDto[] {
  const q = normalizeSearch(text);
  if (!q) return [];
  const u = getUniverse();
  const scored: { score: number; liquidity: number; r: InstrumentSearchResultDto }[] = [];
  for (const i of u.instruments) {
    const symbol = normalizeSearch(i.symbol);
    const name = normalizeSearch(i.name);
    let score = -1;
    if (symbol === q) score = 0;
    else if (symbol.startsWith(q)) score = 1;
    else if (name.includes(q)) score = 2;
    if (score < 0) continue;
    const secondary = i.cls === 'Option' || i.cls === 'Debt' || i.cls === 'Rights' ? 1 : 0;
    scored.push({ score: score * 2 + secondary, liquidity: i.liquidity, r: { insCode: i.insCode, symbol: i.symbol, name: i.name, instrumentClass: i.cls, parentInsCode: i.parentInsCode } });
  }
  scored.sort((a, b) => a.score - b.score || b.liquidity - a.liquidity);
  return scored.slice(0, take).map((s) => s.r);
}

function movers(kind: string, take: number): MoverDto[] {
  const u = getUniverse();
  const traded = u.entries.filter((e) => EQUITY_CLASSES.includes(e.instrumentClass as MockClass) && e.trades > 0);
  const by: Record<string, (a: (typeof traded)[number], b: (typeof traded)[number]) => number> = {
    gainers: (a, b) => (b.changePercent ?? 0) - (a.changePercent ?? 0) || b.value - a.value,
    losers: (a, b) => (a.changePercent ?? 0) - (b.changePercent ?? 0) || b.value - a.value,
    value: (a, b) => b.value - a.value,
    inflow: (a, b) => (b.individualNetValue ?? -Infinity) - (a.individualNetValue ?? -Infinity),
    outflow: (a, b) => (a.individualNetValue ?? Infinity) - (b.individualNetValue ?? Infinity),
  };
  const compare = by[kind];
  if (!compare) throw new ProblemError('Unknown kind', `"${kind}" is not one of gainers, losers, value, inflow, outflow.`, 400);
  return traded
    .slice()
    .sort(compare)
    .slice(0, take)
    .map((e) => ({ insCode: e.insCode, symbol: e.symbol, name: e.name, instrumentClass: e.instrumentClass, dayStatus: e.dayStatus, close: e.close, changePercent: e.changePercent, value: e.value, individualNetFlow: e.individualNetValue }));
}

function screen(kind: string, take: number): ScreenRowDto[] {
  const known = ['Momentum6To1', 'High52WeekProximity', 'IndividualFlowIntensity20', 'BuyerPower5'];
  if (!known.includes(kind)) throw new ProblemError('Unknown screen', `"${kind}" is not one of ${known.join(', ')}.`, 400);
  const u = getUniverse();
  const rows = u.instruments
    .filter((i) => EQUITY_CLASSES.includes(i.cls))
    .map((i) => {
      const r = rngFor(kind + i.insCode);
      const score =
        kind === 'Momentum6To1'
          ? normal(r, 0.12 + i.oneYear * 0.2, 0.25)
          : kind === 'High52WeekProximity'
            ? clamp(uniform(r, 0.55, 1.02), 0.4, 1)
            : kind === 'IndividualFlowIntensity20'
              ? normal(r, -0.01, 0.06)
              : lognormal(r, 1, 0.35);
      const observations = kind === 'BuyerPower5' ? 5 : kind === 'IndividualFlowIntensity20' ? 20 : kind === 'High52WeekProximity' ? 236 : 104;
      return { insCode: i.insCode, symbol: i.symbol, name: i.name, instrumentClass: i.cls, sectorCode: i.sector, score, observations };
    });
  rows.sort((a, b) => b.score - a.score);
  return rows.slice(0, take);
}

function marketWatch(body: unknown, empty: boolean) {
  const request = (body ?? {}) as Partial<MarketWatchRequestDto>;
  if (empty) return { session: null, entries: [], evaluated: 0, filterErrors: [] };
  const u = getUniverse();
  const classes = request.classes ? new Set(request.classes.map(classKey)) : null;
  let rows = u.instruments
    .map((i) => ({ entry: u.snapshot.get(i.insCode)!.entry, snap: u.snapshot.get(i.insCode)! }))
    .filter(({ entry }) => !classes || classes.has(classKey(entry.instrumentClass)));
  if (request.tradedOnly) rows = rows.filter(({ entry }) => entry.trades > 0);
  const evaluated = rows.length;
  if (request.filter && request.filter.trim()) {
    const outcome = applyFilter(request.filter, rows);
    return { session: SESSION, entries: outcome.entries, evaluated, filterErrors: outcome.errors };
  }
  return { session: SESSION, entries: rows.map((r) => r.entry), evaluated, filterErrors: [] };
}

async function route(request: ApiRequest, mode: Scenario): Promise<unknown> {
  const [area, action, ...rest] = request.path.split('/').map(decodeURIComponent);
  const arg = rest.join('/');
  const q = request.query ?? {};
  const str = (key: string) => (q[key] === null || q[key] === undefined || q[key] === '' ? null : String(q[key]));
  const num = (key: string, fallback: number) => (q[key] === null || q[key] === undefined || q[key] === '' ? fallback : Number(q[key]));
  const empty = mode === 'empty';

  if (mode === 'error' && !(area === 'App' && action === 'GetInfo')) {
    throw new ProblemError('Internal Server Error', `Mock failure for ${request.path} (scenario "error").`, 500);
  }

  switch (`${area}/${action}`) {
    case 'Market/GetOverview':
      return empty ? null : overview(quote('price_dollar_rl'), quote('sekee'));
    case 'Market/GetBreadth':
      return empty ? [] : breadth(clamp(num('sessions', 60), 1, 400), tedpixReturns());
    case 'Market/GetSectors':
      return empty ? [] : sectors();
    case 'Market/GetMovers':
      return empty ? [] : movers(arg, num('take', 20));
    case 'Market/GetScreen':
      return empty ? [] : screen(arg, num('take', 50));
    case 'Market/GetIndexHistory': {
      const s = indexHistory(arg);
      if (!s) throw notFound(`Index ${arg} is unknown.`);
      return empty ? [] : since(s, str('from'));
    }
    case 'Market/GetMacroHistory': {
      const s = macroHistory(arg);
      if (!s) throw new ProblemError('Unknown series', `"${arg}" is not one of price_dollar_rl, sekee, geram18, ons, price_eur.`, 400);
      return empty ? [] : since(s, str('from'));
    }
    case 'Market/Search':
      return search(str('text') ?? '', num('take', 10));
    case 'Market/GetMarketWatch':
      return marketWatch(request.body, empty);
    case 'Instrument/GetMetrics': {
      if (empty) throw notFound(`Instrument ${arg} has no data.`);
      const m = metrics(arg, macroHistory('price_dollar_rl')!);
      if (!m) throw notFound(`Instrument ${arg} is unknown.`);
      return m;
    }
    case 'Instrument/GetBars': {
      const h = history(arg);
      if (!h) throw notFound(`Instrument ${arg} is unknown.`);
      return since(h.bars, str('from'));
    }
    case 'Instrument/GetFlows': {
      const h = history(arg);
      if (!h) throw notFound(`Instrument ${arg} is unknown.`);
      return since(h.flows, str('from'));
    }
    case 'Instrument/GetEvents':
      return history(arg)?.events ?? [];
    case 'Instrument/GetShareholders':
      return shareholders(arg);
    case 'Instrument/GetLetters':
      return letters(arg, num('take', 50));
    case 'Instrument/GetMessages':
      return (history(arg)?.messages ?? []).slice(0, num('take', 50));
    case 'MarketData/GetHealth':
      return health(mode === 'stale');
    case 'MarketData/RunJob':
      return runJob(arg);
    case 'MarketData/GetJobs':
      return getJobs();
    case 'Sync/Pull':
      return syncPull(request.body);
    case 'Sync/ExportBundle':
      throw notFound('Sync/ExportBundle is served over HTTP only.');
    case 'App/GetInfo':
      return appInfo();
    default:
      throw notFound(`No route for ${request.method} ${request.path}.`);
  }
}

export function createMockTransport(): Transport {
  return async (request) => {
    const mode = scenario();
    const base = request.path.startsWith('Market/GetMarketWatch') ? 160 : 70;
    await delay((base + Math.random() * 120) * (mode === 'slow' ? 12 : 1), request.signal);
    const result = await route(request, mode);
    // A JSON round trip, like the wire: callers can never mutate the fixtures.
    return result === undefined ? null : JSON.parse(JSON.stringify(result));
  };
}
