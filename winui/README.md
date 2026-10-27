# Boors — native Windows workstation (WinUI 3)

A native Windows implementation of **Boors**, the analysis workstation for the Iranian stock market (TSE / IFB),
built with **WinUI 3 (Windows App SDK 2.5) and .NET 10**. It reuses the market-data layer of the original
.NET + Blazor implementation (`boilerplate/src/Market`, copied here) and adds:

- **Boors.Desktop** — the WinUI 3 workstation: overview, market watch (دیده‌بان), symbol workspace, screens, data
  health and settings, Persian (RTL, Jalali, Persian digits) by default and English, dark/light, with an in-process
  DuckDB store (the offline replica) or a remote node.
- **Boors.Node** — a lean ASP.NET Core minimal-API node: the boilerplate's market API with the same routes and JSON,
  data health, the collection job queue with the Iran-time schedule, and raw-cache sync between nodes.

Everything under `winui/` is self-contained: no project, import, props file or runtime path points outside this folder.

## Project map

| Project | What it is |
|---|---|
| `src/Boors.Market` | Market-data library (copied from the boilerplate): TSETMC/Codal/tgju/fipiran clients with resilience, write-once raw cache, parsers, dated rules (`rules/rules.yaml`, embedded), day-status classifier, DuckDB `MarketStore`, collectors, analytics, TSETMC filter (Jint), data health |
| `src/Boors.Market.Cli` | `boors` CLI (copied): probe, collect, rebuild, health, query, market views, cache bundles |
| `src/Boors.Market.Tests` | 117 offline MSTest tests on captured golden files (copied, plus missed-session repair) |
| `src/Boors.Market.Services` | Application layer shared by node and desktop: API DTOs + source-generated JSON, the `IMarketApi` contract with **local** (in-process DuckDB) and **remote** (HTTP) implementations, the single-writer **job queue** + worker, the **Asia/Tehran scheduler**, **node sync** (bundle export/pull) |
| `src/Boors.Market.Services.Tests` | 26 offline tests: cron/schedule and catch-up, queue, job logs and traffic, run statistics, API contract on an empty store, JSON shape, remote client error mapping, bundle pull |
| `src/Boors.Node` | Minimal-API host of `IMarketApi` (+ GetJobs, ExportBundle, Pull), problem details, response compression, health checks, optional operator token |
| `src/Boors.Desktop` | WinUI 3 app (MVVM, DI, x:Bind), unpackaged + self-contained Windows App SDK, x64 |
| `rules/rules.yaml` | Dated market rules (embedded into Boors.Market) |

```
 TSETMC · Codal · tgju · fipiran
        │  (resilient HTTP, rate limited)
        ▼
 raw cache (write-once .json.gz + .meta.json) ──bundle zip──► other nodes / desktop replicas
        │  parsers + classifier (Boors.Market)
        ▼
 market.duckdb (MarketStore, single writer) ◄── MarketJobQueue ◄── scheduler (Iran time) / RunJob / Sync
        │  MarketAnalytics, MarketWatchService, DataHealthService
        ▼
 IMarketApi ──► LocalMarketApi (thread pool) ──► Boors.Node (minimal API, JSON) ──► RemoteMarketApi
        │                                                                              │
        └──────────────────────────► Boors.Desktop (WinUI 3) ◄─────────────────────────┘
```

## Build, run, test

Requirements: .NET SDK 10.0.100+ (tested with 10.0.402) on Windows 10 1809+ x64. No Visual Studio and no Windows
SDK install are needed — the Windows App SDK build tools come from NuGet (`dotnet build` works on a machine with only
the .NET SDK).

```powershell
# Everything (7 projects), Release
dotnet build winui/Boors.WinUI.slnx -c Release

# Tests (offline, Microsoft.Testing.Platform)
cd winui/src/Boors.Market.Tests; dotnet test -c Release            # 117 tests
cd winui/src/Boors.Market.Services.Tests; dotnet test -c Release   # 26 tests

# CLI against a data folder (copy, never move, a .boors-data folder into winui/.boors-data — it is git-ignored)
dotnet run --project winui/src/Boors.Market.Cli -c Release -- --data winui/.boors-data health
dotnet run --project winui/src/Boors.Market.Cli -c Release -- --data winui/.boors-data market overview

# Node (http://localhost:5080); BOORS_DATA / BOORS_NODE / BOORS_OPERATOR_TOKEN override appsettings.json
$env:BOORS_DATA = "winui/.boors-data"; dotnet run --project winui/src/Boors.Node -c Release

# Desktop (the exe works directly too: src/Boors.Desktop/bin/x64/Release/net10.0-windows10.0.19041.0/win-x64/Boors.Desktop.exe)
$env:BOORS_DATA = "$PWD/winui/.boors-data"; dotnet run --project winui/src/Boors.Desktop -c Release
```

DuckDB allows one process per store file: close the desktop before pointing the node or the CLI at the same folder.

### Desktop

- **Data:** local mode opens `%LOCALAPPDATA%\Boors\data\market.duckdb` (raw cache next to it); `BOORS_DATA`,
  `--data <dir>` or Settings → *Local store* override it (a restart applies a new folder). An empty store shows an
  empty state: the download in progress, or *Sync from node* when background download is off. Remote mode (Settings → *Data source*) reads a Boors.Node over HTTP instead.
- **Operations:** *Data health* shows dataset freshness, the last runs and source reachability, and *Sync from node*
  pulls another node's raw-cache bundle, imports it and replays it into the store.
- **Data collection** (the job monitor; `--page jobs`): the running job live (progress line, elapsed time, HTTP
  requests, failed requests, retries, warnings, cancel), the queue (cancel one or every waiting job), and a period
  filter (24 hours, 7, 30, 90 days) over KPI tiles (runs, success rate, failing jobs, items, requests, next scheduled
  job), runs per day (or hour) stacked by outcome, the run time of one job, a table of every job (last run, next run,
  schedule in words, runs, success rate, failures in a row, average and longest time, the last 20 outcomes, last
  problem; *Run now*), the run history (job, outcome and text filters, CSV export) and the HTTP traffic per host
  (requests, failures, error rate, average, p95 and slowest latency, last status and error). Opening a run or a queue
  entry shows its fields and log: a queue entry keeps the lines its collectors, source clients and retry policy wrote
  while it ran (the newest 1,000, following a running job live); an older run of this app is read back from the log
  files; runs made by a node or the CLI have no log here. The menu item shows how many jobs failed since the page was
  last open. A run left "running" by a process that died is shown as interrupted. Outcomes always carry an icon and a
  word (✓ OK, ↷ skipped, ⚠ partial, ✕ failed, ⊘ cancelled, ⚡ interrupted). Remote mode shows the node's jobs (polled
  every 3 s while the page is open).
- **Background download** (on by default, Local mode; Settings → *Download market data automatically in the
  background*, applied without a restart): the app runs the Iran-time schedule below while it is open and, when it
  starts or the machine wakes, queues once every job whose scheduled time passed since it last ran — index history,
  missed sessions, end of day, instruments, backfill, then the rest. A new store shows the market after the first jobs
  and keeps backfilling 400 instruments per run until the history is complete. The title bar shows the running job;
  the open page reloads when a job brings new prices or series. The setting is `AutoDownload` in `settings.json`; the
  old opt-in `Collect` key is ignored, so existing installs switch on.
- **Keyboard:** Ctrl+K command palette (symbols and commands: navigation, theme, language, digits, colours, density,
  run jobs, sync), Ctrl+F symbol search in the title bar, F5 reload, Enter in the filter box applies it.
- **Command line** (session overrides, not saved): `--lang fa|en`, `--theme dark|light|system`,
  `--digits persian|latin`, `--preset iranian|cb`, `--density pro|lite`, `--mode local|remote`, `--node-url <url>`,
  `--page overview|watch|screens|health|jobs|settings|symbol:<insCode>`, `--data <dir>`, `--size WxH`, `--scroll Y`,
  `--exit-after-ms N` (closes N ms after the first page shows data; for startup measurements).
- **Files:** settings `%LOCALAPPDATA%\Boors\settings.json`; structured logs (CLEF JSON lines)
  `%LOCALAPPDATA%\Boors\logs\desktop-yyyyMMdd.jsonl`; startup timings `...\logs\startup.txt`.

### Publishing the desktop

```powershell
dotnet publish winui/src/Boors.Desktop -c Release -o publish/desktop                        # framework-dependent, ReadyToRun
dotnet publish winui/src/Boors.Desktop -c Release -o publish/desktop-sc -p:SelfContained=true   # also bundles .NET
```

- **ReadyToRun** is on for Release publish (`PublishReadyToRun`): it precompiles the app and its NuGet libraries
  (Microsoft.Extensions.*, DuckDB.NET, ScottPlot, CommunityToolkit) to cut JIT work at startup.
- **Trimming / Native AOT are off**: DuckDB.NET, Jint and the configuration binder use reflection that is not
  annotated for trimming; a trimmed build needs a curated root list (and a full UI pass) before it is safe.
- **Size:** the Windows App SDK runtime is self-contained (`WindowsAppSDKSelfContained`), so no runtime installer is
  needed, but the 2.x metapackage — required by ScottPlot.WinUI — also brings Windows ML/AI (ONNX Runtime, DirectML,
  ≈55 MB) that the app never loads. Excluding those components with `ExcludeAssets` was tried and breaks the build
  (the ML component's transitive `Microsoft.Windows.AI.MachineLearning` targets require a higher minimum OS), so the
  build output is ≈218 MB. Unused DLLs cost disk, not startup (they are never loaded).

### Node

| Route | Notes |
|---|---|
| `GET api/v1/Market/GetOverview?date=` | 204 when the store is empty (as the boilerplate's MVC action) |
| `GET api/v1/Market/GetBreadth?sessions=` · `GetSectors?date=` · `GetMovers/{kind}?take=&date=` · `GetScreen/{kind}?take=` | same clamps and defaults as the boilerplate |
| `GET api/v1/Market/GetIndexHistory/{insCode}?from=` · `GetMacroHistory/{series}?from=` · `Search?text=&take=` | |
| `POST api/v1/Market/GetMarketWatch` | `{date, filter, classes, tradedOnly}`; a filter syntax error is a 400 problem |
| `GET api/v1/Instrument/{GetMetrics,GetBars,GetFlows,GetEvents,GetShareholders,GetLetters,GetMessages}/{insCode}` | 404 problem for an unknown instrument |
| `GET api/v1/MarketData/GetHealth` · `GetJobs` | health report; job queue + schedule (with next run times) + HTTP traffic per host since start |
| `GET api/v1/MarketData/GetJob/{id}` | a queue entry with its log and traffic; 404 when the queue no longer holds it |
| `GET api/v1/MarketData/GetRuns?job=&since=&take=` | run history (`collection_run`), newest first; default the last 30 days, 5,000 rows |
| `POST api/v1/MarketData/RunJob/{job}` | **operator token**; returns the job id as text (like the boilerplate's `string` action) |
| `POST api/v1/MarketData/CancelJob/{id}` | **operator token**; `true`, or `false` when the job already finished |
| `GET api/v1/Sync/ExportBundle?since=` | **operator token**; zip of raw-cache entries fetched on/after `since` |
| `POST api/v1/Sync/Pull` | **operator token**; `{nodeUrl, since, operatorToken}` → queued: download bundle, import (union), replay into the store |
| `GET /health` · `/alive` | health checks (market data: degraded when stale on a collecting node) |

JSON is camelCase with dictionary keys unchanged (`returns: {"1w": …}`), dates `yyyy-MM-dd` — the same shapes as the
boilerplate DTOs (the DTO records are copied verbatim). Errors are RFC 9457 problem details. Responses are
Brotli/Gzip compressed. `BOORS_OPERATOR_TOKEN` (or `Node:OperatorToken`) makes RunJob and Sync require
`Authorization: Bearer <token>`; empty = open. `MarketData:Collect=true` schedules the jobs (Asia/Tehran):
instruments 07:30 daily; end of day 13:35 and 18:35 (ETFs) Sat–Wed; supervisor messages every 30 min 08–18 Sat–Wed;
Codal every 15 min 07–23; dollar/gold 09/13/17/21; funds 20:00; indices 01:15; backfill 02:00; halts 03:00 — the
boilerplate's Hangfire schedule — plus missed sessions (`repair`) 01:30. All writes go through one queue (DuckDB has
a single writer); a job that is already waiting is not queued twice.

Catch-up: an entry is due when its latest scheduled time is later than the start of its job's last run in
`collection_run` (a run cancelled or left "running" by a crash does not count; a job the user cancelled waits for its
next time). The scheduler checks at least once a minute, so jobs missed while the process was down or the machine
slept run once when it is back, in catch-up order. Backfill is queued again while its last run hit
`MaxBackfillPerRun`. `repair` fills trading days of the last 30 days on which no instrument has a bar (the end-of-day
job never ran) with one `ClosingPrice/GetInstrmentsHistoryInDay/{yyyyMMdd}` call per day (~2,200 traded
instruments, ~170 KB), oldest first, stopping at the first day it cannot fetch; the bars are tagged `marketwatch`
like end-of-day rows, corporate actions are detected as in end of day, and same-day flows stay missing on those days
(no per-day bulk source). It also rewrites the latest stored session from the same list (one call, cached
afterwards), because a 13:35 end-of-day snapshot holds intraday ETF prices when the 18:35 run was missed.

## Data compatibility

- The **raw cache is the shared format** of every Boors implementation:
  `raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{time}-{paramsHash}-{node}.json.gz` + `.meta.json`, `latest/` pointers.
  A bundle from Boors.Node, `boors cache export` (here or in the boilerplate) or any other node imports here and
  vice versa; import is a union (existing files are never overwritten).
- **market.duckdb** is a projection of the cache (schema version from `MarketStoreSchema`), interchangeable with the
  boilerplate's store; *Reload from cache* (`reload` job) replays the cache into the open store — the same steps as
  `boors rebuild` without deleting the file. As documented upstream, a rebuild uses the latest response of each
  request, so intraday snapshots merged earlier can differ slightly (e.g. TEDPIX 8,265,155.56 rebuilt vs
  8,265,156.65 merged live).
- The CLI is the boilerplate CLI: `boors --data winui/.boors-data health` and `market overview` print **byte-identical**
  output to the boilerplate's build on a copy of the same data.

## Library choices

**Charts — ScottPlot.WinUI 5.1** over LiveChartsCore (LiveCharts2) 2.0:
ScottPlot renders immediate-mode on SkiaSharp, so thousands of candles, bars and lines redraw in a few milliseconds
with no per-point objects; it has candlesticks with a `Sequential` (gap-free session) mode, custom tick generators
(used for Jalali/Gregorian session ticks), axis linking across controls (the three symbol panes share X), continuous
per-frame autoscale (Y follows the visible window), and full control of colours/fonts per theme. LiveCharts2 is
retained-mode with animation and MVVM-friendly series, which is pleasant for small dashboards but heavier per point and
per frame for large financial series, and its WinUI package was the less mature of the two at the time of writing.
Text on a Skia canvas is not shaped for Persian, so charts only draw digits; titles, legends and readouts are XAML.

**Grid — custom `DataTable` (ItemsRepeater)** over WinUI.TableView 1.5 and CommunityToolkit DataGrid:
the market watch needs ~3,100 rows × 20 columns, a frozen symbol column, fast sorting and RTL. The custom grid
realizes only visible rows, recycles them, fills cells by direct assignment (no per-cell bindings or templates),
sorts on a pool thread, and keeps the header and the frozen column in place with composition expression animations
(no lag while scrolling). WinUI.TableView is ListView-based with reflection `{Binding}` per cell (not trim/AOT safe
by its own docs) and a generic editing model we don't need; the CommunityToolkit DataGrid is an unmaintained UWP
port. Measured: sorting 3,023 rows takes 2–9 ms for numeric columns and 7–34 ms for Persian-collated text.

**Other:** CommunityToolkit.Mvvm 8.4 (partial-property source generators), Microsoft.Extensions.Hosting (DI,
logging, hosted job worker/scheduler), Vazirmatn v33.003 variable font (OFL, bundled in `Assets/Fonts`, tabular
figures in grids), the WinUI `TitleBar`, `SelectorBar` and `NavigationView` controls. Windows App SDK **2.5.1** is the
latest stable (2.x supports Windows 10 1809+); Mica is used on Windows 11 and a solid theme background on Windows 10
(Mica is not available there).

## Design notes

- **Colours** (validated with the dataviz palette validator against #202020 and #fbfbfb): Iranian green/red by default
  (passes lightness, chroma, normal-vision and contrast; fails colour-vision-deficiency separation by nature, so every
  change also carries ▲/▼ and a sign), and a colour-blind-safe blue/orange preset that passes every check. Status
  colours are reserved for data health and always come with an icon and a label. Series that are not up/down
  (index line, value bars) use violet so they never read as "up" in either preset.
- **Never a dual axis:** TEDPIX in dollars is indexed to 100; price, value and flow are separate panes.
- **RTL:** Persian flips the shell (menu on the right, symbol column on the right of the grid); charts, sparklines,
  breadth bars and numeric cells stay left to right; numeric runs are wrapped in LRE…PDF (WinUI's text stack does not
  honour the newer U+2066 isolates). Jalali dates (PersianCalendar) under Persian, ISO under English; Persian digits
  with ٬ and ٫ separators are a display option only — exports are ASCII.
- **Lite / Pro:** Lite hides the breadth and sector panels, uses larger rows and a shorter market-watch column set.

## Verification (2026-10-19, on the 2026-10-17 capture copied to `winui/.boors-data`)

- `dotnet build winui/Boors.WinUI.slnx -c Release`: 7 projects, 0 warnings, 0 errors. Tests: 117 + 26 passed, offline
  (counts as of the job monitor, 2026-10-27).
- CLI `health` and `market overview`: identical output to the boilerplate CLI (TEDPIX 8,265,157 −1.91%, 146 up / 604 down,
  71 buy queues 2.19 hemat, individual net −6.82 hemat).
- Node: every route exercised with curl (200/204/400/401/404 as designed; 3,023 market-watch rows; Brotli 1.7 MB → 0.6 MB);
  node-to-node sync: 127 MB bundle, 4,240 files imported, replay 130 s; desktop replica pull: 4,244 files, 230 s.
- Desktop: every page reviewed in dark+fa, light+fa and dark+en (plus Lite + colour-blind, remote mode, runtime theme
  and language switches) from screenshots; sorting 3,023 rows 2–34 ms.
- Startup to a populated overview (warm, existing 180 MB store, six runs each, quiet machine): **ReadyToRun publish
  median 1.14 s** (first frame 0.89 s, best 1.05 s); plain `dotnet build` output (JIT) median 1.82 s (first frame
  1.23 s). While other workloads kept the CPU at 50–75 % the same builds took 3.5–7 s (even the runtime start, before
  any app code, then took 0.5–1.4 s instead of 0.13 s). Startup overlaps window creation with the host build and the
  first queries (`App.OnLaunched`, `OverviewViewModel.Prefetch`); the chart control is created after the first frame.
  Measure with `Boors.Desktop.exe --exit-after-ms 200` and read `%LOCALAPPDATA%\Boors\logs\startup.txt`.

## Known gaps and deviations

- The live intraday pipeline (§5.4: SignalR fan-out, order book, trades tape), treemap, sector small multiples,
  watchlists/workspaces and alerts are not built (they are not built in the boilerplate either).
- Limit bands and event markers on the candlestick chart are not drawn (the bars API carries no limits).
- Remote mode reads everything from the node; running jobs remotely needs the node's operator token (Settings).
- Shareholder snapshots are refreshed from TSETMC on demand (at most daily, 15 s cap) unless *Offline* is on — the only
  read that may touch the network, as in the boilerplate.
- The MSIX package and auto-update are not set up (unpackaged by design).
