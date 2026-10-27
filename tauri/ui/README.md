# Boors UI (SolidJS)

The workstation UI of the Tauri implementation: market overview, market watch, symbol workspace, screens and data
health for the Tehran Stock Exchange and Iran Fara Bourse. One codebase serves the desktop app (Tauri IPC, offline)
and any browser talking to a `boors-server` node (HTTP). It speaks only the contract in
[`../docs/api-contract.md`](../docs/api-contract.md).

## Scripts

| Script | What it does |
|---|---|
| `npm run dev` | Vite on `http://localhost:5173` (the port `apps/desktop/src-tauri/tauri.conf.json` expects). `/api` is proxied to `BOORS_API` (default `http://localhost:5080`, the `boors-server` default). |
| `npm run dev:mock` | The same with `VITE_MOCK=1` (`vite --mode mock`, reads `.env.mock`): every route is served from deterministic fixtures in `src/mocks/`, no backend needed. `VITE_MOCK=1 npm run dev` works too. |
| `npm run build` | Production build into `dist/` (what `boors-server --ui-dir` and the Tauri bundle serve). |
| `npm run preview` | Serves `dist/` (with the same `/api` proxy). |
| `npm run typecheck` | `tsc --noEmit`, strict. |
| `npm run lint` | ESLint flat config: `@eslint/js` + `typescript-eslint` + `eslint-plugin-solid`. |
| `npm test` | Vitest: formatting, transport and client, grid sort/filter/CSV, series maths, and a contract-shape check of every mock route. |

Desktop: from `tauri/`, `npm run tauri:dev` starts `npm --prefix ui run dev` and the app; `npm run tauri:build`
runs `npm --prefix ui run build` and bundles `ui/dist`.

### Mock mode

`src/mocks/` generates a ~3,100-instrument market (shares, funds, rights, options, bonds) whose headline numbers are
the live sample of the development plan (§9.3): TEDPIX 8,265,157 (−1.91%), 146 up / 604 down, 30.8 hemat traded,
71 buy queues (2.19 hemat), 143 sell queues, individual net flow −6.8 hemat, session 1405/07/25. Bars, flows,
capital events, halts, Codal letters and holders are generated per instrument from a seeded PRNG (seven years of
Saturday–Wednesday sessions), so the symbol page agrees with the market watch. فولاد uses its real InsCode
`46348559193224090`; synthetic InsCodes are 17-digit strings above 2^53, like real ones. The filter box runs a small
dev-only evaluator for the TSETMC syntax (syntax errors come back as 400 problems, per-row failures as
`filterErrors`); the job buttons drive a timed job queue.

Page states can be forced with a query parameter, remembered for the tab: `?mock=stale` (stale banners),
`?mock=empty` (empty states), `?mock=error` (problem errors on every route), `?mock=slow` (12× latency, to see the
skeletons), `?mock=normal` to reset.

The mock code is behind `if (import.meta.env.VITE_MOCK === '1') await import('./mocks')`, which Vite replaces with a
constant at build time: a production build contains no fixture code (checked: no mock chunk, no generator symbols in
`dist/`).

## Structure

```
src/
  main.tsx              entry: font + styles, mock transport in mock mode, render
  App.tsx               routes (keyed on language + digits, so every formatted string is rebuilt on a switch)
  api/                  contract DTOs (types.ts), transports (transport.ts), client with de-dup/cache/abort (client.ts),
                        typed endpoints (index.ts: market.*, instrument.*, marketData.*, sync.*, app.*)
  format/format.ts      port of MarketFormat.cs: numbers, hemat/toman units, signed changes, Jalali dates, digits
  i18n/                 en.ts (the key set and its type) and fa.ts (typed against it), t(), class/status labels
  state/                settings (localStorage), createQuery (abortable, keyed loads), data health, recent symbols, titles
  components/           Shell (top bar, side/bottom nav), SymbolSearch, CommandPalette, KpiTile, Sparkline, Table, ui
  grid/                 VirtualGrid (market watch) and its pure logic (sort, rank, filter, CSV, windowing)
  charts/               Lightweight Charts (IndexChart, SymbolChart, Jalali time scale in lw.ts) and ECharts
                        (EChart wrapper; OverviewCharts: breadth, sector bars, treemap); colour maths; theme bridge
  pages/                Overview, Watch, Symbol, Screens, Health, NotFound (+ watch/ columns and state, overview/ series)
  mocks/                fixtures and the mock transport (dev only)
  styles/               tokens.css (design tokens), base.css, components.css
public/boot.js          applies saved theme/language/density before first paint (a file, not inline: strict CSP)
```

### Transport

`client` picks the transport once: Tauri IPC (`invoke('api', { method, path, query, body })`) when
`'__TAURI_INTERNALS__' in window`, otherwise `fetch('/api/v1/' + path)`. Both reject with `ProblemError`
(`title`, `detail`, `status`, `kind: problem | network | aborted`). On top:

- identical concurrent requests share one call (keyed by method, path, sorted query and body);
- GET answers are cached in memory (default 30 s; longer for history, 0 for jobs/health polling; LRU at 200 entries);
- every call takes an `AbortSignal`; a waiter that aborts leaves the shared call, which is cancelled when its last
  waiter leaves. `createQuery` aborts its request when its key changes and when the page unmounts (route change);
- `RunJob` and `Sync/Pull` send `Authorization: Bearer <token>` over HTTP when an operator token is saved on the
  data-health page (session storage, or local storage with "Remember"). IPC needs none.

64-bit ids (`insCode`, `parentInsCode`, `tracingNo`, message `id`) stay strings end to end.

## Design

- **Tokens** (`styles/tokens.css`): surfaces, text (≥ 4.5:1 on `--surface` and `--surface-2` in both themes), grid
  and axis, accent, status (always icon + label), and the up/down pairs. Dark is the default theme; light is its
  own selected set. Charts read the same tokens (`charts/theme.ts`) and repaint on theme or preset change.
- **Up/down presets**, checked with the dataviz palette validator against the chart surfaces:
  - Iranian (default) green up / red down: light `#008300`/`#e34948`, dark `#0ca30c`/`#e66767`. Their CVD
    separation is in the 6–8 "floor" band (ΔE 7.2 light, 7.1 dark), legal only with secondary encoding, so every change
    carries ▲/▼ and a sign, and bars sit on a visible zero line.
  - Colour-blind safe blue up / orange down: light `#2a78d6`/`#eb6834`, dark `#3987e5`/`#d95926`; passes every check
    (CVD ΔE 24.7 light, 26.8 dark).
  - Value traded uses a neutral `--volume` hue and corporate events a violet `--event`, so neither reads as up/down.
- **Diverging scales** (treemap): grey midpoint → up/down, mixed in OKLab, fixed ±daily-limit domain per instrument,
  overflow colours beyond the limit (reopening auctions); halted/not-traded tiles are grey with a hatch decal.
- **RTL/LTR**: `fa` is RTL, `en` LTR, laid out with logical properties. Numbers are `<bdi dir="ltr">` with tabular
  figures; signed changes are isolated runs (`▲ +2.50%`), never reordered. Chart containers are LTR in every culture
  (as on TSETMC); under fa the time axis is Jalali, with tick weights on Jalali month/year boundaries (a custom
  horizontal-scale behaviour) and Persian month names. Digits follow the Persian/Latin toggle; CSV exports are ASCII.
- **Typography**: Vazirmatn variable font (OFL), bundled from the `vazirmatn` package — no CDN, works offline and
  behind Iran's filtering. Inline separators are rules, not "·" (which reads as the Persian zero ۰).
- **Density**: Pro shows every panel and column; Lite hides the market map, extra market-watch columns and secondary
  table columns, and uses taller rows. Defaults to Lite on narrow screens. Phones get a bottom tab bar.
- **Keyboard**: Ctrl+K command palette (symbols, pages, settings; recent symbols when empty), `/` focuses the symbol
  search, arrow keys/PageUp/PageDown/Home/End/Enter in the market-watch grid, arrow keys in tab lists, Ctrl+Enter
  applies a filter.

## Performance budget

| Item | Budget | Measured |
|---|---|---|
| Initial JS (shell + overview route), gzip | ≤ 150 KB | see the build output in the final report; ~50 KB + 7 KB CSS |
| Charts | lazy, per route | `lw` (Lightweight Charts) and `OverviewCharts` (ECharts) are separate chunks loaded after first paint |
| Market-watch sort, 3,112 rows | < 50 ms | sort ≈ 1 ms (numeric), ≤ 7 ms first text sort; click → sorted DOM laid out ≈ 17 ms median, 27 ms max |
| Market-watch scroll | 60 fps | 0 frames over 20 ms at 1,700–7,200 px/s (production build, headless Chromium) |

How the grid gets there: only ~47 rows exist in the DOM; the window is one translated block whose start snaps to
8-row blocks (most scroll frames are pure compositing); rows are keyed `<For>` items with `contain: layout style`;
sorting re-slices an index array (numeric keys in a `Float64Array`, text columns ranked once per dataset with
`Intl.Collator('fa')`); quick search filters precomputed normalised keys. Nothing re-renders the whole table.

## Dependencies

| Package | Why |
|---|---|
| `solid-js` | Fine-grained reactivity: a sort or a price change touches only the affected text nodes; ~7 KB runtime. |
| `@solidjs/router` | Routing with lazy route components (code splitting per page). |
| `lightweight-charts` 5 | Time series: candlesticks with value and flow panes in one chart (shared time scale and crosshair), custom horizontal-scale behaviour for Jalali ticks, series markers. Daily-limit bands (`BarDto.allowedLow/allowedHigh`) are dashed line series where the store knows them, plus dashed price lines for the latest session. Apache-2.0; its TradingView attribution logo stays visible (`layout.attributionLogo: true`). |
| `echarts` 6 | Treemap, diverging sector bars and 100% breadth bars, tree-shaken through `echarts/core` (bar + treemap + grid/tooltip/markLine, canvas). |
| `@tauri-apps/api` | `invoke` for the IPC transport; imported dynamically, so the web build does not load it. |
| `vazirmatn` | The Persian UI font (variable woff2, bundled). |
| dev: `vite`, `vite-plugin-solid`, `typescript` 6.0, `vitest`, `eslint` + `@eslint/js` + `typescript-eslint` + `eslint-plugin-solid`, `@types/node` | Build, types, tests, lint. TypeScript stays on 6.0 because `typescript-eslint` does not support 7.x yet. |

Deliberately not used: a UI kit (plain CSS with tokens is smaller and fully RTL-aware), `@tanstack/solid-virtual`
(the hand-rolled block-snapping virtualizer above is smaller and measured faster for fixed-height rows), date
libraries (Intl's Persian calendar covers Jalali).

## Notes for the server side

- The UI uses history routing (`/market/watch`, `/symbol/{insCode}`): a server must answer unknown non-`/api` paths
  with `index.html` (boors-server does).
- `classes` in `MarketWatchRequestDto` is sent as the contract's enum names (`TseStock`, …); `instrumentClass` is read
  in either spelling (`TseStock` or `tse_stock`).
- Fractions vs percents follow the contract and the .NET pages: `weightedReturn`, `medianReturn`, `returns[*]`,
  `usdReturn1Year`, `buyerPowerPercentile` and `BarDto.return` are fractions; `*Percent` fields are percents.
