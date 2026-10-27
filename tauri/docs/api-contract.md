# Boors API contract (tauri implementation)

The SolidJS UI (`ui/`) talks to the Rust core through one contract with two transports:

- **HTTP**, served by `boors-server` (Iran node, abroad node, any browser).
- **Tauri IPC**, served in-process by the desktop app (`apps/desktop`), which works offline.

Both transports call the same Rust dispatcher (`boors_market::api::dispatch`), so a route behaves identically on
both. The routes and JSON shapes match the original .NET API in `boilerplate/` (`IMarketController`,
`IInstrumentController`, `IMarketDataController` and the DTOs in `boilerplate/src/Shared/Features/Market*`). The
only differences are that there is no sign-in, and that the `Sync/*` routes are new.

## Transports

### HTTP

`{METHOD} /api/v1/{path}?{query}`. A POST sends a JSON body with `Content-Type: application/json`.

- A success is `200` with a JSON body.
- A failure is `4xx`/`5xx` with an RFC 7807 problem body: `{ "title": string, "detail": string | null, "status": number }`.
  - `400` means a bad argument or an unknown kind/job.
  - `404` means the instrument or route was not found.
  - `401` means a missing or wrong operator token.
  - `500` covers everything else.

### Tauri IPC

```ts
import { invoke } from '@tauri-apps/api/core';
const result = await invoke('api', { method: 'GET' | 'POST', path: string, query?: Record<string, string | number | boolean | null>, body?: unknown });
```

- `path` is everything after `/api/v1/`, including path parameters, e.g. `Instrument/GetBars/778253364357513`.
- The call resolves with the same JSON value the HTTP body would carry.
- It rejects with the same problem object, `{ title, detail, status }`.

The UI detects Tauri with `'__TAURI_INTERNALS__' in window`.

## JSON conventions

- Property names are camelCase and match the C# record parameter names: `insCode`, `changePercent`, ...
- Every property is always present. `null` is serialised explicitly; nothing is omitted.
- `DateOnly` is `"yyyy-MM-dd"`. `DateTimeOffset` is ISO 8601 with an offset, e.g. `"2026-10-17T13:35:02.123+03:30"` or a `Z` time.
- Rials and other `decimal` values are JSON numbers, and so are all `long`, `int` and `double` values.
  - Prices and values are rials.
  - A `*Percent` value is in percent (`2.5` means 2.5%). That covers `changePercent`, `lastChangePercent`, `fromHigh52WeekPercent`, `tedpixUsdChangePercent` and the `percent` of a shareholder.
  - Everything else that is a return, share or percentile is a **fraction**, where `0.025` means 2.5%. That covers `weightedReturn`, `medianReturn`, every value in `returns`, `usdReturn1Year`, `buyerPowerPercentile`, `individualBuyShare` and `BarDto.return`.
- `BarDto.allowedLow` / `allowedHigh` hold that day's permitted price band when the store knows it (from the session snapshot), and are `null` otherwise. The UI draws the daily-limit bands from them.
- Enum-valued strings use the C# enum member names:
  - `instrumentClass` is one of `Other, TseStock, IfbStock, IfbBaseYellow, IfbBaseOrange, IfbBaseRed, Rights, EtfEquity, EtfFixedIncome, EtfRealEstate, EtfMixed, FundLeveraged, FundGoldSilver, FundOfFunds, Option, Debt`.
  - `dayStatus` is one of `Traded, NoTrade, SymbolHalted, MarketClosed, ReopenAuction, QueueLockedUp, QueueLockedDown`.
  - The dataset `status` is one of `Ok, Stale, Empty, Info`.
  - For message `kind`/`reason` strings, see `Domain/MarketData.cs` (`MessageKind`, `HaltReason`).
- An omitted `date` query means the latest stored session.

## Routes

| Method | Path | Query / body | Returns |
|---|---|---|---|
| GET | `Market/GetOverview` | `date?` | `MarketOverviewDto \| null` |
| GET | `Market/GetBreadth` | `sessions` (default 60) | `BreadthDayDto[]` |
| GET | `Market/GetSectors` | `date?` | `SectorPerformanceDto[]` |
| GET | `Market/GetMovers/{kind}` | `take` (default 20), `date?`; kind: `gainers, losers, value, inflow, outflow` | `MoverDto[]` |
| GET | `Market/GetScreen/{kind}` | `take` (default 50); kind: `Momentum6To1, High52WeekProximity, IndividualFlowIntensity20, BuyerPower5` | `ScreenRowDto[]` |
| GET | `Market/GetIndexHistory/{insCode}` | `from?` (yyyy-MM-dd) | `SeriesPointDto[]` |
| GET | `Market/GetMacroHistory/{series}` | `from?`; series: `price_dollar_rl, sekee, geram18, ons, price_eur` | `SeriesPointDto[]` |
| GET | `Market/Search` | `text`, `take` (default 10) | `InstrumentSearchResultDto[]` |
| POST | `Market/GetMarketWatch` | body `MarketWatchRequestDto` | `MarketWatchResultDto` |
| GET | `Instrument/GetMetrics/{insCode}` | — | `InstrumentMetricsDto` (404 if unknown) |
| GET | `Instrument/GetBars/{insCode}` | `from?` | `BarDto[]` |
| GET | `Instrument/GetFlows/{insCode}` | `from?` | `FlowDayDto[]` |
| GET | `Instrument/GetEvents/{insCode}` | — | `CorporateEventDto[]` |
| GET | `Instrument/GetShareholders/{insCode}` | — | `ShareholderDto[]` |
| GET | `Instrument/GetLetters/{insCode}` | `take` (default 50) | `CodalLetterDto[]` |
| GET | `Instrument/GetMessages/{insCode}` | `take` (default 50) | `SupervisorMessageDto[]` |
| GET | `MarketData/GetHealth` | — | `MarketDataHealthDto` |
| POST | `MarketData/RunJob/{job}` | job: `instruments, indices, backfill, eod, funds, macro, codal, messages, halts, all` | `string` (the queued job id) |
| GET | `MarketData/GetJobs` | — | `JobStatusDto[]` (queued, running and the last 50 finished, newest first) |
| POST | `Sync/Pull` | body `{ "nodeUrl": string, "since": "yyyy-MM-dd" \| null }` | `{ "imported": number, "skipped": number }`: downloads `{nodeUrl}/api/v1/Sync/ExportBundle?since=` and imports it, then rebuilds the affected store tables |
| GET | `Sync/ExportBundle` | `since?` | HTTP only: `application/zip`, a raw-cache bundle (same format as `boors cache export`). Over IPC the desktop answers 404. |
| GET | `App/GetInfo` | — | `{ "version": string, "node": string, "dataRoot": string, "transport": "http" \| "ipc", "collect": boolean }` |

On a server, `MarketData/RunJob`, `Sync/Pull` and `Sync/ExportBundle` require the header
`Authorization: Bearer {BOORS_OPERATOR_TOKEN}` when that environment variable is set. Over IPC they need no token.

## DTOs

These are TypeScript shapes, the source of truth for the UI. The Rust structs serialise to exactly these.

```ts
type DateOnly = string; // yyyy-MM-dd

interface SeriesPointDto { date: DateOnly; value: number }
interface IndexMoveDto { insCode: string; name: string; value: number; changePercent: number | null }
interface MacroQuoteDto { series: string; date: DateOnly; value: number; changePercent: number | null }

interface MarketOverviewDto {
  session: DateOnly; indices: IndexMoveDto[];
  equityValue: number; fundValue: number; equityTrades: number;
  advancers: number; decliners: number; unchanged: number; notTraded: number; halted: number;
  buyQueues: number; buyQueueValue: number | null; sellQueues: number; sellQueueValue: number | null;
  individualNetFlow: number; individualPerCapitaBuy: number | null; individualPerCapitaSell: number | null;
  flowValuesEstimated: boolean; usd: MacroQuoteDto | null; coin: MacroQuoteDto | null; tedpixUsdChangePercent: number | null;
}

interface BreadthDayDto { date: DateOnly; advancers: number; decliners: number; unchanged: number; notTraded: number; halted: number; lockedUp: number; lockedDown: number; equityValue: number }

interface SectorPerformanceDto { sectorCode: string; sectorName: string | null; instruments: number; traded: number; value: number; weightedReturn: number | null; medianReturn: number | null; individualNetFlow: number; advancers: number; decliners: number }

interface MoverDto { insCode: string; symbol: string; name: string | null; instrumentClass: string; dayStatus: string | null; close: number; changePercent: number | null; value: number; individualNetFlow: number | null }

interface ScreenRowDto { insCode: string; symbol: string; name: string | null; instrumentClass: string; sectorCode: string | null; score: number; observations: number }

interface InstrumentSearchResultDto { insCode: string; symbol: string; name: string | null; instrumentClass: string; parentInsCode: string | null }

interface MarketWatchRequestDto { date: DateOnly | null; filter: string | null; classes: string[] | null; tradedOnly: boolean }

interface MarketWatchEntryDto {
  insCode: string; symbol: string; name: string | null; instrumentClass: string; sectorCode: string | null; dayStatus: string | null;
  close: number; changePercent: number | null; last: number; lastChangePercent: number | null;
  volume: number; value: number; trades: number; allowedLow: number | null; allowedHigh: number | null;
  queue: string | null; queueValue: number | null; priceEarnings: number | null; marketCap: number | null;
  individualNetValue: number | null; perCapitaBuy: number | null; perCapitaSell: number | null; buyerPower: number | null; individualBuyShare: number | null;
}
interface MarketWatchResultDto { session: DateOnly | null; entries: MarketWatchEntryDto[]; evaluated: number; filterErrors: string[] }

interface InstrumentMetricsDto {
  insCode: string; symbol: string; name: string | null; instrumentClass: string; sectorCode: string | null; sectorName: string | null;
  asOf: DateOnly | null; dayStatus: string | null; close: number | null; changePercent: number | null; marketCap: number | null;
  high52Week: number | null; low52Week: number | null; fromHigh52WeekPercent: number | null;
  returns: Record<string, number | null>; // keys exactly as MarketAnalytics.GetInstrumentMetrics builds them ("1w", "1m", ...)
  usdReturn1Year: number | null; averageValue20: number | null; individualNetFlow20: number | null;
  perCapitaBuy: number | null; perCapitaSell: number | null; buyerPower: number | null; buyerPowerPercentile: number | null;
}

interface BarDto { date: DateOnly; open: number; high: number; low: number; close: number; adjustedClose: number; volume: number; value: number; trades: number; status: string | null; return: number | null; allowedLow: number | null; allowedHigh: number | null }

interface FlowDayDto { date: DateOnly; buyIndividualValue: number; sellIndividualValue: number; buyInstitutionalValue: number; sellInstitutionalValue: number; buyIndividualCount: number; sellIndividualCount: number; individualNetValue: number; perCapitaBuy: number | null; perCapitaSell: number | null; buyerPower: number | null; valuesEstimated: boolean }

interface CorporateEventDto { date: DateOnly; kind: 'adjustment' | 'share_change'; source: string | null; factor: number | null; sharesBefore: number | null; sharesAfter: number | null }

interface ShareholderDto { snapshotDate: DateOnly; holderName: string; shares: number; percent: number; change: number; changeAmount: number }

interface CodalLetterDto { tracingNo: string; title: string; letterCode: string | null; letterTypeId: number | null; publishedAt: string | null; disclosureGroup: string | null; hasExcel: boolean; reportUrl: string | null; excelUrl: string | null; pdfUrl: string | null }

interface SupervisorMessageDto { id: string; date: DateOnly; title: string; body: string | null; kind: string; reason: string; reopensWithoutLimit: boolean }

interface MarketDataHealthDto { generatedAt: string; node: string; expectedSession: DateOnly; healthy: boolean; datasets: DatasetHealthDto[]; runs: CollectionRunDto[]; lastProbeAt: string | null; sources: SourceReachabilityDto[] }
interface DatasetHealthDto { dataset: string; rows: number; latestDate: DateOnly | null; expectedDate: DateOnly | null; status: 'Ok' | 'Stale' | 'Empty' | 'Info' }
interface CollectionRunDto { job: string; node: string; startedAt: string; finishedAt: string | null; status: string; items: number; failures: number; message: string | null }
interface SourceReachabilityDto { name: string; verdict: string; status: number | null; error: string | null }

interface JobStatusDto { id: string; job: string; state: 'Queued' | 'Running' | 'Succeeded' | 'Failed'; queuedAt: string; startedAt: string | null; finishedAt: string | null; message: string | null }
```

### 64-bit identifiers are strings (deliberate difference from the .NET API)

TSETMC InsCodes do not fit in a JavaScript number. فولاد is `46348559193224090` and TEDPIX is `32097828799138957`,
both above 2^53 ≈ 9.007e15. A JS `number` silently rounds them. So this contract serialises every 64-bit identifier as
a **JSON string** of its decimal digits:

- `insCode`
- `parentInsCode`
- `tracingNo`
- the supervisor-message `id`

The shapes above already use `string` for them. Path and query
parameters carry the same digits, e.g. `Instrument/GetBars/46348559193224090`. Counts, volumes and rial values stay
JSON numbers. A volume or value above 2^53 rials loses precision only beyond the 16th significant digit, which display
never needs.
