# AGENTS.md — `tauri/` (Rust + Tauri 2 + SolidJS implementation of Boors)

This folder is a self-contained implementation of Boors. It does not depend on anything outside `tauri/`:
no path dependencies, imports, symlinks or runtime file reads that leave the folder. Files that originate elsewhere
(`rules/rules.yaml`, the golden files, the contract baseline, the DNTPersianUtils holiday table) are **copies**.

## 1. Layout

| Path | What it is |
|---|---|
| `crates/boors-market` | The core library, a port of the .NET `Boors.Market`: domain, Persian text, rules, raw cache + bundles, HTTP sources, parsers, DuckDB store, collectors, health, contracts, analytics, TSETMC filters, the API dispatcher and job queue |
| `crates/boors-cli` | The `boors` binary (same commands and options as the .NET CLI) |
| `crates/boors-server` | `boors-server`: axum HTTP node serving `/api/v1/*`, `/health`, the built UI, and the collection scheduler |
| `apps/desktop/src-tauri` | The Tauri 2 desktop app (`dev.boors.desktop`); one IPC command, `api`, calls the same dispatcher |
| `ui/` | The SolidJS UI (owned by the UI work; the Rust side only reads `ui/dist`) |
| `docs/api-contract.md` | **The API contract. It is authoritative.** Routes, JSON names, null handling, enum strings, string ids |
| `rules/rules.yaml` | Dated market rules, embedded into `boors-market` with `include_str!` |

## 2. Conventions

- **Rust edition 2024**, stable toolchain, `cargo clippy --workspace --all-targets -- -D warnings` must stay clean and
  `cargo fmt` formatted (`rustfmt.toml` sets the line width).
- **Errors:** `thiserror` in the library (`boors_market::Error`), `anyhow` in binaries. **No `unwrap`/`expect` in
  library code** outside tests and provably infallible constants (compiled regexes).
- **Logging:** `tracing` with structured fields (`tracing::info!(job, items, "…")`), never `println!` in the library.
- **Money and prices** are `rust_decimal::Decimal` (C# `decimal` semantics). Only ratios and statistics are `f64`.
- **Dates** are `chrono::NaiveDate` (`DateOnly`), instants `DateTime<Utc>`; Iran time comes from `text::iran_now()`
  (Asia/Tehran rules, DST before 2022).
- **Raw cache compatibility is a hard requirement.** `raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{HHmmssfff}-{hash}-{node}.json.gz`,
  the `.json.meta.json` sidecar (PascalCase, System.Text.Json formatting and escaping), `latest/` pointers that never
  move backwards, and zip bundles must stay interchangeable with the .NET and WinUI implementations. Never change the
  params-key strings or the hash.
- **The store is a projection of the raw cache.** Schema changes bump `storage::schema::VERSION` and rebuild; no
  migrations. One writer at a time: collection jobs go through the job queue (`api::jobs`).
- **Parsers are pure** (`&[u8]` in, records out) and tolerant of the shapes in `docs/development-plan.md` §9.4.
- **The contract file wins.** If the C# behaviour and `docs/api-contract.md` disagree, implement the contract's shape
  and record the discrepancy in `README.md` (section "Deviations").
- **Tests run offline.** Golden files live in `crates/boors-market/tests/golden/`; tests never touch the network.
  Live checks belong in CLI commands (`boors verify …`, `boors probe`).
- **Network etiquette:** TSETMC ≤ 2 req/s, Codal one request per 2 s, others 1 req/s (the defaults). Never run a full
  backfill against the live sources for testing; replay the raw cache with `--offline` instead.
- Cap long-running commands with `timeout <seconds> …` and report exit code 124 if hit.

## 3. Commands

```bash
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p boors-cli --release -- --data .boors-data --offline market overview
cargo run -p boors-server --release -- --data .boors-data --port 5080
npm install && npm run tauri:dev      # from tauri/, needs the UI dev server
npm run tauri:build                   # from tauri/, builds ui/dist then the installer
```

## 4. Where to look in the .NET original (read-only reference)

`boilerplate/src/Market/Boors.Market/**` (library), `Boors.Market.Cli/**` (CLI), `Boors.Market.Tests/**` (tests),
`boilerplate/src/Server/Boors.Server.Api/Features/Market*/**` (API). Port behaviour, not just shapes, and keep the
module names aligned (`Ingestion/MarketCollector*.cs` ↔ `ingestion/collector.rs`, etc.).
