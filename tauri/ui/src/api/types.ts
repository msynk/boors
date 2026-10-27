// DTOs of the Boors API, copied from tauri/docs/api-contract.md (the source of truth). Every property is always
// present on the wire; `null` is explicit. 64-bit identifiers (insCode, parentInsCode, tracingNo, message id) are
// decimal strings because they do not fit in a JavaScript number.

export type DateOnly = string; // yyyy-MM-dd
export type IsoDateTime = string; // ISO 8601 with an offset

export interface SeriesPointDto { date: DateOnly; value: number }
export interface IndexMoveDto { insCode: string; name: string; value: number; changePercent: number | null }
export interface MacroQuoteDto { series: string; date: DateOnly; value: number; changePercent: number | null }

export interface MarketOverviewDto {
  session: DateOnly;
  indices: IndexMoveDto[];
  equityValue: number;
  fundValue: number;
  equityTrades: number;
  advancers: number;
  decliners: number;
  unchanged: number;
  notTraded: number;
  halted: number;
  buyQueues: number;
  buyQueueValue: number | null;
  sellQueues: number;
  sellQueueValue: number | null;
  individualNetFlow: number;
  individualPerCapitaBuy: number | null;
  individualPerCapitaSell: number | null;
  flowValuesEstimated: boolean;
  usd: MacroQuoteDto | null;
  coin: MacroQuoteDto | null;
  tedpixUsdChangePercent: number | null;
}

export interface BreadthDayDto {
  date: DateOnly;
  advancers: number;
  decliners: number;
  unchanged: number;
  notTraded: number;
  halted: number;
  lockedUp: number;
  lockedDown: number;
  equityValue: number;
}

export interface SectorPerformanceDto {
  sectorCode: string;
  sectorName: string | null;
  instruments: number;
  traded: number;
  value: number;
  /** Market-cap-weighted return as a fraction (0.01 = 1%). */
  weightedReturn: number | null;
  /** Median return as a fraction. */
  medianReturn: number | null;
  individualNetFlow: number;
  advancers: number;
  decliners: number;
}

export interface MoverDto {
  insCode: string;
  symbol: string;
  name: string | null;
  instrumentClass: string;
  dayStatus: string | null;
  close: number;
  changePercent: number | null;
  value: number;
  individualNetFlow: number | null;
}

export interface ScreenRowDto {
  insCode: string;
  symbol: string;
  name: string | null;
  instrumentClass: string;
  sectorCode: string | null;
  score: number;
  observations: number;
}

export interface InstrumentSearchResultDto {
  insCode: string;
  symbol: string;
  name: string | null;
  instrumentClass: string;
  parentInsCode: string | null;
}

export interface MarketWatchRequestDto {
  date: DateOnly | null;
  filter: string | null;
  classes: string[] | null;
  tradedOnly: boolean;
}

export interface MarketWatchEntryDto {
  insCode: string;
  symbol: string;
  name: string | null;
  instrumentClass: string;
  sectorCode: string | null;
  dayStatus: string | null;
  close: number;
  changePercent: number | null;
  last: number;
  lastChangePercent: number | null;
  volume: number;
  value: number;
  trades: number;
  allowedLow: number | null;
  allowedHigh: number | null;
  /** "buy" or "sell" when the close snapshot shows a queue at the band. */
  queue: string | null;
  queueValue: number | null;
  priceEarnings: number | null;
  marketCap: number | null;
  individualNetValue: number | null;
  perCapitaBuy: number | null;
  perCapitaSell: number | null;
  buyerPower: number | null;
  individualBuyShare: number | null;
}

export interface MarketWatchResultDto {
  session: DateOnly | null;
  entries: MarketWatchEntryDto[];
  evaluated: number;
  filterErrors: string[];
}

export interface InstrumentMetricsDto {
  insCode: string;
  symbol: string;
  name: string | null;
  instrumentClass: string;
  sectorCode: string | null;
  sectorName: string | null;
  asOf: DateOnly | null;
  dayStatus: string | null;
  close: number | null;
  changePercent: number | null;
  marketCap: number | null;
  high52Week: number | null;
  low52Week: number | null;
  fromHigh52WeekPercent: number | null;
  /** Total rial returns as fractions over calendar windows: "1w", "1m", "3m", "6m", "1y". */
  returns: Record<string, number | null>;
  /** Fraction. */
  usdReturn1Year: number | null;
  averageValue20: number | null;
  individualNetFlow20: number | null;
  perCapitaBuy: number | null;
  perCapitaSell: number | null;
  buyerPower: number | null;
  /** 0..1 against the instrument's own last ~250 sessions. */
  buyerPowerPercentile: number | null;
}

export interface BarDto {
  date: DateOnly;
  open: number;
  high: number;
  low: number;
  close: number;
  adjustedClose: number;
  volume: number;
  value: number;
  trades: number;
  status: string | null;
  /** Close / Yesterday − 1 as a fraction, on priced days only. */
  return: number | null;
  /** That day's permitted price band (raw rials), when the store has the session snapshot; null otherwise. */
  allowedLow: number | null;
  allowedHigh: number | null;
}

export interface FlowDayDto {
  date: DateOnly;
  buyIndividualValue: number;
  sellIndividualValue: number;
  buyInstitutionalValue: number;
  sellInstitutionalValue: number;
  buyIndividualCount: number;
  sellIndividualCount: number;
  individualNetValue: number;
  perCapitaBuy: number | null;
  perCapitaSell: number | null;
  buyerPower: number | null;
  valuesEstimated: boolean;
}

export interface CorporateEventDto {
  date: DateOnly;
  kind: 'adjustment' | 'share_change';
  source: string | null;
  factor: number | null;
  sharesBefore: number | null;
  sharesAfter: number | null;
}

export interface ShareholderDto {
  snapshotDate: DateOnly;
  holderName: string;
  shares: number;
  percent: number;
  change: number;
  changeAmount: number;
}

export interface CodalLetterDto {
  tracingNo: string;
  title: string;
  letterCode: string | null;
  letterTypeId: number | null;
  publishedAt: IsoDateTime | null;
  disclosureGroup: string | null;
  hasExcel: boolean;
  reportUrl: string | null;
  excelUrl: string | null;
  pdfUrl: string | null;
}

export interface SupervisorMessageDto {
  id: string;
  date: DateOnly;
  title: string;
  body: string | null;
  kind: string;
  reason: string;
  reopensWithoutLimit: boolean;
}

export type DatasetStatus = 'Ok' | 'Stale' | 'Empty' | 'Info';

export interface DatasetHealthDto {
  dataset: string;
  rows: number;
  latestDate: DateOnly | null;
  expectedDate: DateOnly | null;
  status: DatasetStatus;
}

export interface CollectionRunDto {
  job: string;
  node: string;
  startedAt: IsoDateTime;
  finishedAt: IsoDateTime | null;
  status: string;
  items: number;
  failures: number;
  message: string | null;
}

export interface SourceReachabilityDto { name: string; verdict: string; status: number | null; error: string | null }

export interface MarketDataHealthDto {
  generatedAt: IsoDateTime;
  node: string;
  expectedSession: DateOnly;
  healthy: boolean;
  datasets: DatasetHealthDto[];
  runs: CollectionRunDto[];
  lastProbeAt: IsoDateTime | null;
  sources: SourceReachabilityDto[];
}

export type JobState = 'Queued' | 'Running' | 'Succeeded' | 'Failed';

export interface JobStatusDto {
  id: string;
  job: string;
  state: JobState;
  queuedAt: IsoDateTime;
  startedAt: IsoDateTime | null;
  finishedAt: IsoDateTime | null;
  message: string | null;
}

export interface SyncPullRequestDto { nodeUrl: string; since: DateOnly | null }
export interface SyncPullResultDto { imported: number; skipped: number }

export interface AppInfoDto {
  version: string;
  node: string;
  dataRoot: string;
  transport: 'http' | 'ipc';
  collect: boolean;
}

export type MoverKind = 'gainers' | 'losers' | 'value' | 'inflow' | 'outflow';
export type ScreenKind = 'Momentum6To1' | 'High52WeekProximity' | 'IndividualFlowIntensity20' | 'BuyerPower5';
export type MacroSeries = 'price_dollar_rl' | 'sekee' | 'geram18' | 'ons' | 'price_eur';
export type JobName = 'instruments' | 'indices' | 'backfill' | 'eod' | 'funds' | 'macro' | 'codal' | 'messages' | 'halts' | 'all';

export const JOB_NAMES: readonly JobName[] = ['instruments', 'indices', 'backfill', 'eod', 'funds', 'macro', 'codal', 'messages', 'halts', 'all'];

/** Well-known instruments. */
export const TEDPIX = '32097828799138957';
export const EQUAL_WEIGHT_INDEX = '67130298613737946';
