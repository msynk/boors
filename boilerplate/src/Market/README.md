# Boors market data

The data layer for Tehran Stock Exchange (TSE) and Iran Fara Bourse (IFB) data. It has no dependency on the Blazor
stack, so the server, background jobs, the CLI and research tools can all use it.
See [docs/development-plan.md](../../../docs/development-plan.md) §3–§6 for the design, §9.2–§9.3 for the Phase 1 and Phase 2 status and
§9.4 for what the live data taught us.

| Project | What it is |
|---|---|
| `Boors.Market` | Domain records, write-once raw response cache, source clients and parsers (TSETMC, Codal, tgju, fipiran), dated market rules (`rules/rules.yaml`), day-status classifier, market calendar, the DuckDB market store, collectors, data health, contract checks, point-in-time adjustment |
| `Boors.Market.Cli` | `boors` developer and operations CLI |
| `Boors.Market.Tests` | MSTest unit, golden-file and end-to-end collector tests built on real responses captured on 2026-10-17 (no network) |

## How data flows

```
source ──HTTP (browser UA, retry, circuit breaker, token bucket)──► raw cache (write-once, gzip)
                                                                        │ parsers (pure)
                                                                        ▼
                                  classifier (day status, halts) ──► market store (DuckDB, idempotent merges)
                                                                        │
                                              data health ◄─────────────┤──► CLI queries, API, (P2) UI
```

1. A client sends the request through an `HttpClient` with a browser User-Agent and a resilience pipeline. The pipeline
   order is: total timeout, retry (exponential backoff with jitter, which also retries 403), circuit breaker,
   token-bucket rate limit, then a per-attempt timeout. Rate limits: TSETMC 2/s, Codal 0.5/s (it answers HTTP 429
   to bursts), tgju and fipiran 1/s.
2. The response is written to the raw cache before anything parses it:
   `raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{time}-{paramsHash}-{node}.json.gz` plus a `.meta.json` sidecar.
   Files are never overwritten. `latest/` holds one pointer per request.
3. Parsers turn bytes into domain records and normalize Persian text (ی/ک, digits, padding).
4. Collectors classify each daily row and merge everything into `market.duckdb`. The classifier statuses are:
   - `Traded`
   - `NoTrade`
   - `SymbolHalted` (with a halt reason)
   - `MarketClosed`
   - `ReopenAuction`
   - `QueueLockedUp` / `QueueLockedDown`

   Merges are idempotent and apply these precedence rules:
   - an official history row beats a same-day market-watch row;
   - official flows beat estimated flows;
   - the instrument class is taken from the best source available: fund type, then yVal, then ISIN.
5. The store is a projection of the cache: `boors rebuild` deletes it and rebuilds it from the cache without the
   network.

## Prices and returns

- **Raw prices are stored unchanged.**
- **Daily return:** `Close / Yesterday − 1`, computed on days with a real price. TSETMC's `priceYesterday` is the
  exchange's adjusted base price, so this already accounts for capital increases and dividends. Non-traded days get
  null, never zero.
- **Adjusted series:** built point-in-time from factors implied by the bars (`Yesterday ÷ previous Close`).
  TSETMC's published adjustment list misses most capital increases since 2020 and uses meeting dates.
  `boors verify adjustments <symbol>` shows the difference.

## CLI

Run from the repository root with `dotnet run --project src/Market/Boors.Market.Cli -- <command>`:

| Command | What it does |
|---|---|
| `probe` | Can this node reach every source (market data, Telegram/Bale, NuGet mirror, AvalAI)? |
| `collect all` | Run every job in dependency order (first run: add `--max 2000` to backfill all ~1,000 instruments, ~35 min) |
| `collect eod` | End-of-day append from 3 bulk calls (after the close; `--force` to snapshot mid-session) |
| `collect backfill --max N [--with-messages]` | Full history for instruments that have none yet |
| `collect instruments --details` / `indices --history` / `funds` / `macro` / `codal` / `messages` / `halts` | Individual jobs |
| `rebuild` | Rebuild `market.duckdb` from the raw cache only |
| `health` | Dataset freshness vs the expected session, last job runs, last probe |
| `query "SELECT …"` | SQL against the market store |
| `cache export bundle.zip [--since yyyy-MM-dd]` / `cache import bundle.zip` | Move raw responses between nodes |
| `import macro file.csv` | Monthly values with no machine-readable source; columns `series,period,period_end,value,source[,note]` |
| `market overview` / `breadth` / `sectors` / `movers <gainers\|losers\|value\|inflow\|outflow>` | What the overview pages show |
| `market metrics <symbol>` / `market search <text>` | Symbol header and returns card; command-palette search |
| `market screen <Momentum6To1\|High52WeekProximity\|IndividualFlowIntensity20\|BuyerPower5>` | Basic screens |
| `market watch --filter "(pc)>(py) && (ct).Buy_I_Volume>(ct).Sell_I_Volume"` | Market watch with a TSETMC filter (runs in the sandboxed Jint interpreter; `[ih]`, `[isN]` and code mode supported) |
| `verify mw-keys` / `verify adjustments <symbol>` / `verify contracts [--update]` / `verify reconcile [--count 20]` | Data-spec, adjustment, schema-drift and store-vs-TSETMC checks |
| `fetch …` | Phase 0 inspection commands (history, market watch, index, Codal) |

Global options:

| Option | Effect |
|---|---|
| `--data <dir>` | Where the raw cache and store live (default `.boors-data`, which git ignores) |
| `--node <name>` | Node name recorded on every raw entry; the env var `BOORS_NODE` does the same |
| `--cache` | Prefer cached responses (resume) |
| `--offline` | Use the cache only |
| `--verbose` | Log every request |

## Server

`Boors.Server.Api` registers the services (`MarketData` section in `appsettings.json`). It exposes:

- `GET api/v1/MarketData/GetHealth` and `POST api/v1/MarketData/RunJob/{job}`, both under the Operations_View policy;
- a `marketData` health check;
- the read API for the workstation pages: `api/v1/Market/*` (GetOverview, GetBreadth, GetSectors, GetMovers, GetScreen,
  Search, GetMarketWatch) and `api/v1/Instrument/*` (GetMetrics, GetBars, GetFlows, GetEvents, GetShareholders,
  GetLetters, GetMessages). Contracts and DTOs are in `src/Shared/Features/Market`.

When `MarketData:Collect` is `true`, the server also schedules the jobs in Hangfire in Iran time. Jobs run one at a
time per process because DuckDB allows one writer. Run one server instance per node.

## Tests

```bash
cd src/Market/Boors.Market.Tests && dotnet test
```

To refresh golden files or the contract baseline, see `Boors.Market.Tests/GoldenFiles/README.md` and run
`boors verify contracts --update`.
