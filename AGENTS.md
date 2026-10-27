# AGENTS.md

This repository holds three independent implementations of Boors, an analysis workstation for the Iranian stock
market. Before you change anything, find out which implementation the task is about. Then follow **that folder's own
`AGENTS.md`**, which is the source of truth for its stack, commands and conventions.

| Folder | Stack | Instructions |
|---|---|---|
| `boilerplate/` | .NET 10 + Blazor (bit Boilerplate) | [`boilerplate/AGENTS.md`](boilerplate/AGENTS.md) |
| `tauri/` | Rust + Tauri 2 + SolidJS | [`tauri/AGENTS.md`](tauri/AGENTS.md) |
| `winui/` | C# + WinUI 3 | [`winui/AGENTS.md`](winui/AGENTS.md) |

## Rules that apply everywhere

- **Keep the implementations separate.** Code, build files and runtime paths in one folder must never reference
  another folder. You may copy a file from one implementation into another, but never link to it. A feature
  requested "for the app" does not imply all three. Ask which implementation is meant, unless the user says.
- **The raw-cache format is the shared contract.** All three write and read
  `raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{time}-{paramsHash}-{node}.json.gz`, with `.meta.json` sidecars and
  `latest/` pointers, and the same zip bundle format. A change to that format must land in all three implementations,
  or none.
- **Local data stays out of git.** Each implementation keeps its data in `.boors-data/` inside its own folder.
- **Stack-neutral knowledge lives at the root.** That is the product plan (`docs/development-plan.md`) and the
  research (`reports/`, `research_notes/`).
- **Never wait unbounded.** Cap long-running commands with `timeout <seconds> <command>`, and report exit code `124`
  when the cap is hit.
