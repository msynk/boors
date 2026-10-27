// Typed endpoints of the contract. Every function takes optional CallOptions (abort signal, cache TTL, force).

import { client, type CallOptions } from './client';
import type {
  AppInfoDto,
  BarDto,
  BreadthDayDto,
  CodalLetterDto,
  CorporateEventDto,
  DateOnly,
  FlowDayDto,
  InstrumentMetricsDto,
  InstrumentSearchResultDto,
  JobName,
  JobStatusDto,
  MacroSeries,
  MarketDataHealthDto,
  MarketOverviewDto,
  MarketWatchRequestDto,
  MarketWatchResultDto,
  MoverDto,
  MoverKind,
  ScreenKind,
  ScreenRowDto,
  SectorPerformanceDto,
  SeriesPointDto,
  ShareholderDto,
  SupervisorMessageDto,
  SyncPullRequestDto,
  SyncPullResultDto,
} from './types';

export { client, type CallOptions } from './client';
export { ProblemError, isAbort } from './transport';
export * from './types';

const seg = (value: string) => encodeURIComponent(value);

const get = <T>(path: string, query?: Record<string, string | number | boolean | null | undefined>, options?: CallOptions) =>
  client.call<T>({ method: 'GET', path, query }, options);

export const market = {
  getOverview: (date?: DateOnly | null, o?: CallOptions) => get<MarketOverviewDto | null>('Market/GetOverview', { date }, o),
  getBreadth: (sessions = 60, o?: CallOptions) => get<BreadthDayDto[]>('Market/GetBreadth', { sessions }, o),
  getSectors: (date?: DateOnly | null, o?: CallOptions) => get<SectorPerformanceDto[]>('Market/GetSectors', { date }, o),
  getMovers: (kind: MoverKind, take = 20, date?: DateOnly | null, o?: CallOptions) =>
    get<MoverDto[]>(`Market/GetMovers/${seg(kind)}`, { take, date }, o),
  getScreen: (kind: ScreenKind, take = 50, o?: CallOptions) => get<ScreenRowDto[]>(`Market/GetScreen/${seg(kind)}`, { take }, o),
  getIndexHistory: (insCode: string, from?: DateOnly | null, o?: CallOptions) =>
    get<SeriesPointDto[]>(`Market/GetIndexHistory/${seg(insCode)}`, { from }, { ttl: 300_000, ...o }),
  getMacroHistory: (series: MacroSeries, from?: DateOnly | null, o?: CallOptions) =>
    get<SeriesPointDto[]>(`Market/GetMacroHistory/${seg(series)}`, { from }, { ttl: 300_000, ...o }),
  search: (text: string, take = 10, o?: CallOptions) => get<InstrumentSearchResultDto[]>('Market/Search', { text, take }, { ttl: 120_000, ...o }),
  // A read-only POST: cacheable by its body.
  getMarketWatch: (request: MarketWatchRequestDto, o?: CallOptions) =>
    client.call<MarketWatchResultDto>({ method: 'POST', path: 'Market/GetMarketWatch', body: request }, { ttl: 20_000, ...o }),
};

export const instrument = {
  getMetrics: (insCode: string, o?: CallOptions) => get<InstrumentMetricsDto>(`Instrument/GetMetrics/${seg(insCode)}`, undefined, o),
  getBars: (insCode: string, from?: DateOnly | null, o?: CallOptions) =>
    get<BarDto[]>(`Instrument/GetBars/${seg(insCode)}`, { from }, { ttl: 120_000, ...o }),
  getFlows: (insCode: string, from?: DateOnly | null, o?: CallOptions) =>
    get<FlowDayDto[]>(`Instrument/GetFlows/${seg(insCode)}`, { from }, { ttl: 120_000, ...o }),
  getEvents: (insCode: string, o?: CallOptions) => get<CorporateEventDto[]>(`Instrument/GetEvents/${seg(insCode)}`, undefined, { ttl: 300_000, ...o }),
  getShareholders: (insCode: string, o?: CallOptions) =>
    get<ShareholderDto[]>(`Instrument/GetShareholders/${seg(insCode)}`, undefined, { ttl: 300_000, ...o }),
  getLetters: (insCode: string, take = 50, o?: CallOptions) => get<CodalLetterDto[]>(`Instrument/GetLetters/${seg(insCode)}`, { take }, o),
  getMessages: (insCode: string, take = 50, o?: CallOptions) =>
    get<SupervisorMessageDto[]>(`Instrument/GetMessages/${seg(insCode)}`, { take }, o),
};

export const marketData = {
  getHealth: (o?: CallOptions) => get<MarketDataHealthDto>('MarketData/GetHealth', undefined, { ttl: 15_000, ...o }),
  runJob: (job: JobName, o?: CallOptions) =>
    client.call<string>({ method: 'POST', path: `MarketData/RunJob/${seg(job)}`, operator: true }, { ttl: 0, dedupe: false, ...o }),
  getJobs: (o?: CallOptions) => get<JobStatusDto[]>('MarketData/GetJobs', undefined, { ttl: 0, ...o }),
};

export const sync = {
  pull: (request: SyncPullRequestDto, o?: CallOptions) =>
    client.call<SyncPullResultDto>({ method: 'POST', path: 'Sync/Pull', body: request, operator: true }, { ttl: 0, dedupe: false, ...o }),
};

export const app = {
  getInfo: (o?: CallOptions) => get<AppInfoDto>('App/GetInfo', undefined, { ttl: 600_000, ...o }),
};
