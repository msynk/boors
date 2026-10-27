# Boors

Boors is an analysis workstation for the Iranian stock market (TSE/IFB). It collects market data, disclosures and
macro data, versions them in a raw cache, and shows them through dashboards, a market watch with TSETMC-compatible
filters, symbol workspaces, screeners and a data-health page.

The repository holds three independent implementations of the same product. They share no code: each folder builds,
runs and tests on its own. What they do share is the **raw-cache format**, so a cache or sync bundle collected by one
implementation can be imported and rebuilt by the others.

| Folder | Stack | What it contains |
|---|---|---|
| [`boilerplate/`](boilerplate/) | .NET 10, Blazor WebAssembly, bit BlazorUI / bit Boilerplate, ASP.NET Core, PostgreSQL, Hangfire, DuckDB | The original implementation (phases 0–2). Web, MAUI and Windows Forms hybrid clients. |
| [`tauri/`](tauri/) | Rust (tokio, axum, DuckDB, boa), Tauri 2, SolidJS + TypeScript + Vite, Lightweight Charts, ECharts | A Rust core shared by the `boors` CLI, the HTTP node server and the Tauri desktop app. One SolidJS UI is used both by the desktop app (over IPC) and by browsers (over HTTP). |
| [`winui/`](winui/) | C# / .NET 10, WinUI 3 (Windows App SDK), ASP.NET Core minimal API, DuckDB | A native Windows desktop app with the market-data library in-process, plus a lean node server. |

Shared, stack-neutral material:

- [`docs/development-plan.md`](docs/development-plan.md): the product plan, data sources, data model, analytics and roadmap. Its stack sections describe the `boilerplate/` implementation.
- [`reports/`](reports/) and [`research_notes/`](research_notes/): the feasibility research.

See each folder's `README.md` for how to build, run and test that implementation, and its `AGENTS.md` for its coding
conventions.
