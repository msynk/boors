# Boors market data

The data layer for Tehran Stock Exchange (TSE) and Iran Fara Bourse (IFB) data. It has no dependency on the Blazor
stack, so the server, background jobs, the CLI and research tools can all use it.
See [docs/development-plan.md](../../docs/development-plan.md) §3–§6 for the design.

| Project | What it is |
|---|---|
| `Boors.Market` | Domain records, write-once raw response cache, TSETMC and Codal clients and parsers, dated market rules (`rules/rules.yaml`), day-status classifier, market calendar |
| `Boors.Market.Cli` | `boors` developer CLI |
| `Boors.Market.Tests` | MSTest unit and golden-file tests built on real responses captured on 2026-10-17 |

## How data flows

1. A client (`TsetmcClient`, `CodalClient`) sends the request through an `HttpClient` that has a browser User-Agent
   (cdn.tsetmc.com answers 403 without one) and a resilience pipeline. The pipeline order is: total timeout, retry
   (exponential backoff with jitter, which also retries 403), circuit breaker, token-bucket rate limit (2 req/s for
   TSETMC, 1 for Codal), then a per-attempt timeout.
2. The response body is written to the raw cache before any parsing:
   `.boors-data/raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{time}-{paramsHash}-{node}.json.gz` plus a `.meta.json` sidecar.
   Files are never overwritten. `latest/` holds one pointer per request, which points at the newest successful response.
3. Parsers (`TsetmcParser`, `CodalParser`) are pure functions from bytes to domain records. They normalize Persian
   text (ی/ک, digits, padding) and never touch the network, so the whole cache can be re-parsed after a fix.
4. `DayStatusClassifier` labels each daily row with one of these statuses:
   - `Traded`
   - `NoTrade`
   - `SymbolHalted`
   - `MarketClosed` (the phantom rows TSETMC emits during closures)
   - `ReopenAuction`
   - `QueueLockedUp` / `QueueLockedDown`

   It uses `MarketRules` (price limit by instrument class and date) and `MarketCalendar` (seeded closures plus days
   when TEDPIX did not move at all).

## CLI

Run from the repository root:

```bash
dotnet run --project src/Market/Boors.Market.Cli -- probe                      # can this node reach every source?
dotnet run --project src/Market/Boors.Market.Cli -- fetch history فولاد        # full history + flows + events, classified
dotnet run --project src/Market/Boors.Market.Cli -- fetch instruments --details --cache   # instrument master (~30 min the first time)
dotnet run --project src/Market/Boors.Market.Cli -- fetch marketwatch          # snapshot, queues, value by class
dotnet run --project src/Market/Boors.Market.Cli -- fetch index                # TEDPIX and detected closures
dotnet run --project src/Market/Boors.Market.Cli -- fetch codal-categories     # Codal LetterType ids
dotnet run --project src/Market/Boors.Market.Cli -- fetch codal-letters فولاد --type 58
dotnet run --project src/Market/Boors.Market.Cli -- verify mw-keys --count 10  # check market-watch compact keys
```

Global options:

| Option | Effect |
|---|---|
| `--data <dir>` | Where the raw cache and outputs go (default `.boors-data`, which git ignores) |
| `--node <name>` | Node name recorded on every raw entry; the env var `BOORS_NODE` does the same |
| `--cache` | Use cached responses when present. Use it to resume a long fetch. |
| `--offline` | Never touch the network; replay from the cache only |
| `--verbose` | Log every request |

Parsed outputs (CSV with a UTF-8 BOM so Excel shows Persian correctly) go to `.boors-data/parsed/`. Reports from
`probe` and `verify` go to `.boors-data/reports/`.

## Tests

```bash
cd src/Market/Boors.Market.Tests && dotnet test
```

The tests use no network. To refresh a golden file, see `Boors.Market.Tests/GoldenFiles/README.md`.
