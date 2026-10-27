// The mock transport must answer exactly in the contract's shapes (tauri/docs/api-contract.md): every property
// present (null explicit, nothing omitted, nothing extra) and 64-bit ids as decimal strings.
import { describe, expect, it } from 'vitest';
import { ProblemError } from '../api/transport';
import { createMockTransport } from './index';

const mock = createMockTransport();
const get = (path: string, query?: Record<string, string | number | boolean | null>) => mock({ method: 'GET', path, query });
const post = (path: string, body?: unknown) => mock({ method: 'POST', path, body });

const KEYS = {
  overview: 'session indices equityValue fundValue equityTrades advancers decliners unchanged notTraded halted buyQueues buyQueueValue sellQueues sellQueueValue individualNetFlow individualPerCapitaBuy individualPerCapitaSell flowValuesEstimated usd coin tedpixUsdChangePercent',
  indexMove: 'insCode name value changePercent',
  macroQuote: 'series date value changePercent',
  breadth: 'date advancers decliners unchanged notTraded halted lockedUp lockedDown equityValue',
  sector: 'sectorCode sectorName instruments traded value weightedReturn medianReturn individualNetFlow advancers decliners',
  mover: 'insCode symbol name instrumentClass dayStatus close changePercent value individualNetFlow',
  screen: 'insCode symbol name instrumentClass sectorCode score observations',
  search: 'insCode symbol name instrumentClass parentInsCode',
  watchResult: 'session entries evaluated filterErrors',
  watchEntry:
    'insCode symbol name instrumentClass sectorCode dayStatus close changePercent last lastChangePercent volume value trades allowedLow allowedHigh queue queueValue priceEarnings marketCap individualNetValue perCapitaBuy perCapitaSell buyerPower individualBuyShare',
  metrics:
    'insCode symbol name instrumentClass sectorCode sectorName asOf dayStatus close changePercent marketCap high52Week low52Week fromHigh52WeekPercent returns usdReturn1Year averageValue20 individualNetFlow20 perCapitaBuy perCapitaSell buyerPower buyerPowerPercentile',
  bar: 'date open high low close adjustedClose volume value trades status return allowedLow allowedHigh',
  flow: 'date buyIndividualValue sellIndividualValue buyInstitutionalValue sellInstitutionalValue buyIndividualCount sellIndividualCount individualNetValue perCapitaBuy perCapitaSell buyerPower valuesEstimated',
  event: 'date kind source factor sharesBefore sharesAfter',
  holder: 'snapshotDate holderName shares percent change changeAmount',
  letter: 'tracingNo title letterCode letterTypeId publishedAt disclosureGroup hasExcel reportUrl excelUrl pdfUrl',
  message: 'id date title body kind reason reopensWithoutLimit',
  health: 'generatedAt node expectedSession healthy datasets runs lastProbeAt sources',
  dataset: 'dataset rows latestDate expectedDate status',
  run: 'job node startedAt finishedAt status items failures message',
  source: 'name verdict status error',
  job: 'id job state queuedAt startedAt finishedAt message',
  info: 'version node dataRoot transport collect',
  point: 'date value',
};

function expectShape(value: unknown, keys: string) {
  expect(value).toBeTypeOf('object');
  const actual = Object.keys(value as object).sort();
  expect(actual).toEqual(keys.split(' ').sort());
  for (const key of actual) expect((value as Record<string, unknown>)[key]).not.toBeUndefined();
}

const FOLAD = '46348559193224090';
const isId = (v: unknown) => typeof v === 'string' && /^\d+$/.test(v);

describe('mock transport: shapes per contract', () => {
  it('Market/GetOverview matches the live sample of plan §9.3', async () => {
    const o = (await get('Market/GetOverview')) as Record<string, unknown> & { indices: Record<string, unknown>[]; usd: unknown; coin: unknown };
    expectShape(o, KEYS.overview);
    o.indices.forEach((i) => expectShape(i, KEYS.indexMove));
    expectShape(o.usd, KEYS.macroQuote);
    expectShape(o.coin, KEYS.macroQuote);
    expect(o.indices[0]).toMatchObject({ insCode: '32097828799138957', value: 8265157, changePercent: -1.91 });
    expect(o).toMatchObject({ advancers: 146, decliners: 604, buyQueues: 71, sellQueues: 143, tedpixUsdChangePercent: -3.37 });
  });

  it('breadth, sectors, movers, screens, search and series', async () => {
    const breadth = (await get('Market/GetBreadth', { sessions: 60 })) as unknown[];
    expect(breadth).toHaveLength(60);
    breadth.forEach((b) => expectShape(b, KEYS.breadth));
    ((await get('Market/GetSectors')) as unknown[]).forEach((s) => expectShape(s, KEYS.sector));
    const gainers = (await get('Market/GetMovers/gainers', { take: 5 })) as { changePercent: number }[];
    expect(gainers).toHaveLength(5);
    gainers.forEach((m) => expectShape(m, KEYS.mover));
    expect(gainers[0].changePercent).toBeGreaterThanOrEqual(gainers[4].changePercent);
    ((await get('Market/GetScreen/Momentum6To1', { take: 10 })) as unknown[]).forEach((s) => expectShape(s, KEYS.screen));
    const found = (await get('Market/Search', { text: 'فولاد', take: 3 })) as { insCode: string; symbol: string }[];
    found.forEach((s) => expectShape(s, KEYS.search));
    expect(found[0]).toMatchObject({ insCode: FOLAD, symbol: 'فولاد' });
    ((await get('Market/GetIndexHistory/32097828799138957', { from: '2026-09-01' })) as unknown[]).forEach((p) => expectShape(p, KEYS.point));
    const usd = (await get('Market/GetMacroHistory/price_dollar_rl', { from: '2026-10-01' })) as { date: string; value: number }[];
    expect(usd.at(-1)).toEqual({ date: '2026-10-17', value: 2714431 });
  });

  it('market watch: ~3,100 instruments, class groups, traded-only', async () => {
    const all = (await post('Market/GetMarketWatch', { date: null, filter: null, classes: null, tradedOnly: false })) as { entries: { insCode: unknown }[]; evaluated: number };
    expectShape(all, KEYS.watchResult);
    expect(all.entries.length).toBeGreaterThan(3000);
    expectShape(all.entries[0], KEYS.watchEntry);
    expect(all.entries.every((e) => isId(e.insCode))).toBe(true);
    const shares = (await post('Market/GetMarketWatch', { date: null, filter: null, classes: ['TseStock', 'IfbStock', 'IfbBaseYellow', 'IfbBaseOrange', 'IfbBaseRed'], tradedOnly: true })) as {
      entries: { instrumentClass: string; trades: number }[];
    };
    expect(shares.entries.length).toBe(146 + 604 + 38);
    expect(shares.entries.every((e) => e.trades > 0 && /Stock|Base/.test(e.instrumentClass))).toBe(true);
    // Rules-key spelling is accepted too.
    const rights = (await post('Market/GetMarketWatch', { date: null, filter: null, classes: ['rights'], tradedOnly: false })) as { entries: unknown[] };
    expect(rights.entries.length).toBeGreaterThan(50);
  });

  it('market watch filters: syntax errors are 400 problems, per-row failures are filterErrors', async () => {
    const up = (await post('Market/GetMarketWatch', { date: null, filter: '(pc)>(py)*1.03', classes: null, tradedOnly: true })) as { entries: { changePercent: number }[] };
    expect(up.entries.every((e) => e.changePercent > 2.9)).toBe(true);
    const error = await post('Market/GetMarketWatch', { date: null, filter: '(pc)>(py', classes: null, tradedOnly: true }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ProblemError);
    expect((error as ProblemError).status).toBe(400);
    const partial = (await post('Market/GetMarketWatch', { date: null, filter: '(ct).Buy_I_Volume > 0', classes: null, tradedOnly: true })) as { filterErrors: string[] };
    expect(partial.filterErrors.length).toBeGreaterThan(0);
  });

  it('instrument routes for فولاد agree with the market watch', async () => {
    const metrics = (await get(`Instrument/GetMetrics/${FOLAD}`)) as { returns: Record<string, unknown>; close: number; asOf: string };
    expectShape(metrics, KEYS.metrics);
    expect(Object.keys(metrics.returns)).toEqual(['1w', '1m', '3m', '6m', '1y']);
    const bars = (await get(`Instrument/GetBars/${FOLAD}`)) as { date: string; close: number; adjustedClose: number }[];
    bars.slice(-3).forEach((b) => expectShape(b, KEYS.bar));
    expect(bars.length).toBeGreaterThan(1500);
    expect(bars.at(-1)).toMatchObject({ date: '2026-10-17', close: 4410 });
    expect(metrics).toMatchObject({ close: 4410, asOf: '2026-10-17' });
    // Point-in-time adjustment: the latest bar is unadjusted, older bars are scaled down by later capital events.
    expect(bars.at(-1)!.adjustedClose).toBe(4410);
    expect(bars[0].adjustedClose).toBeLessThan(bars[0].close);
    ((await get(`Instrument/GetFlows/${FOLAD}`, { from: '2026-10-01' })) as unknown[]).forEach((f) => expectShape(f, KEYS.flow));
    ((await get(`Instrument/GetEvents/${FOLAD}`)) as unknown[]).forEach((e) => expectShape(e, KEYS.event));
    ((await get(`Instrument/GetShareholders/${FOLAD}`)) as unknown[]).forEach((s) => expectShape(s, KEYS.holder));
    const letters = (await get(`Instrument/GetLetters/${FOLAD}`, { take: 5 })) as { tracingNo: unknown }[];
    expect(letters).toHaveLength(5);
    letters.forEach((l) => expectShape(l, KEYS.letter));
    expect(letters.every((l) => isId(l.tracingNo))).toBe(true);
    const messages = (await get(`Instrument/GetMessages/${FOLAD}`, { take: 5 })) as { id: unknown }[];
    messages.forEach((m) => expectShape(m, KEYS.message));
    expect(messages.every((m) => isId(m.id))).toBe(true);
    await expect(get('Instrument/GetMetrics/1')).rejects.toMatchObject({ status: 404 });
  });

  it('data health, jobs, sync and app info', async () => {
    const health = (await get('MarketData/GetHealth')) as { datasets: unknown[]; runs: unknown[]; sources: unknown[] };
    expectShape(health, KEYS.health);
    health.datasets.forEach((d) => expectShape(d, KEYS.dataset));
    health.runs.forEach((r) => expectShape(r, KEYS.run));
    health.sources.forEach((s) => expectShape(s, KEYS.source));
    const id = await post('MarketData/RunJob/macro');
    expect(id).toBeTypeOf('string');
    const jobs = (await get('MarketData/GetJobs')) as { id: string }[];
    expectShape(jobs[0], KEYS.job);
    expect(jobs[0].id).toBe(id);
    await expect(post('MarketData/RunJob/nope')).rejects.toMatchObject({ status: 400 });
    await expect(post('Sync/Pull', { nodeUrl: 'ftp://x', since: null })).rejects.toMatchObject({ status: 400 });
    expectShape(await get('App/GetInfo'), KEYS.info);
  });
});
