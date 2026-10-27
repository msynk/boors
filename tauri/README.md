# Boors — Rust + Tauri 2 + SolidJS

An analysis workstation for the Iranian stock market (TSE/IFB). This folder is a self-contained rewrite of the .NET
implementation in `boilerplate/`: the market-data layer is a faithful Rust port of `Boors.Market`, the HTTP node and
the desktop app share one API dispatcher, and the SolidJS UI (`ui/`) talks to both through one contract
([`docs/api-contract.md`](docs/api-contract.md)).

## Architecture

```
                     ┌──────────────────────── crates/boors-market ─────────────────────────┐
 TSETMC  Codal       │ sources/  ── HTTP: browser UA/Referer/Origin, total timeout, retry     │
 tgju  fipiran ──────┼─►          (backoff+jitter, 403/429), circuit breaker, token bucket,  │
                     │            per-attempt timeout                                        │
                     │     │ every response is stored first                                  │
                     │     ▼                                                                 │
                     │ raw_cache/  write-once gzip + .meta.json + latest/  ◄─► zip bundles   │──► other nodes
                     │     │ (format-compatible with the .NET FileSystemRawCache)             │    (.NET, WinUI, Rust)
                     │     ▼ parsers (pure)                                                  │
                     │ domain/ (day status, calendar) + rules/ (rules.yaml)                  │
                     │     ▼                                                                 │
                     │ storage/  DuckDB market.duckdb (schema v3, idempotent merges)         │
                     │     ▼                                                                 │
                     │ ingestion/ (jobs, health)  analytics/  filters/ (QuickJS sandbox)     │
                     │     ▼                                                                 │
                     │ api::dispatch(ctx, method, path, query, body) + job queue + scheduler │
                     └───────────────┬───────────────────────────────┬───────────────────────┘
                                     │                               │
              crates/boors-server (axum)                  apps/desktop/src-tauri (Tauri 2)
              GET/POST /api/v1/*  /health                 invoke('api', {method, path, query, body})
              serves ui/dist, --collect scheduler         data in %LOCALAPPDATA%\dev.boors.desktop\data
                                     │                               │
                                     └────────── ui/ (SolidJS) ──────┘
              crates/boors-cli: `boors` — the same jobs, analytics and checks in a terminal
```

## Crates

| Crate | Binary | What it is |
|---|---|---|
| `crates/boors-market` | — | The core library (port of `Boors.Market`). Modules: `domain`, `text`, `rules`, `raw_cache`, `sources` (`tsetmc`, `codal`, `tgju`, `fipiran`, `http`), `storage`, `ingestion`, `contracts`, `analytics`, `filters`, `probe`, `api` |
| `crates/boors-cli` | `boors` | The developer/operations CLI with the commands and global options of the .NET CLI |
| `crates/boors-server` | `boors-server` | HTTP node: `/api/v1/*`, `/health`, the built UI with SPA fallback, gzip/br, CORS for dev, operator token, Iran-time scheduler |
| `apps/desktop/src-tauri` | `boors-desktop` | Tauri 2 app "Boors" (`dev.boors.desktop`), one IPC command `api` |

## Build, run, test

Rust stable (edition 2024) with the MSVC toolchain on Windows. DuckDB is compiled from source the first time
(`duckdb` crate, `bundled`), which takes several minutes.

```bash
cd tauri
cargo build --workspace --release
cargo test --workspace                                  # offline: golden files + in-memory stores
cargo clippy --workspace --all-targets -- -D warnings

# CLI (global options: --data --node --cache --offline --verbose --rules)
target/release/boors --data .boors-data rebuild         # store from the raw cache only
target/release/boors --data .boors-data market overview
target/release/boors --data .boors-data market watch --filter "(pc)>(py) && (ct).Buy_I_Volume>(ct).Sell_I_Volume"
target/release/boors --data .boors-data health

# HTTP node (env: BOORS_DATA, BOORS_NODE, BOORS_PORT=5080, BOORS_HOST, BOORS_COLLECT, BOORS_OPERATOR_TOKEN, BOORS_UI_DIR)
target/release/boors-server --data .boors-data --port 5080 [--collect] [--cors] [--ui-dir ui/dist]

# Desktop (from tauri/; the scripts run the UI's npm scripts)
npm install
npm run tauri:dev        # Vite dev server on :5173 + the app
npm run tauri:build      # builds ui/dist, then the NSIS/MSI installers
```

The CLI commands: `probe`, `fetch instruments|history|marketwatch|index|static|codal-categories|codal-letters`,
`verify mw-keys|adjustments|contracts|reconcile`, `collect instruments|indices|calendar|backfill|eod|funds|macro|codal|messages|halts|all`,
`market overview|breadth|sectors|movers|screen|metrics|search|watch`, `rebuild`, `health`, `query`,
`cache export|import`, `import macro`.

## Data compatibility

- **Raw cache:** same layout (`raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{HHmmssfff}-{sha256(paramsKey)[..16]}-{node}.json.gz`),
  same params keys, same `.json.meta.json` sidecar (PascalCase, System.Text.Json indentation, escaping and
  `DateTimeOffset` format, CRLF on Windows), same `latest/{source}/{endpoint}/{hash}.json` pointers that only move
  forward, same write-once/suffix rules. A cache written by the .NET collector is read as is (the test data in
  `.boors-data/` is a copy of one), and vice versa.
- **Bundles:** `boors cache export/import` and `GET Sync/ExportBundle` produce the same zip layout as
  `RawCacheBundle.cs` (`raw/...` entries, bodies stored, sidecars deflated); import is a union.
- **Store:** `market.duckdb` uses the DDL of `MarketStoreSchema.cs` (schema version 3) on DuckDB 1.5.6, the version
  DuckDB.NET 1.5.6.1 bundles, so the files are interchangeable too. The store is a projection: delete it and run
  `boors rebuild`.
- **Rules / holidays:** `rules/rules.yaml` is embedded (`--rules` overrides it). The official-holiday table is the
  DNTPersianUtils.Core 7.0.2 table (Jalali 1395–1405) exported verbatim to `crates/boors-market/data/holidays.txt`;
  Persian/Arabic character folding uses tables extracted from the same library.
