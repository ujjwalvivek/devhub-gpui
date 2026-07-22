# DevHub-GPUI Decision Log [Architecture Decision Records]

> Living project brief, implementation plan, decision log, and handoff context.
> Keep this document current throughout the project. If implementation and this
> document disagree, stop and update the document or explicitly record why the
> implementation must differ.

## Config Paths

Windows:

- `config.toml`: `%AppData%\Roaming\devhub-gpui\config\config.toml`
- `projects.toml`: `%AppData%\Local\devhub-gpui\cache\projects.toml`

Linux:

- `config.toml`: `${XDG_CONFIG_HOME:-~/.config}/devhub-gpui/config.toml`
- `projects.toml`: `${XDG_CACHE_HOME:-~/.cache}/devhub-gpui/projects.toml`

`DEVHUB_GPUI_STATE_DIR` overrides both platforms, using
`<override>/config/config.toml` and `<override>/cache/projects.toml`.

## Optional future enhancements and platform limits

These are not unfinished parity work. They are possible post-v1.1.0 product
investments or explicit platform boundaries.

| Capability                             | Complexity | Notes                                                                                                                                                                                               |
| -------------------------------------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Per-source scan status and errors      | Low        | ScanModel already tracks aggregate state; per-source tracking needs per-root error/result fields and UI per-project-row indicators                                                                  |
| Manual refresh per root or host        | Medium     | Requires source-level refresh button and per-source cancellation routing; the cancellation infrastructure now exists                                                                                |
| Global full-text search index (`grep`) | High       | Persistent incremental index file, watched filesystem events, hybrid local/SSH query — a significant standalone feature                                                                             |
| SSH host command environment           | N/A        | POSIX hosts use standard shell tools. Windows hosts require PowerShell plus Git for Windows, whose bundled `sh`, `find`, `grep`, `stat`, `wc`, `head`, and `cat` run the same bounded scripts. SFTP-only and Windows hosts without Git for Windows remain unsupported. |

## Status

- Current stage: DevHub-GPUI v2.1.4 — project hub with Git, terminal, command
  palette, IDE detection, MCP project intelligence, and todo list
  - Git integration: branch picker, unified diffs, commit history, commit box,
  commit graph, staged/unstaged file actions, Fetch, Push
  - Embedded pseudo-terminal per project with PTY, resize, scrollback, pinning,
  lifecycle ownership, and SSH routing
  - Recoverable persistence with atomic backups, writer locking, startup
  recovery, stale-recovery conflict detection
  - Metadata-driven editor launcher: auto-detect IDEs from platform metadata,
  filter JetBrains entries by project evidence
  - Command palette with keyboard-first navigation and live theme preview
  - Read-only MCP server (`devhub-mcp` crate): stdio binary for external agents
    + in-app loopback HTTP server with toggle and required bearer token auth
  - Per-project todo list persisted across restarts, SSH projects included
  - The old OpenCode chat harness and credential storage were deleted — MCP
  clients bring their own models
- Completed stage: DevHub-GPUI v1.1.0 complete within the approved functional-parity scope:
  - Local + SSH discovery, tree, read, README, search
  - Cancellation of all in-flight operations (scan, tree, file, readme, search)
  - Remote `.gitignore` semantics via `git check-ignore` over SSH
  - Pin/unpin and hide/archive projects with persistent config
  - Right-click context menu with Pin/Hide actions
  - Zed-first launch (local `zed <path>` and SSH `ssh://user@host/path`)
  - Custom local/SSH source picker, versioned config/cache, standalone release automation
- Closure gate passed on 2026-07-03: local static validation is green, the
  cross-platform v1.1.0 release and checksums are published, and the updated
  website presents GPUI as the primary DevHub with the egui build retained as
  the legacy reference.
- Implementation started: semantic dark theme and compact application shell
- Workspace observed on 2026-06-30: minimal generated-and-reviewed Cargo workspace
- Renamed-workspace audit on 2026-07-01: stale `[REDACTED]\gpui` paths in this plan
  corrected to `[REDACTED]\devhub-gpui`; static validation gate green (fmt-check,
  check, 8 tests, clippy with warnings denied, release build from cache)
- Milestone 1 on 2026-07-01: added `devhub-core` config and cache modules with
  serde/toml/directories dependencies, distinct `devhub-gpui` identity, versioned
  cache, configured scan roots/max depth, and explicit rescan. 14 tests pass.
- Milestone 2 on 2026-07-01: added editor launching, incremental keyboard
  filtering, and Enter-to-open workflow. 23 tests pass.
- Milestone 2 revision on 2026-07-01: replaced hardcoded editor config with
  Windows native "Open with" dialog (SHOpenWithDialog). 14 tests pass.
- Milestone 3 on 2026-07-01: added local workbench (file tree, bounded file
  reading, content search) with tabbed details panel. 23 tests pass.
- Deep audit on 2026-07-01 found Milestone 3 incomplete in practice: the flat
  global tree sort breaks hierarchy, hidden mode conflates dotfiles with ignored
  and generated files, workbench tasks accept stale results, preview text is not
  selectable or highlighted, the plan overstates implemented UI state, clippy
  fails, and the moved release target contains stale artifacts.
- Remediation Phase 1 static implementation on 2026-07-01: restored hierarchical
  tree ordering, separated dotfile visibility from ignore/generated-directory
  policy, added explicit workbench load states and stale-result rejection, moved
  folder picking off the foreground executor, surfaced persistence errors,
  added crash-safe config/cache replacement, split reusable UI primitives into
  `ui.rs`, replaced the placeholder README, and removed stale artifacts. All 32
  tests, fmt, check, clippy with warnings denied, and release build pass; native
  visual and interaction review remains.
- Remediation Phase 1 native review passed on 2026-07-01.
- Remediation Phase 2A on 2026-07-01 implemented a custom eager document and
  Markdown renderer. Native review rejected it because scrolling and selection
  were laggy and inaccurate, Markdown fidelity was poor, and README content
  could truncate. That implementation is superseded rather than accepted.
- Remediation Phase 2B on 2026-07-01 replaced the failed custom renderer with
  `gpui-component` 0.5.1: a rope-backed read-only code editor with Tree-sitter
  highlighting and a selectable virtualized Markdown view. README now owns the
  main Overview area below a compact metadata-card header. Remote Markdown and
  HTML images are sanitized to omission markers before rendering. Static
  validation passes with 35 tests.
- Remediation Phase 2B native review passed on 2026-07-01: README layout and
  scrolling, Markdown preview, code scrolling and selection, clipboard feedback,
  wrapping, highlighting, search positioning, read-only behavior, and existing
  window interactions were accepted by the user. Remediation Phase 2 is complete.
- Remediation Phase 3 SSH implementation on 2026-07-02: added persisted SSH host
  and root configuration, a unified custom local/SSH folder picker, remote project
  discovery, tree browsing, README/file reading, content search, and VS Code
  Remote SSH launch. OpenSSH operations use BatchMode, validated targets,
  connection/operation deadlines, bounded output, and stale-result rejection.
  The full static gate passes with 42 tests; native review remains. Its initial
  VS Code launch choice was later superseded by the Zed-first parity closure.
- Remediation Phase 3B on 2026-07-02: increased the inherited application font
  weight to medium with semibold section labels; replaced file-tree text glyphs
  with gpui-component chevrons, folder states, and recognizable file-type icons;
  enabled README raster/SVG web images; and resolved relative image paths through
  recognized GitHub/GitLab remotes. Static validation passes with 43 tests;
  native review rejected image and icon rendering because the application still
  used GPUI's null asset source and `NullHttpClient`.
- Remediation Phase 3C on 2026-07-02: installed the published component asset
  bundle, added a bounded reqwest-backed GPUI HTTP client, embedded `appicon.svg`
  in the title strip and Windows executable resources, and replaced GPUI's
  maximize-only Windows call with an `IsZoomed`-aware maximize/restore toggle.
  The full static gate passes with 43 tests.
- Remediation Phase 3 native review passed on 2026-07-02: the user confirmed the
  application/taskbar icon, typography, tree icons, GIF/SVG/raster README images,
  maximize/restore, Snap Layout hover, local/SSH picker and persistence, mixed
  scanning and partial failure, remote README/tree/file/search, hidden mode,
  stale-result rejection, errors/timeouts, and the then-current VS Code Remote
  SSH launch. Zed-first launch now replaces that historical path.
- Post-Phase 3 README image experiment was reverted on 2026-07-02. README images
  now become explicit alt-text placeholders and no application HTTP client is
  installed. Local and SSH project browsing therefore does not contact image
  hosts. Linked-image destinations remain available as ordinary links. The full
  static gate passes with 44 tests; the release build used the documented
  machine-local `GPUI_FXC_PATH` workaround.
- Parity-closure implementation on 2026-07-02 replaced the temporary VS Code
  remote launcher with Zed-first local/SSH launching, using Zed's documented
  `ssh://user@host[:port]/path` CLI target. The details bar now shows host, Git
  remote, markers, and modified time; SSH hosts retain independent scan depths;
  the five legacy palettes and System/Dark/Light modes persist in config. Fmt,
  all-target check, 46 tests, warning-denied clippy, and release build pass;
  native review remains.
- The unsuccessful decorative titlebar icon was removed on 2026-07-02. Settings
  now occupies the leftmost titlebar slot as a Lucide hamburger button outside
  the window drag region; its state and behavior are unchanged. The canonical
  app icon remains embedded in the executable for Windows Task View/taskbar use.
- Production fixtures were removed on 2026-07-02. A first launch is now defined
  as both config and cache being absent; only that condition opens Settings
  automatically. Existing installations with an empty cache retain the normal
  empty-project state. The truth table is covered by a unit test.
- The learning-phase current-directory scan fallback was removed on 2026-07-02.
  Scan with no configured local/SSH sources performs no filesystem access and
  reports `Add at least one local folder or SSH source before scanning.`
- Configuration schema versioning was added on 2026-07-02. New files serialize
  as version 1; existing unversioned files are treated as version 0, normalized,
  and atomically rewritten as version 1. A configuration declaring a newer
  version is rejected without modification, and ordinary saves refuse to
  overwrite it. Fmt, all-target check, 48 tests, warning-denied clippy, and the
  release build pass.
- Replacement-readiness remediation on 2026-07-02 added an isolated
  `DEVHUB_GPUI_STATE_DIR` validation path, an IME-capable project filter,
  Ctrl+F focus, virtualized project rows, 10,000-project cache and 50,000-project
  filtering tests, partial local-root failure handling, and preservation of the
  last known-good cache on total scan failure.
  Native review then caught two virtualization regressions: project rows now
  explicitly fill the sidebar so hover/selection remain edge-to-edge, and scan
  startup clears workbench state while suppressing selection of retained cached
  projects so stale details never flash during scanning.
- The user confirmed the isolated clean-install, IME input, keyboard navigation,
  large-list scrolling, partial failure, and total-failure recovery gate on
  2026-07-02. The temporary validation checklist was then removed; this living
  plan retains the durable result. Filter changes and keyboard selection now use
  the same selection path as mouse clicks, immediately resetting and loading the
  newly selected project's details.
- Standalone release automation was added on 2026-07-02 after reviewing the
  read-only DevHub tag-release workflow and GPUI's platform requirements. Release
  Windows binaries use the GUI subsystem; GitHub Actions tests, builds, and
  archives Windows x64, Linux x64, and macOS Apple Silicon, then publishes tagged
  builds with generated release notes and SHA-256 checksums. macOS fallback
  opening uses `open` rather than Linux's `xdg-open`. The complete local gate
  passes with 55 tests, warning-denied clippy, thin-LTO release build, and a
  direct PE-header check confirming subsystem 2 (`Windows GUI`). The first tag
  remains the required hosted-run validation of the GitHub Actions matrix.
- Cancellation and remote-ignore closure on 2026-07-03 added 4 core + 1 GPUI
  test (60 total). `cargo fmt`, `check`, `test`, `clippy -D warnings`, and
  `release build` all pass.
- `RELEASE.md` now provides the v1.1.0 download map, portable installation,
  first-run, SSH, checksum, upgrade, and rollback instructions. It ships inside
  every archive and is used verbatim as the tagged GitHub release body.
- Source application: `[REDACTED]\devhub` (read-only)
- Target platform for the first working build: native Windows
- Recovery and persistence hardening on 2026-07-19: typed diagnostics
  (Diagnostic, Severity, PersistDiagnostic), concurrent bounded SSH stream
  draining with `drain_to_end`, and a `ChildKiller` that terminates orphans
  on project switch and app exit. Config/cache persistence uses atomic backups,
  unique temp files, startup recovery, writer locking, and stale-recovery
  conflict detection. All 87 tests pass.
- V2 baseline on 2026-07-20: full Git integration (branch picker, semantic
  diffs, commit history, commit box, commit graph, staged/unstaged actions,
  Fetch, Push), embedded pseudo-terminal, metadata-driven IDE launcher with
  project-aware JetBrains filtering, command palette, live theme preview, and
  the restored-project-on-startup UX. All 105 tests pass.
- Pipeline validation on 2026-07-20: the full static gate passes at 105 tests,
  the native gate covers compact/wide windows, every theme, keyboard-only
  navigation, local and SSH projects, all Git repository states, terminal
  workflows, editor discovery, long paths, and every loading/empty/error/
  success/cancelled state.
- V2.1.0 MCP release on 2026-07-21: added `devhub-mcp` crate as a read-only
  stdio MCP server (project tree, file read, content search, README, Git log)
  plus an in-app HTTP server on `0.0.0.0` with status-strip toggle and bearer
  token auth. Per-project todo list with add/toggle/delete/persistence. Removed
  the OpenCode provider and credential storage — MCP clients bring their own
  models. Replaced the chat panel with the todo side panel. 122 tests pass.
- V2.1.1 closure on 2026-07-22: stable local/SSH project identity, bounded MCP
  cancellation and symlink containment, Linux client-side decorations, Windows
  SSH-host routing, localhost-only MCP HTTP with generated bearer tokens,
  settings controls, and matching native icon metadata for the GUI and MCP
  executable identities on Windows, Linux, and macOS.
- V2.1.2 packaging closure on 2026-07-22: replaced raw release archives with a
  per-user Windows installer, one branded Linux AppImage containing both
  executable identities, and a macOS disk image containing separate branded GUI
  and MCP app bundles. Optional Windows signing and macOS signing/notarization
  use release secrets without changing local builds.
- V2.1.3 reliability closure on 2026-07-22: made MCP path containment independent
  of the server OS, preserved binary refusal for remote files, removed PowerShell
  CLIXML framing from Windows SSH errors, and added direct HTTP concurrency,
  restart, authentication, Host-header, and real-client reconnect coverage.
- V2.1.4 Linux integration closure on 2026-07-22: explicitly reapplied the
  runtime Wayland title and application ID after window creation, working around
  GPUI 0.2.2 dropping the initial title from compositor application switchers.


The plan was approved on 2026-06-30. Phase 1 created only the minimal scaffold;
no DevHub source was copied and no Git operation or DevHub modification occurred.

## Intent

Build a small native GPUI playground that teaches GPUI by implementing a narrow
slice of DevHub. This is not initially a production rewrite. It should be safe
to build, break, discard, and improve while preserving the existing DevHub as a
working behavioral and visual reference.

The first useful outcome is **DevHub Lite**: a GPUI window that can display a
small project list, select a project, and show basic details. Real project
discovery is added only after the basic GPUI concepts work with fixture data.

The project may mature over time, but each increase in scope must earn its
complexity.

After the real local-data path works, the project will deliberately develop a
Zed-inspired visual language. GPUI supplies rendering and interaction primitives;
it does not supply Zed's appearance by default.

## Goals

1. Create the smallest currently viable standalone GPUI application on Windows.
2. Learn GPUI in an explicit sequence: application, window, entity, render,
   elements, interaction, actions, focus, and background work.
3. Keep the existing DevHub operational and untouched.
4. Reuse DevHub's framework-independent Rust logic only after the GPUI shell is
   understood.
5. Preserve clean dependency direction so domain logic never depends on GPUI.
6. Maintain a runnable application at every completed milestone.
7. Build a distinct, Zed-inspired DevHub interface after real data and state
   transitions are available to design against.

## Non-goals for the learning phase

- A complete DevHub rewrite or feature-parity promise
- An editor, IDE, terminal emulator, or generic workbench
- Remote SSH project discovery
- File-tree browsing, full-text search, or file preview
- Git mutation or repository management
- Telemetry cards, remote SVG loading, or HTTP integration
- Custom window chrome before the dedicated polish phase
- Pixel-for-pixel Zed cloning or direct reuse of Zed's internal UI crates
- Theme parity with the egui application before the visual phase
- A broad reusable component library without multiple concrete consumers
- Packaging, installers, automatic updates, or release automation
- Linux cross-platform validation before the native Windows path works

These are deferred, not forbidden. They require an explicit later decision.

## Permissions and safety boundaries

### Writable target

All implementation writes are confined to:

```text
[REDACTED]\devhub-gpui
```

### Read-only reference

The following source may be inspected and selectively copied. Its Rust/egui
code is never edited, renamed, formatted, built in a way that writes into it,
or used as a mutable workspace:

```text
[REDACTED]\devhub
```

Exception granted on 2026-07-02: the `[REDACTED]\devhub\web` folder (the
Astro marketing site) may be edited in place for website updates. This is the
only writable path inside the read-only reference; the egui Rust source in
`[REDACTED]\devhub\src` remains strictly read-only.

Additional rules:

- Do not add a Cargo path dependency pointing at `[REDACTED]\devhub`; it would
  make the new project machine-specific and couple it to an application crate.
- Do not create symlinks or hard links into DevHub.
- Do not purge egui from DevHub.
- Do not run Git commands unless the user explicitly requests Git work.
- Do not launch editors, SSH sessions, URLs, or other external programs during
  early milestones.
- Do not reuse DevHub's live config/cache directories initially. The playground
  must use a distinct application/config identity so it cannot corrupt or
  migrate the existing application's data accidentally.

## Technical facts and assumptions

- GPUI is pre-1.0; breaking API changes are expected.
- Use the latest installed stable Rust toolchain that is compatible with the
  selected GPUI release.
- The selected published release is `gpui 0.2.2`, whose crates.io package still
  contains application and Windows platform startup through `Application::new()`.
  Zed's current main branch is moving platform startup into `gpui_platform`, but
  that package was not available on crates.io during Phase 1.
- The machine needs working MSVC build tools, a Windows SDK, and a current GPU
  driver with DirectX 11 support.
- The published `create-gpui-app` tool exists, but its generated template must
  be reviewed because scaffolding tools may lag GPUI's current API.
- DevHub currently uses egui/eframe immediate-mode UI. Its UI code is a rewrite,
  not a mechanical translation to GPUI.
- DevHub's config, cache, discovery, workspace, and editor-launch modules contain
  reusable logic, but small UI couplings must be removed when copied.

Current reference links:

- GPUI README: <https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md>
- GPUI API docs: <https://docs.rs/gpui/latest/gpui/>
- `create-gpui-app`: <https://docs.rs/crate/create-gpui-app/latest>
- Curated GPUI projects: <https://github.com/zed-industries/awesome-gpui>

Official GPUI source/API documentation takes precedence over tutorials and
community examples. Record the exact dependency versions selected during
bootstrap in the decision log below.

## Architecture direction

The workspace began with one application crate and added a core crate only when
real DevHub logic was introduced. Current structure after Phase 6:

```text
[REDACTED]\devhub-gpui\
├── ADR.md
├── Cargo.toml                  # workspace manifest
├── Cargo.lock                  # committed application lockfile once generated
├── README.md                   # concise build/run instructions
├── build\app_icon.rs           # shared Windows/macOS icon rendering
├── packaging\                 # Linux desktop and macOS bundle metadata
└── crates\
    ├── devhub-core\            # UI-independent local/SSH domain and workspace logic
    │   ├── Cargo.toml
    │   └── src\
    │       ├── lib.rs
    │       ├── config.rs       # TOML config with distinct devhub-gpui identity
    │       ├── cache.rs        # versioned project cache
    │       ├── cancellation.rs
    │       ├── discovery.rs
    │       ├── remote.rs       # bounded OpenSSH transport and remote operations
    │       └── workspace.rs    # project-aware tree, read, and search API
    ├── devhub-gpui\            # GPUI state and presentation
    │   ├── Cargo.toml
    │   ├── build.rs            # prepares native Windows/macOS icon assets
    │   ├── assets\appicon.svg  # canonical application icon source
    │   └── src\
    │       ├── app.rs          # application state and workbench rendering
    │       ├── assets.rs       # component icons plus embedded application icon
    │       ├── lib.rs          # scan model plus tested offline Markdown helpers
    │       ├── platform.rs     # isolated Windows frame and drag integration
    │       ├── scan.rs         # testable scan coordination/state machine
    │       ├── theme.rs        # local semantic visual tokens and font roles
    │       ├── ui.rs           # reusable presentation primitives
    │       └── main.rs         # minimal executable bootstrap
    └── devhub-mcp\             # read-only MCP server for project intelligence
        ├── Cargo.toml
        ├── build.rs            # embeds the Windows companion icon
        └── src\
            ├── lib.rs          # tool implementations (tree, read, search, README, Git log)
            └── main.rs         # stdio binary entrypoint
```

Dependency direction:

```text
devhub-gpui  --->  devhub-core  --->  std / narrowly justified libraries
     |
     +------------> gpui 0.2.2 (platform included in this published release)
     |
     +------------> gpui-component 0.5.1 (text views and editor only)
     |
     +------------> gpui-component-assets 0.5.1 (bundled Lucide SVGs)

devhub-mcp  --->  devhub-core
```

`devhub-core` must not import GPUI, egui, colors, widgets, window types, or UI
contexts. Presentation mappings such as project type to color belong in the
GPUI crate.

## Dependency policy

- Use the reviewed `gpui 0.2.2` crates.io baseline. Do not use `*` or Zed's
  moving Git main branch in the checked-in manifest. Revisit the separate
  `gpui_platform` package only when a compatible release is published and an
  upgrade has a concrete benefit.
- Preserve `Cargo.lock` because this is an application and GPUI is pre-1.0.
- Do not depend on Zed's full repository or internal Zed UI crates initially.
- `gpui-component` was excluded during bootstrap. Phase 2's native smoke test
  demonstrated concrete selection, scrolling, and Markdown-layout failures, so
  Phase 2B adopts its text components without migrating the whole UI kit. The
  `tree-sitter-languages` feature deliberately increases compile time and the
  dependency graph in exchange for maintained language parsing.
- Add no application async runtime merely out of habit; first use GPUI's executor.
- Add no logging, HTTP, SVG, theme, or serialization dependency until a milestone
  needs it.
- Keep default features minimal only when doing so is documented and does not
  make the Windows setup fragile.
- Every added dependency must be recorded under Decisions with its purpose.

## Visual direction

GPUI does not provide Zed's appearance automatically. The visual language was
deliberately created locally and accepted by the user.

Accepted direction:

- Stark, near-black shell
- `#0e0e0e` primary application surface
- Edge-to-edge panes
- 1px separators
- Almost-square geometry
- Compact rows and headers
- Restrained accent usage
- Flat metadata instead of cards and pill badges
- Client-drawn titlebar integrated with the application
- Zed-inspired principles without copying Zed UI source

Current approximate geometry:

- 34px title strip
- 30px project rows
- 28px section headers
- 22px status strip
- 260px project sidebar
- 680x440 minimum window size

Typography was increased by 1px during the final Phase 8 pass because the
earlier compact type was too difficult to read.

Do not regress toward rounded dashboard cards, large padding, bright layered
surfaces, or generic SaaS styling.

## GPUI concepts already exercised

The project has validated the following GPUI primitives. Avoid replacing these
with unnecessary abstractions before understanding the existing implementation.

- `Application`
- `WindowOptions`
- Transparent titlebars
- Root entities
- `Context<T>`
- `Render`
- Element composition
- Entity-backed state
- `cx.notify()`
- Background and foreground executors
- Actions and `KeyBinding`
- `FocusHandle`
- Scroll tracking
- Stable element IDs
- Client-drawn window controls
- Native platform fallbacks

## Implementation phases

Each phase is a reviewable checkpoint. Complete its acceptance criteria, update
this document, explain what changed and why, and stop for user review before a
large scope increase.

### Phase 0 — Approve the living plan

Deliverable:

- This reviewed plan accurately captures scope, permissions, sequencing, and
  validation.

Acceptance criteria:

- [x] User approves the plan or requested revisions are incorporated.
- [x] No implementation has started prematurely.

### Phase 1 — Preflight and scaffold

Purpose: prove the Windows toolchain and create only the minimal project shape.

Steps:

1. Read-only preflight checks:
   - `rustc --version`
   - `cargo --version`
   - `rustup show active-toolchain`
   - verify the MSVC host target
2. Check whether `create-gpui-app` is installed and inspect its `--version` and
   `--help` output.
3. If absent, request approval to install the published tool with Cargo.
4. Use `create-gpui-app --workspace` because the user explicitly prefers the
   generator when available.
5. Since the target root already contains `ADR.md`, generate into a temporary
   writable staging directory rather than overwriting or nesting blindly.
6. Inspect the generated files before copying the minimal scaffold into this
   workspace.
7. Compare generated GPUI dependencies and startup code with current official
   GPUI guidance.
8. If the generator is stale, make the smallest compatible corrections and
   record them. If it cannot generate a repairable current Windows project,
   manually create the official minimal scaffold and document the exact reason
   for the fallback.
9. Keep `ADR.md`; do not initialize or mutate Git.

Acceptance criteria:

- [x] Workspace contains only the minimal manifests, README, and one Rust source
      file in addition to this plan.
- [x] Dependency versions and generator version are recorded below.
- [x] No DevHub source has been copied.
- [x] `cargo metadata` resolves the intended workspace.

Likely failure cases and responses:

- Cargo network/DNS failure: retry only with the required network approval.
- Missing MSVC linker or SDK: stop and report the exact Visual Studio Installer
  components needed.
- Generator emits obsolete APIs: repair minimally or use the documented manual
  fallback; do not pull in the full Zed workspace.
- Dependency version conflict: select one compatible GPUI/platform pair and pin
  it; do not mix versions from unrelated examples.

### Phase 2 — Static native window

Purpose: establish the smallest understandable GPUI lifecycle.

Implement in one `main.rs`:

- platform application creation
- application run callback
- one native window with system decorations
- one root entity/view
- one `Render` implementation
- a small element tree using `div` and text
- a reasonable initial and minimum window size

Comments should explain only non-obvious GPUI concepts: who owns the entity,
when `render` runs, and how the application/window contexts are used.

Do not add themes, assets, custom title bars, macros beyond what GPUI requires,
or extra modules.

Acceptance criteria:

- [x] `cargo fmt` succeeds.
- [x] `cargo check --workspace --all-targets` succeeds without warnings caused by
      our code.
- [x] `cargo run -p devhub-gpui` opens a visible native Windows window.
- [x] Resizing and closing the window work normally.
- [x] Any visual verification limitation is stated explicitly.

### Phase 3 — Entity state and interaction

Purpose: learn state updates before introducing DevHub complexity.

Add one deliberately small interaction, such as a counter or selected fixture
project:

- state stored in the root entity
- click handler mutating state through the GPUI context
- explicit notification/refresh behavior
- hover/focus styling only if it clarifies lifecycle behavior

Keep everything in one source file unless it becomes materially harder to read.

Acceptance criteria:

- [x] Interaction updates the visible UI repeatedly without recreating state.
- [x] The code explains why state survives across renders.
- [x] Formatting, checking, and visible run validation pass.

Implementation status on 2026-06-30:

- Counter entity and click listener implemented.
- `cx.notify()` explicitly requests the follow-up render.
- `cargo fmt`, `cargo fmt --check`, `cargo check --workspace --all-targets
--locked`, and clippy with warnings denied pass.
- Visible repeated-click behavior and state retention through resize were
  confirmed by the user. Closing the app intentionally resets in-memory state.

### Phase 4 — DevHub Lite with fixture data

Purpose: validate that GPUI suits the target interaction model before copying
backend code.

Use a tiny in-memory `Project` fixture and implement only:

- a project list
- selected-row state
- a details region
- simple responsive layout within sensible minimum dimensions
- one keyboard action for selection or refresh, if the basic action API remains
  small enough

Use system fonts and simple colors. A polished visual match is not a goal.

Acceptance criteria:

- [x] List selection works by mouse.
- [x] Details update from selected state.
- [x] Empty-list and no-selection states render without panic.
- [x] Narrow-window behavior remains usable or has an explicit minimum width.
- [x] No DevHub production config/cache is read or written.

Implementation status on 2026-06-30:

- Three in-memory project fixtures render in a left pane (historical learning phase;
  removed from production startup during parity closure).
- Stable row IDs and entity-backed selected index drive a right detail pane.
- Explicit no-selection and empty-list branches are present.
- The native window uses an 820x520 initial size and 640x400 minimum.
- Formatting, locked workspace checking, and clippy with warnings denied pass.
- Visible mouse selection, row highlighting, detail updates, and resize behavior
  were confirmed by the user.

### Phase 5 — Introduce `devhub-core`

Purpose: reuse proven backend behavior without importing egui architecture.

Create `crates/devhub-core` only now. Copy the smallest coherent subset from the
read-only DevHub reference. Start with project model and local discovery; defer
everything else.

Candidate source modules, in adoption order:

1. project model and local discovery from `src/discovery`
2. cache logic, only after a distinct playground cache identity is defined
3. configuration, only after theme/appearance types are separated from UI
4. workspace/file operations, only when the UI needs them
5. editor launching, only after external-process behavior is explicitly approved

Required adaptations:

- Move project-color mappings out of the model and into `devhub-gpui`.
- Move theme and appearance configuration out of framework-independent config.
- Preserve paths as `PathBuf`; convert to display strings only at the UI edge.
- Avoid reading or migrating existing DevHub data automatically.
- Audit all subprocess calls. Early local discovery must not mutate Git or launch
  interactive commands.
- Record copied file/function provenance in this plan so future DevHub changes can
  be compared deliberately.

Testing focus:

- marker-file detection
- duplicate-project handling
- missing/unreadable directories
- non-UTF-8 or unusual Windows paths where practical
- empty results
- deterministic sorting

Acceptance criteria:

- [x] `devhub-core` compiles without GPUI or egui dependencies.
- [x] Core tests pass and do not touch real user config/cache data.
- [x] Fixture UI can switch to core `Project` values without changing its visual
      architecture.

Implementation status on 2026-06-30:

- Added `crates/devhub-core` with only `ignore 0.4.26` as a direct dependency.
- Ported the project model, marker detection, local walking, deterministic sort,
  direct `.git/config` origin reading, timestamps, search keys, and deduplication.
- Removed egui colors, serialization, tracing, remote SSH, UI scan status, config,
  cache, and editor-launch concerns from the copied boundary.
- Adapted Rust 2024 let-chains to Rust 2021 nested conditions.
- Avoided the source scanner's synthetic parent entry when real child projects
  have already been discovered.
- Added four isolated temporary-directory tests; all pass.
- Switched GPUI fixture values to the shared core `Project` model without running
  discovery from the UI.
- Formatting, locked workspace checks/tests, and clippy with warnings denied pass.
- The user confirmed the Phase 5 visual smoke test remained unchanged.

### Phase 6 — Background local scanning

Purpose: learn GPUI's executor and safe entity updates using a real workload.

Implement:

- an explicit user-triggered local scan
- filesystem work off the UI thread
- loading, success, empty, and error states
- safe delivery of results back to the owning entity
- protection against stale results when multiple scans overlap
- cancellation or generation-token behavior if GPUI's task model makes it
  straightforward

Do not add SSH, continuous watching, or incremental indexing.

Acceptance criteria:

- [x] The window remains responsive while scanning.
- [x] Repeated refreshes do not apply stale results or panic after window close.
- [x] Errors are visible and actionable rather than silently logged.
- [x] Large result sets are bounded or rendered with an appropriate GPUI list
      primitive before they can freeze the UI.

Implementation status on 2026-07-01:

- Added an explicit `Scan`/`Scan again` action for the process's current working
  directory; no source is scanned automatically.
- Blocking discovery runs on GPUI's background executor and returns through a
  foreground task holding only a weak entity reference.
- A generation counter rejects stale overlapping results; entity release after
  window close is handled without updating released state.
- Loading, loaded, empty, and error states are modeled explicitly and rendered.
- The project list has a stable element ID and vertical scrolling.
- Moved pure scan coordination into the small testable `devhub-gpui` library;
  disabled the native binary test harness after rustc stack overflowed on its
  deeply composed GPUI element type. The binary remains covered by
  `cargo check --all-targets` and clippy.
- Six tests pass: four discovery tests and two scan-state/staleness tests.
- Formatting, locked tests/checking, and clippy with warnings denied pass.
- Visible responsiveness, real-result replacement, repeated scans, selection,
  and safe closing were confirmed by the user.

### Phase 7 — Zed-inspired visual foundation

Purpose: modernize DevHub's presentation deliberately after real project data,
loading, empty, and error states exist. GPUI does not provide a Zed skin; this
phase creates a small local visual language using GPUI primitives.

Implement:

- a dark-first set of named design tokens for surfaces, text, borders, accents,
  selection, focus, success, warning, and error states
- intentional UI and monospace font roles with a compact type scale
- a stark, compact geometry: 28-32px rows, 1px separators, 0-1px radii,
  restrained padding, and edge-to-edge panes instead of card-like surfaces
- a Zed-inspired application shell for the project list, detail region, toolbar,
  and status feedback
- client-drawn Windows chrome integrated into the shell, including native
  drag, minimize, maximize/Snap Layout, close, resize, and restore behavior
- complete hover, active, selected, focused, and disabled states
- a small number of local repeated components only when duplication proves the
  boundary, such as project row, section header, badge, and status message
- contrast and fractional-DPI checks on Windows

Constraints:

- Recreate design principles; do not copy Zed's internal UI crate code.
- Hide the Windows system title bar only through GPUI's supported transparent
  titlebar and native window-control hit-test APIs; do not use Win32 bindings.
- Do not add animation, remote assets, or a broad component framework yet.
- Preserve all loading, empty, error, and populated states while restyling.
- Keep design tokens in `devhub-gpui`; no visual type may leak into
  `devhub-core`.

Acceptance criteria:

- [x] Raw one-off colors and spacing in feature code are replaced by named local
      tokens where repetition or semantics justify them.
- [x] Project list, details, loading, empty, and error states share one coherent
      visual hierarchy.
- [x] Selection remains distinguishable without relying only on color; the focus
      token is defined for Phase 8 keyboard/focus wiring.
- [x] Layout remains usable at the minimum size and at common Windows DPI scales.
- [x] Integrated chrome preserves drag, resize, minimize, maximize, restore,
      Snap Layout hover, close, and inactive-window presentation.
- [x] Visual changes do not alter core behavior or introduce Zed source coupling.

Implementation status on 2026-07-01:

- The first visual pass was rejected during user review: it was a conventional
  dark dashboard, not the intended compact and stark Zed-inspired shell.
- Phase 7 is reopened. Rounded cards/badges, generous spacing, 44px rows, the
  lighter layered palette, and the native Windows title bar must be replaced.
- The accepted direction starts from `#0e0e0e`, uses almost-square geometry,
  thin separators, denser typography, and client-drawn integrated chrome.
- Replaced the layered blue-gray palette with a near-black semantic palette;
  the application, title strip, sidebar, and detail pane now begin at
  `#0e0e0e`, with `#202020` separators and restrained state colors.
- Replaced 44px two-line rows, rounded badges/cards, and generous pane padding
  with 30px single-line rows, flat metadata, 28px section headers, and a 22px
  status strip. The only circles left are semantic status indicators.
- Enabled GPUI's transparent Windows titlebar and added a 34px client-drawn
  title strip with platform hit-test regions for drag, minimize,
  maximize/restore with Snap Layout, and close.
- Window controls render active/inactive states, a destructive close hover,
  and a maximized-state restore glyph. The outer edge is suppressed while
  maximized.
- Phase 6 scanning and selection behavior remains unchanged. Formatting, six
  tests, locked all-target checking, and clippy with warnings denied pass.
- Native visual, DPI, and window behavior passed user review for the corrected
  shell, apart from the frame border/corner issue carried into Phase 8.
- User accepted the corrected visual direction and advanced the project to
  Phase 8, with the outer gray border and overly round DWM corners identified
  as a carry-forward frame correction.

### Phase 8 — Interaction and visual polish

Purpose: refine the proven visual foundation without turning the playground into
an unbounded design-system project.

Candidate work, adopted only when each item has a clear benefit:

- keyboard navigation, focus traversal, and visible focus treatment
- compact filtering or command actions using GPUI's action/keybinding model
- a restrained local icon set and polished project-type/status badges
- refined loading, empty, error, retry, and stale-result communication
- subtle transitions or animation where they explain state changes
- light appearance after the dark theme is coherent
- large-list profiling and virtualization if real project counts require it
- evaluation of a third-party GPUI component crate only for a demonstrated gap

Acceptance criteria:

- [x] Primary flows are implemented for mouse and keyboard.
- [x] Focus, loading, error, selected, active, and hover states are complete;
      there are no persistently disabled controls in this scope.
- [x] Animation was deliberately omitted because it does not clarify state yet.
- [x] Custom chrome has GPUI hit regions plus explicit Windows fallbacks.
- [x] Formatting, checking, eight tests, clippy, and release build pass.
- [x] Final native smoke test of the latest control fallbacks remains user-owned.

Implementation status on 2026-07-01:

- Removed the original gray application border, then added a deliberate 1px
  `#202020` edge after the native DWM edge proved inconsistent.
- Added a Windows-only DWM surface adapter using Windows' supported small-corner
  treatment. DWM does not expose an exact 2px radius;
  `DWMWCP_ROUNDSMALL` is the closest platform-owned option.
- DWM border suppression was first suspected when the user reported that the
  window could not move, but restoring a native border color did not fix drag
  behavior and the requested 1px edge remained invisible.
- Added an explicit GPUI 1px `#202020` outer edge. The title region retains
  GPUI's `WindowControlArea::Drag` and now has a Windows-native move-loop
  fallback for configurations where it receives a client mouse-down instead.
  Double-click still toggles maximize/restore.
- Added a root focus handle and GPUI actions/keybindings for Up/Down project
  selection, Home/End bounds, and Ctrl+R scanning.
- Mouse selection and the titlebar scan action return focus to the root so
  keyboard navigation remains available.
- Added a restrained shortcut hint to the status strip.
- Added explicit click fallbacks for minimize, maximize/restore, and close while
  retaining GPUI control hit regions for native behavior such as Snap Layout.
- Increased every rendered text role by 1px for legibility without loosening
  the compact layout.
- Keyboard selection now scrolls the active project into view; its empty,
  boundary, and stale-index behavior is covered by isolated tests.
- Moved Windows DWM and native move-loop code into `platform.rs` so unsafe,
  platform-specific work is not mixed into view rendering.
- Optional filtering UI, animation, light theme, virtualization, and third-party
  components were not added because no demonstrated need justifies them yet.

### Phase 9 — Maturity and repository-direction review

At this point, stop and decide whether the experiment should remain small or
become the new DevHub implementation.

Review questions:

- Is GPUI productive enough for this application?
- Are Windows rendering, text, focus, accessibility, and DPI behavior acceptable?
- Is core/UI separation clean?
- Does the Zed-inspired visual language feel coherent without copying Zed?
- Which DevHub feature gives the most value next?
- Is a third-party component crate now justified?
- Should development remain here or eventually move into the DevHub repository?

Outcome on 2026-07-01:

- The user deems the GPUI experiment successful.
- GPUI is productive enough for a DevHub successor: native rendering, async
  scanning, entity state, actions, focus, scrolling, and a distinct visual
  language have all been exercised without importing Zed UI crates.
- `devhub-core` is a sound boundary and should remain UI-independent.
- Windows client chrome is viable but is the clearest maintenance risk; keep
  platform fallbacks isolated and test them on each GPUI upgrade.
- Accessibility, IME/text input, persisted configuration, and larger project
  sets are not yet validated and must be treated as maturity work, not assumed.
- No third-party GPUI component crate is justified at this point.
- Keep `[REDACTED]\devhub` intact as the reference application. Continue this
  workspace as the successor (preferably renamed `devhub-gpui` or
  `devhub-next`) rather than purging egui in place. Promote it to the canonical
  DevHub repository only after the first two successor milestones below pass.

Recommended successor roadmap, in order:

1. Persist distinct `devhub-gpui` configuration and cache data, add configurable
   local scan roots/max depth, and preserve explicit background rescans.
2. Add compact project filtering and configured editor launching so the app
   completes DevHub's core discover-select-open workflow.
3. Port the local file tree/read/search workbench, then evaluate remote SSH
   discovery separately; do not couple remote complexity to local maturity.

### Successor Milestone 1 — Configuration and persistence

Purpose: give the successor a distinct, migration-safe persistence layer so it
can remember configured scan roots, max depth, and scanned project results
without touching egui DevHub data.

Implemented:

- `crates/devhub-core/src/config.rs`
  - Versioned `Config` schema containing appearance, local sources/depth, and
    SSH hosts/depth
  - Missing version means legacy version 0 and migrates atomically to version 1
  - Newer versions are rejected and protected from accidental overwrite
  - `load_or_create`, `save`, `ensure_dirs_exist`, `config_path`, `config_dir`,
    `cache_dir`
  - Distinct application identity `"devhub-gpui"` via `directories::ProjectDirs`
  - TOML format; missing fields fall back to defaults via `#[serde(default)]`
- `crates/devhub-core/src/cache.rs`
  - Versioned `ProjectCache` starting at version 1 (distinct from egui DevHub v4)
  - `load_projects`, `save_projects`, `cache_path`
  - Version mismatch returns no projects rather than attempting migration
- `crates/devhub-core/src/discovery.rs`
  - Added `Serialize`/`Deserialize` derives to `Project`, `ProjectSource`,
    `ProjectType` so the cache can round-trip the domain model
- `crates/devhub-gpui/src/main.rs`
  - Loads config on startup and ensures config/cache directories exist
  - Loads cached projects on first launch; falls back to fixture data only when
    no cache exists yet
  - Scans all configured `scan_dirs` (not just the current working directory)
    using the configured `max_depth`
  - Saves scanned results to the cache after a successful scan
  - Explicit rescan only; no automatic filesystem access
  - Status strip shows the first configured root (or the fallback current dir)

New dependencies (recorded in the decision log):

- `serde 1` with `derive` feature
- `toml 0.8`
- `directories 6`

Acceptance criteria:

- [x] Distinct `devhub-gpui` configuration/cache identity
- [x] Configurable local scan roots
- [x] Configurable maximum depth
- [x] Persistence to the platform configuration directory
- [x] Versioned cache format
- [x] Versioned configuration format with tested v0-to-v1 migration
- [x] Future configuration versions are rejected without being rewritten
- [x] Explicit rescan behavior
- [x] Migration safety that cannot overwrite existing egui DevHub data
- [x] Remote hosts excluded from this milestone
- [x] Formatting, checking, 14 tests, clippy, and release build pass
- [x] User native smoke test of Milestone 1 remains user-owned

Implementation status on 2026-07-01:

- Added config and cache modules to `devhub-core` with serde/toml/directories.
- The `Project` model now derives `Serialize`/`Deserialize` so the cache can
  round-trip it without a separate DTO.
- The distinct `"devhub-gpui"` identity means config and cache files live in a
  separate directory from the egui DevHub; there is no shared path and no
  migration logic that could touch the old data.
- Cached projects load on startup. The original fixture fallback was removed
  during parity closure; after a scan, results persist to `projects.toml`.
- The scan task now validates all configured roots before walking; the first
  invalid root produces a visible error state.
- Tests cover config defaults, TOML round-trip, missing-field defaults, identity
  distinctness, cache round-trip, and wrong-version rejection. No test touches
  real user config/cache data.
- Formatting, 14 tests (10 core + 4 gpui), clippy with warnings denied, and the
  release build all pass.

Exp 3 addition on 2026-07-01 after Milestone 2 passed:

- `crates/devhub-gpui/src/platform.rs`
  - `pick_folder(hwnd_raw)` — wraps the Windows native `IFileOpenDialog` with
    `FOS_PICKFOLDERS`. Blocks the calling thread; must run on a background
    executor. Uses `CoInitializeEx`/`CoUninitialize` for COM apartment init.
  - `window_hwnd_raw(window)` — extracts the raw `HWND` as `isize` so it can
    cross thread boundaries.
- `crates/devhub-gpui/src/main.rs` — settings overlay
  - Settings gear button in the titlebar (next to Scan) toggles the overlay
  - Overlay replaces the main content area when active
  - Lists pending scan directories with "x" remove buttons
  - "+ Add directory" spawns a background task calling `pick_folder`, then
    pushes the result to the pending list (deduplicated)
  - "-" / "+" buttons adjust max depth (1–20 range)
  - "Cancel" discards pending changes and closes the overlay
  - "Save && Rescan" persists to config, closes the overlay, and triggers a
    fresh scan
  - Escape also closes the overlay if active
- Drive-by: Enter keybinding fixed. `handle_filter_keydown` now silently
  consumes Enter when the filter query is non-empty. Additionally,
  `open_selected_project` guards against opening while the filter is active.
  (GPUI 0.2.2 `on_key_down` callbacks cannot stop action dispatch, so both
  the key handler and the action handler participate.)
- Drive-by: `platform.rs` import cleanup — `use crate::PathBuf` -> `use std::path::PathBuf`, removed duplicate function-scoped import.
- 23 tests pass (19 core + 4 gpui), check, clippy, and release build pass.
- No new dependencies added.

Superseded on 2026-07-02: the COM folder picker, `pick_folder`, and
`window_hwnd_raw` were removed when Phase 3 introduced the unified custom
local/SSH picker. This block remains as historical implementation context.

### Successor Milestone 2 — Complete the core DevHub loop

Purpose: complete the discover → select → open workflow so the successor can
replace the egui DevHub for its primary use case: finding a project and opening
it in an editor.

Implemented:

- `crates/devhub-gpui/src/platform.rs`
  - `open_with_picker(path, window)` — shows the Windows native "Open with"
    dialog using `SHOpenWithDialog` with `OAIF_EXEC | OAIF_ALLOW_REGISTRATION`
  - Enumerates all registered applications on the user's system; no hardcoded
    editor paths or config needed
  - The selected program is launched by Windows with the project path
  - Runs on a background thread so the GPUI window stays responsive; the HWND
    is passed as `isize` to cross the thread boundary (raw pointers are not Send)
  - Added `Win32_UI_Shell` feature to the `windows` dependency
- `crates/devhub-gpui/src/main.rs`
  - Incremental keyboard filtering via `on_key_down`:
    - Typing printable characters filters the project list by `search_key`
    - Backspace removes the last character
    - Escape clears the filter
    - Modifier-combined keys (Ctrl+R, etc.) pass through to actions
  - `OpenSelectedProject` action bound to Enter
  - An "OPEN" button in the details panel header for mouse users
  - Filtered project list: `filtered_indices()` maps the filtered view to
    underlying project indices; keyboard navigation, mouse selection, and
    auto-scroll all operate within the filtered set
  - Auto-selects the first match when filtering narrows the list
  - "NO MATCHES" empty state when the filter has no results
  - Sidebar header shows `visible/total` count when filtering
  - Status strip shows the active filter query
  - Keyboard hints updated: `type filter · ↑↓ select · Enter open · Ctrl+R scan`
  - Filter state cleared on new scans

Revised from the initial Milestone 2 approach:

- The first implementation used hardcoded editor config (`EditorConfig` with
  template strings) and VS Code install-path fallback. This was rejected
  because hardcoded paths are fragile and machine-specific.
- Replaced with `SHOpenWithDialog`, which delegates entirely to the Windows
  shell. The user picks from all registered applications on their system; no
  editor configuration is needed or stored in `config.toml`.
- Removed `crates/devhub-core/src/editor.rs` and `editors`/`default_editor`
  from `Config`. Old config files with those fields still load (serde ignores
  unknown fields by default).
- The `launch_error` field is the current shared application-error surface. The
  Windows picker handles its own dialog errors, while config/cache load and save
  failures are reported through this field rather than silently discarded.

New GPUI concepts exercised:

- `on_key_down` with `cx.listener` — capturing raw keystrokes for incremental
  text input without a full text-input widget (IME/text input remains
  unvalidated; this is an action-path filter, not a text field)
- `KeyDownEvent` / `Keystroke` — reading `key`, `key_char`, and `modifiers`
  to distinguish printable characters from action shortcuts

New Windows API exercised:

- `SHOpenWithDialog` with `OPENASINFO` and `OAIF_EXEC` — the native
  "Open with" dialog, isolated in `platform.rs`

Acceptance criteria:

- [x] Compact project filtering
- [x] Local editor launching (via native Windows picker)
- [x] Keyboard operation for filter/select/open
- [x] The complete discover → select → open workflow
- [x] Formatting, checking, 14 tests, clippy, and release build pass
- [x] User native smoke test of Milestone 2 remains user-owned

Implementation status on 2026-07-01:

- The filter uses the existing `search_key` field on `Project` (already
  lowercase) so no new indexing is needed. The query is lowercased on input.
- GPUI 0.2.2 has no high-level text input widget; `on_key_down` +
  `Keystroke.key_char` provides an incremental filter without IME complexity.
- The "OPEN" button and Enter key both call `open_selected_project`, which
  calls `open_with_picker` in `platform.rs`.
- `SHOpenWithDialog` is called on a background thread because it blocks until
  the user picks or cancels. The HWND is extracted as `isize` to cross the
  thread boundary safely (raw pointers are not `Send`).
- Removed `editor.rs` from `devhub-core`; editor launching is now entirely a
  platform concern in `platform.rs`, not domain logic.
- Config no longer stores editor definitions; `config.toml` contains only
  `scan_dirs` and `max_depth`.
- 14 tests (10 core + 4 gpui). Formatting, clippy with warnings denied, and
  release build all pass.

### Successor Milestone 3 — Local workbench

Purpose: port the local file tree, bounded file reading, and content search
workbench from the egui DevHub reference, providing a usable local-only
workbench before evaluating remote SSH complexity.

Implemented:

- `crates/devhub-core/src/workspace.rs`
  - `list_tree(root, max_depth, show_hidden) -> Result<TreeListing, String>` —
    gitignore-aware tree walk with stable parent/child preorder, per-directory
    directories-first sorting, a 500-row output bound, a 5,000-entry scan bound,
    truncation metadata, and non-fatal walker warnings
  - Dotfile visibility is independent from ignore rules; `.git`, generated,
    dependency, and vendor directories remain pruned in both modes
  - `read_file(path) -> Result<String, String>` — bounded at 512 KiB,
    returns clear error strings for directories, missing files, and oversized
    files; uses `String::from_utf8_lossy` for non-UTF-8 tolerance
  - `search_content(root, query) -> Vec<SearchHit>` — case-insensitive
    substring search, walks files with `ignore`, skips binary/large files
    by extension and size, capped at 200 hits, preview trimmed to 240 chars
  - Tests cover hierarchy, sibling ordering, hidden/ignored policy, invalid
    roots, bounded reading, and search behavior
- `crates/devhub-gpui/src/main.rs`
  - Tabbed details panel: OVERVIEW | FILES | SEARCH
  - `DetailsTab` enum with Overview, Files, and Search states
  - Files tab: async file tree loading on tab open, click-to-view files,
    "← back" button to return to tree, error state for unreadable files
  - Search tab: type query (captured via `on_key_down` when in search mode),
    press Enter to search, click result to view the file, Escape clears query
  - Selecting a new project resets all workbench state
  - Tree, README, file, and search each use explicit idle/loading/loaded/empty/
    error state and generation/path/query guards reject stale async results
  - File tree shows depth indentation, dir/file icons, and entry count
  - Search results show `file:line` and preview, with match count header
- `crates/devhub-gpui/src/ui.rs`
  - reusable message, metadata, status, icon, and window-control primitives
    separated from application/workbench state

New GPUI concepts exercised:

- A project-list `ScrollHandle` for selection visibility
- Conditional keyboard routing based on UI mode (filter vs search)
- Async work with explicit state and stale-result rejection
- File viewer with scrollable content display

Acceptance criteria:

- [x] File tree
- [x] Bounded file reading (512 KiB limit, clear errors)
- [x] Content search (case-insensitive, result limits, binary skipping)
- [x] Result limits (500 tree entries, 200 search hits, 240-char previews)
- [x] Error states (directory, missing, oversized, unreadable)
- [x] Background execution (all I/O on GPUI's background executor)
- [x] Formatting, checking, 32 tests, clippy pass
- [x] Release build
- [x] User native smoke test of Milestone 3 remains user-owned

Implementation status on 2026-07-01:

- Added `workspace.rs` to `devhub-core` with `list_tree`, `read_file`,
  `search_content`, `TreeListing`, `FileEntry`, and `SearchHit`; no `anyhow` or
  `tracing` dependency was introduced.
- The workbench UI uses three tabs: OVERVIEW (metadata), FILES (tree + viewer),
  SEARCH (type-to-search + results). All filesystem work runs on GPUI's
  background executor with explicit loading states.
- Keyboard routing: `handle_filter_keydown` checks `details_tab` and routes
  keystrokes to the search query when in Search mode, or the project filter
  otherwise. Escape clears the active context's query.
- Clicking a file in the tree or a search hit loads the file content via
  `read_file` on the background executor and displays it with a "← back" button.
- Binary files are skipped during search by extension check (exe, dll, png,
  zip, etc.) and by the 512 KiB size bound in `read_file`.
- Remote SSH was deliberately excluded per the roadmap; evaluate it as a
  separate decision after the local workbench proves mature.

### Remediation Phase 1 — Stabilize the local workbench

Purpose: make Milestone 3 truthful and dependable before expanding scope.

Implement:

- replace global filename sorting with parent-preserving, directories-first
  hierarchical ordering
- make "show hidden" affect hidden paths only; continue respecting gitignore
  and pruning `.git`, build, dependency, and vendor directories
- represent tree, README, file, and search work with explicit idle/loading/
  loaded/empty/error states
- reject stale async results after project, file, query, or hidden-mode changes
- move the native folder picker off GPUI's foreground executor
- surface config/cache persistence failures instead of discarding them
- split the monolithic application file along concrete workbench/UI boundaries
- correct README, plan drift, clippy, release-target, and stray-file hygiene

Acceptance criteria:

- [x] Tree rows remain adjacent to their parents and directories sort before
      files within each parent.
- [x] Hidden mode does not disable gitignore or expose generated directories.
- [x] Empty, loading, error, loaded, and truncated states are distinct.
- [x] Stale async work cannot replace state for a newer user selection.
- [x] Folder picking does not block the GPUI foreground executor.
- [x] Persistence failures are visible and writes are atomic where practical.
- [x] Main application responsibilities are split into maintainable modules.
- [x] Formatting, checking, tests, clippy, release build, and native review pass.

### Remediation Phase 2 — Document and Markdown subsystem

Purpose: replace passive string rendering with a real read-only document model.

Implement:

- selectable text, Ctrl+A/C, mouse drag selection, and keyboard navigation
- line numbers, wrap toggle, horizontal/vertical scrolling, and search-hit line
  positioning
- binary detection and bounded line-oriented layout
- background syntax highlighting selected by file extension
- Markdown parsing and styled README preview with a raw/preview toggle
- shared rendering foundations for code blocks and ordinary text

Implementation status on 2026-07-01:

- Phase 2A's custom eager renderer failed native review and was removed.
- `gpui-component` 0.5.1 is initialized at application startup and its `Root`
  wraps the existing application entity; the rest of the DevHub shell remains
  locally implemented.
- Files use the component's disabled rope-backed `InputState` code editor. It
  provides virtualized rendering, selection/copy, line numbers, wrap control,
  and extension-selected Tree-sitter highlighting while rejecting edits.
- Search-result clicks position the component editor at the matched line.
- README uses selectable, scrollable `TextView::markdown` for the full remaining
  Overview pane, with a compact fixed metadata-card header and raw/preview mode.
- Markdown and HTML images are sanitized into explicit alt-text placeholders
  before component rendering. A Phase 3 experiment with network-backed images
  was reverted because image decoration does not justify making local and SSH
  project inspection contact arbitrary internet hosts.
- Binary and invalid UTF-8 rejection remains in `devhub-core`; file and README
  reads remain bounded background operations.
- Selection copy and copy-all actions produce temporary visible status feedback.

Acceptance criteria:

- [x] Text selection and clipboard copy work with mouse and keyboard.
- [x] Code can wrap or remain horizontally scrollable by explicit user choice.
- [x] Syntax highlighting and scrolling remain responsive on representative files.
- [x] Markdown headings, paragraphs, lists, quotes, links, and code render locally.
- [x] README image policy is explicit and covered by offline-sanitization tests.
- [x] Formatting, checking, tests, clippy, release build, and native review pass.

### Remediation Phase 3 — Selective DevHub parity

Purpose: port product value deliberately after the local foundation is sound.

Implement in priority order:

- reliable source enable/disable, onboarding, editor preferences, open-folder,
  diagnostics, and system-appearance behavior
- a local/remote backend boundary that keeps UI state independent of transport
- remote SSH discovery/tree/read/search with timeout, cancellation, BatchMode,
  bounded output, and explicit errors
- evaluate telemetry and remote SVG cards separately; do not assume parity means
  every legacy feature must return

SSH implementation status on 2026-07-02:

- `devhub-core` exposes one project-aware tree/read/README/search API and routes
  each operation by `ProjectSource`; GPUI does not contain transport scripts.
- `remote.rs` owns validated OpenSSH invocation, POSIX shell scripts, parsing,
  hierarchy reconstruction, fixed limits, and a 30-second operation deadline.
- SSH uses `BatchMode=yes`, `ConnectTimeout=8`, one connection attempt, existing
  user OpenSSH config/keys, and the user's normal host-key policy. It neither
  stores credentials nor weakens host-key checking.
- Remote discovery detects direct project markers and Git repositories under
  configured roots, prunes known generated directories, and caps candidate and
  project output. Failed hosts are reported without discarding successful local
  or remote results.
- The custom source picker navigates local drives/directories and remote
  directories in the same settings flow. It persists normalized, deduplicated
  hosts and roots only under the separate `devhub-gpui` identity.
- Remote projects support tree browsing, hidden-file control, bounded UTF-8 file
  and README reads, fixed-string content search, and stale-result rejection.
- Opening a project is Zed-first. Local paths are passed to `zed`; SSH projects
  use `ssh://user@host[:port]/path`. `OPEN IN…` invokes the Windows application
  chooser for either a local path or the SSH URI as a best-effort fallback.
- Static validation passed with 36 core and 6 GPUI tests before the later asset
  pass. A real SSH host passed the native acceptance gate on 2026-07-02.

Acceptance criteria:

- [x] Local behavior remains unchanged when remote support is unavailable.
- [x] Remote operations run off the UI executor, reject stale results, and time out.
- [x] Config/cache formats remain versioned and migration-safe.
- [x] Features selected for omission are documented as product decisions.
- [x] Formatting, checking, tests, clippy, release build, and native review pass.

Run validation after every meaningful code change, proportionate to the change:

1. `cargo fmt`
2. `cargo fmt --check`
3. `cargo check --workspace --all-targets`
4. `cargo test --workspace` once tests exist
5. `cargo run -p devhub-gpui` when application behavior changes

`cargo clippy --workspace --all-targets -- -D warnings` is introduced after the
first stable window milestone. Do not let framework-generated warnings block the
bootstrap without first distinguishing them from warnings in our code.

GUI run procedure:

- Launch the native application with a bounded observation period.
- Confirm a window was created and remains responsive.
- Verify expected text/interaction visually when possible.
- Close gracefully; do not leave background processes running.
- If the environment cannot inspect a native window, ask the user for visual
  confirmation and report what was verified from process status/logs.

If validation fails:

- capture the exact command and concise error
- determine whether the cause is code, dependency/API drift, missing toolchain,
  GPU/driver support, or sandbox/network restriction
- fix in-scope code failures
- request required permission rather than bypassing restrictions
- report exact installation steps for external prerequisites
- update Known issues when a failure may recur

## Windows-specific edge cases

- MSVC linker/build tools absent or not visible to the active shell
- Windows SDK version missing
- stale GPU driver or Microsoft Basic Display Adapter in use
- unsupported/virtualized GPU despite DirectX availability
- long path handling when Cargo's registry/git caches become deeply nested
- antivirus or indexing software locking Cargo build artifacts
- native window launch succeeding while automated visual inspection is unavailable
- display scaling and fractional DPI affecting layout assumptions
- system light/dark appearance changing while the app runs
- paths containing spaces, Unicode, drive roots, or UNC prefixes
- shell differences between PowerShell and Developer Command Prompt

The first milestone uses native Windows directly, not WSL/WSLg.

### Windows chrome fallback inventory

This is the most platform-specific area of the application. The app hides the
native Windows titlebar using GPUI's transparent titlebar support and draws its
own title strip. GPUI window-control hit regions alone were unreliable on the
user's system.

Observed behavior during development:

- Minimize/maximize/close worked at one point
- Window movement later failed
- DWM border suppression was suspected but was not the actual complete cause
- Native DWM border color did not produce a consistently visible edge
- User requested a visible 1px border
- Window controls later became nonfunctional again

The current implementation therefore includes fallbacks that must not be
removed merely because the code looks redundant. They exist because actual
Windows behavior differed from the expected GPUI path.

- `WindowControlArea::Drag` remains attached to the title region.
- `platform.rs` provides a Windows-native move-loop fallback using
  `ReleaseCapture`, `PostMessageW`, `WM_NCLBUTTONDOWN`, and `HTCAPTION`.
- Double-clicking the drag region should maximize or restore the window.
- Minimize, maximize/restore, and close retain GPUI window-control hit regions.
- They also have explicit click fallbacks: `window.minimize_window()`,
  `window.zoom_window()` (superseded by the `IsZoomed`-aware toggle), and
  `window.remove_window()`.
- The GPUI root draws an explicit 1px `#202020` edge.
- Windows DWM is asked to use its small-corner treatment. DWM does not expose
  an exact 2px corner radius; `DWMWCP_ROUNDSMALL` is the closest supported
  platform option.

All Windows-specific API calls stay isolated in `platform.rs`. Treat custom
Windows chrome as a maintenance risk that must be retested on GPUI upgrades.
If window controls fail, diagnose the precise event path before rewriting the
titlebar again.

## Coding rules

- Prefer clear Rust and concrete types over generic abstractions.
- Keep the file structure minimal; split a module only when it has a distinct
  responsibility and the current file is becoming difficult to navigate.
- Explain GPUI-specific ownership/context behavior near the relevant code.
- Do not comment obvious Rust syntax.
- Avoid `unsafe` unless GPUI itself requires interaction that cannot be expressed
  safely; any use requires explicit review.
- Avoid global mutable state.
- Keep blocking filesystem/subprocess work off the UI thread.
- Model empty, loading, success, and failure states explicitly.
- Use stable IDs for interactive/list elements where GPUI state retention depends
  on identity.
- Do not hide errors merely to keep the demo running.
- Prefer system window behavior and accessibility over custom chrome initially.

## Scope-control rules

Before adding a feature, answer:

1. Which GPUI concept does this teach or validate?
2. Can it be implemented without adding a dependency?
3. Does it belong in the UI crate or core crate?
4. Does it threaten the runnable checkpoint?
5. Is it already listed as a non-goal?

If the feature does not improve learning or validate the DevHub direction, defer
it.

## Progress-reporting protocol

After each meaningful change, report:

- outcome first
- files changed
- GPUI concept introduced
- why the change was scoped that way
- validation commands and results
- known limitation or next review gate

Update this document in the same change with:

- current stage and completed checkboxes
- exact dependency/tool versions when they become known
- decisions and deviations
- current project tree if it materially changes
- known issues
- next approved checkpoint

The final handoff for any milestone must:

- summarize the project structure
- explain the GPUI concepts currently used
- suggest exactly three next experiments, no more

Treat the user as the visual observer. Do not claim native behavior is verified
from compilation alone.

## Validation gate

After every meaningful change, run validation proportional to the change. The
full static gate is:

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

The last completed test validation had 118 `devhub-core` tests, 38 `devhub-gpui`
tests, 4 `devhub-mcp` tests, 160 total passing tests, all-target checking passing,
clippy passing with warnings denied, and a release build passing.

The native GPUI binary has `test = false` because embedding unit tests in the
deeply composed binary previously caused rustc stack overflow. Pure testable
behavior belongs in the library modules.

If the release executable is currently running, Windows may lock
`target\release\devhub-gpui.exe`. Do not terminate the user's process without
permission; ask them to close the app before rebuilding.

## Definition of the first learning milestone

The initial journey is successful when all of the following are true:

- [x] A native Windows GPUI application starts with `cargo run -p devhub-gpui`.
- [x] The user can explain where application, window, entity, render, and element
      responsibilities live in the code.
- [x] Entity-backed state updates through one mouse interaction.
- [x] A DevHub-like list/details layout works with fixture data.
- [x] One local background scan feeds results into the UI without blocking it.
- [x] Core discovery logic is independent of GPUI.
- [x] Formatting, checking, tests, and release build validation pass.
- [x] DevHub remains untouched and functional as the reference implementation.

## Definition of the visual-modernization milestone

The later visual journey is successful when all of the following are true:

- [x] Real project data and every operational state use one coherent token set.
- [x] The interface is recognizably Zed-inspired without copying Zed UI code.
- [x] Compact layout, typography, borders, selection, and focus states are
      consistent across the application.
- [x] Mouse and keyboard flows work at the minimum window size and common Windows
      DPI scales.
- [x] Polish remains local and maintainable rather than becoming a speculative
      general component framework.

## Decision log

| Date       | Decision                                                                     | Reason                                                                                                                                                                                                                                                           |
| ---------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-06-30 | Use native Windows for the first build                                       | It is the available visual desktop and GPUI supports Windows natively.                                                                                                                                                                                           |
| 2026-06-30 | Keep `[REDACTED]\devhub` read-only                                           | Preserve the working application as a behavioral and visual baseline.                                                                                                                                                                                            |
| 2026-06-30 | Build separately in `[REDACTED]\devhub-gpui`                                 | Avoid a big-bang egui purge and allow the experiment to fail safely.                                                                                                                                                                                             |
| 2026-06-30 | Start with one GPUI app crate; add `devhub-core` later                       | Keeps bootstrap beginner-friendly while preserving a clean future boundary.                                                                                                                                                                                      |
| 2026-06-30 | Use `create-gpui-app` when installable, then review its output               | Honors the requested bootstrap path without trusting a potentially stale template blindly.                                                                                                                                                                       |
| 2026-06-30 | Use fixture data before copying DevHub logic                                 | Separates GPUI learning problems from filesystem/domain integration problems.                                                                                                                                                                                    |
| 2026-06-30 | Use a distinct config/cache identity                                         | Prevent accidental corruption or migration of existing DevHub user data.                                                                                                                                                                                         |
| 2026-06-30 | Pin published `gpui 0.2.2` instead of Zed Git main                           | Gives the playground a reproducible baseline and avoids unreleased platform API drift.                                                                                                                                                                           |
| 2026-06-30 | Retain the generator's Rust 2021 edition for bootstrap                       | Avoids an unrelated edition change while validating the generated scaffold.                                                                                                                                                                                      |
| 2026-06-30 | Use system decorations with explicit 820x520 and 640x400 minimum bounds      | Validates ordinary Windows behavior before attempting custom chrome or responsive layouts.                                                                                                                                                                       |
| 2026-06-30 | Schedule Zed-inspired design after real data integration                     | GPUI supplies primitives rather than Zed's skin; designing against real states avoids polishing disposable fixture UI.                                                                                                                                           |
| 2026-06-30 | Recreate Zed design principles without copying internal UI code              | Keeps the playground independent, small, and clear of Zed application-code coupling.                                                                                                                                                                             |
| 2026-06-30 | Add `ignore 0.4.26` as the sole `devhub-core` dependency                     | Reuses DevHub's gitignore-aware walking without importing UI, async, logging, or serialization layers.                                                                                                                                                           |
| 2026-06-30 | Do not emit a synthetic scan-root project after finding children             | Keeps configured source folders from appearing as misleading aggregate projects.                                                                                                                                                                                 |
| 2026-07-01 | Scan only the current working directory on explicit request                  | Validates real background work without config, persistence, or automatic filesystem access.                                                                                                                                                                      |
| 2026-07-01 | Isolate scan coordination in a testable library module                       | Tests stale results and state transitions without using the deeply composed native GPUI binary test harness.                                                                                                                                                     |
| 2026-07-01 | Use a local semantic dark theme instead of Zed UI crates                     | Creates a Zed-inspired hierarchy without copying application code or coupling core models to presentation.                                                                                                                                                       |
| 2026-07-01 | Keep native decorations through the visual-foundation phase                  | Preserves correct Windows movement, resize, minimize, maximize, close, DPI, and accessibility behavior during restyling.                                                                                                                                         |
| 2026-07-01 | Supersede the native-decoration Phase 7 decision with GPUI client chrome     | User review established integrated chrome as part of the visual target; GPUI hit-test regions retain native commands.                                                                                                                                            |
| 2026-07-01 | Treat the experiment as successful and continue as a successor               | The playground validates GPUI state, async work, actions, scrolling, visual styling, and Windows integration.                                                                                                                                                    |
| 2026-07-01 | Keep the egui DevHub intact until successor milestones 1 and 2 pass          | Preserves a working reference while configuration, persistence, and the discover-select-open workflow are rebuilt.                                                                                                                                               |
| 2026-07-01 | Add `serde 1`, `toml 0.8`, `directories 6` to `devhub-core`                  | Required for TOML config and versioned project cache persistence. No presentation or async runtime introduced.                                                                                                                                                   |
| 2026-07-01 | Use `"devhub-gpui"` as the distinct config/cache identity                    | Structural migration safety: separate directory from egui DevHub's `"devhub"` identity; cannot collide or overwrite.                                                                                                                                             |
| 2026-07-01 | Start `devhub-gpui` project cache at version 1                               | Distinct from egui DevHub's cache v4; version mismatch returns empty rather than migrating, preserving isolation.                                                                                                                                                |
| 2026-07-01 | Derive `Serialize`/`Deserialize` on the core `Project` model                 | Lets the cache round-trip the domain model without a separate DTO or UI coupling.                                                                                                                                                                                |
| 2026-07-01 | Add `editor.rs` to `devhub-core` for editor launching                        | Keeps subprocess launching in the domain layer; returns `Result<(), String>` instead of `anyhow` for direct UI display.                                                                                                                                          |
| 2026-07-01 | Use `on_key_down` for filtering instead of a text input widget               | GPUI 0.2.2 has no high-level text input; `on_key_down` + `key_char` provides incremental filtering without IME complexity.                                                                                                                                       |
| 2026-07-01 | Replace hardcoded editor config with `SHOpenWithDialog`                      | Hardcoded editor paths are fragile and machine-specific; the native Windows "Open with" dialog enumerates all registered apps on the user's system without configuration.                                                                                        |
| 2026-07-01 | Move editor launching from `devhub-core` to `platform.rs`                    | `SHOpenWithDialog` is a Windows Shell API call, not domain logic; keeping it in `platform.rs` preserves the core/UI separation and isolates all unsafe platform code.                                                                                            |
| 2026-07-01 | Add `Win32_UI_Shell` feature to the `windows` dependency                     | Required for `SHOpenWithDialog` and `OPENASINFO`; no other Shell APIs are used.                                                                                                                                                                                  |
| 2026-07-01 | Guard `OpenSelectedProject` action when filter is active                     | GPUI `on_key_down` callbacks cannot prevent action dispatch; the action handler checks `filter_query` as a practical fix for Enter opening projects during search.                                                                                               |
| 2026-07-01 | Experiment 1 — Remote SSH discovery evaluation (initially deferred)          | Superseded on 2026-07-02 after Phase 2 matured and the user promoted SSH to required scope.                                                                                                                                                                      |
| 2026-07-01 | Experiment 2 — List virtualization evaluation (not needed)                   | Tree cap is 500 entries. GPUI renders thousands of elements eagerly without issue. No demonstrated benefit.                                                                                                                                                      |
| 2026-07-01 | Experiment 3 — Config UI settings overlay (initial implementation)           | Initially used `IFileOpenDialog`; Phase 3 retained depth/save behavior but replaced the picker with the custom local/SSH browser.                                                                                                                                |
| 2026-07-01 | Add `Win32_System_Com` to Windows dependency features                        | Historical COM picker requirement; removed from the manifest on 2026-07-02.                                                                                                                                                                                      |
| 2026-07-01 | Add `pick_folder` and `window_hwnd_raw` to `platform.rs`                     | Historical native-picker bridge; both functions were removed on 2026-07-02.                                                                                                                                                                                      |
| 2026-07-01 | Add settings overlay with folder picker, depth controls, save/rescan         | The overlay remains; its blocking native picker was replaced by asynchronous custom local/SSH directory listing.                                                                                                                                                 |
| 2026-07-01 | Note: `on_click` and `overflow_y_scroll` are on `StatefulInteractiveElement` | These methods require `Stateful<Div>` (obtained via `.id()`). Bare `Div` does not provide them. Documented for future GPUI work.                                                                                                                                 |
| 2026-07-01 | Adopt `gpui-component` 0.5.1 for Phase 2 text surfaces                       | Its GPUI 0.2.2-compatible virtualized Markdown view and rope-backed editor directly address measured scrolling, selection, wrapping, and rendering failures. The broader component library is not adopted as the application design system.                      |
| 2026-07-01 | Enable `gpui-component/tree-sitter-languages`                                | Maintained syntax parsing is worth the larger compile graph for the document viewer; this replaces the failed custom lexical highlighter.                                                                                                                        |
| 2026-07-02 | Promote SSH parity into Remediation Phase 3                                  | SSH discovery and workbench access are staple DevHub capabilities; the user explicitly accepted the implementation complexity.                                                                                                                                   |
| 2026-07-02 | Use the installed OpenSSH client with POSIX scripts                          | Reuses SSH config, agents, keys, proxies, and host-key policy without embedding credentials or adding a second SSH stack.                                                                                                                                        |
| 2026-07-02 | Replace the native local picker with one custom local/SSH picker             | One source workflow can navigate Windows drives and remote directories while making the configured provider explicit.                                                                                                                                            |
| 2026-07-02 | Keep SSH transport in `devhub-core/src/remote.rs`                            | UI state remains transport-agnostic; process limits, validation, scripts, parsing, and deadlines have one testable owner.                                                                                                                                        |
| 2026-07-02 | Open remote projects through VS Code Remote SSH initially (superseded)       | Historical Phase 3 choice replaced by the Zed-first product decision later on 2026-07-02.                                                                                                                                                                        |
| 2026-07-02 | Raise inherited text weight to medium                                        | Improves legibility without increasing the compact geometry; structural labels use semibold for hierarchy.                                                                                                                                                       |
| 2026-07-02 | Use gpui-component icons in the file tree                                    | SVG chevrons, open/closed folders, and recognizable file categories are clearer than font-dependent geometric glyphs.                                                                                                                                            |
| 2026-07-02 | Enable README raster and SVG web images                                      | GPUI 0.2.2 and TextView already decode both; repository-relative paths are rewritten through recognized GitHub/GitLab raw URLs.                                                                                                                                  |
| 2026-07-02 | Add `gpui-component-assets` 0.5.1                                            | Native review exposed that `IconName` provides paths, not files; the published matching asset bundle supplies the required Lucide SVGs.                                                                                                                          |
| 2026-07-02 | Install a bounded reqwest-backed GPUI HTTP client                            | `Application::new()` defaults to `NullHttpClient`; a one-worker client with 10s connect, 30s total, and 12 MiB response limits makes README images operational.                                                                                                  |
| 2026-07-02 | Embed `appicon.svg` in assets and Windows resource ID 1                      | Displays the supplied icon in client chrome and lets GPUI load the native taskbar/window icon from the executable.                                                                                                                                               |
| 2026-07-02 | Bypass GPUI's Windows `zoom_window()` for restore                            | GPUI 0.2.2 always calls `SW_MAXIMIZE`; the isolated platform fallback checks `IsZoomed` and selects `SW_RESTORE` or `SW_MAXIMIZE`.                                                                                                                               |
| 2026-07-02 | Revert network-backed README images                                          | README images are decorative and inconsistent in GPUI Component; deterministic alt-text placeholders preserve offline local/SSH inspection and avoid third-party requests. The app HTTP client, Tokio runtime, URL rewriting, and badge extraction were removed. |
| 2026-07-02 | Make DevHub a Zed-first project hub                                          | Both local paths and documented Zed SSH URLs open through `zed`; the native application chooser remains a secondary best-effort path rather than maintaining editor profiles.                                                                                    |
| 2026-07-02 | Persist legacy theme and appearance choices                                  | Five DevHub palettes and System/Dark/Light modes are small serializable preferences; Monochrome Dark remains the migration-safe default for existing visual direction.                                                                                           |
| 2026-07-02 | Keep independent SSH scan depth                                              | `RemoteHostConfig.max_depth` is now edited per host and is no longer overwritten by the local/global depth on save.                                                                                                                                              |
| 2026-07-02 | Defer source toggles, FTUE, and Explorer opening                             | Historical decision, partially superseded: a minimal first-run settings flow was later implemented. The remaining choices stayed outside the approved product scope.                                                                                             |
| 2026-07-02 | Ship v1.0.0 as the first standalone tagged release                           | Cross-platform Windows/Linux/macOS archives and SHA-256 checksums attached to the GitHub release; release workflow verified via `github-actions[bot]`; user confirmed native Windows validation of the shipped binary.                                           |
| 2026-07-02 | Coexistence over cutover                                                     | The egui `devhub` and GPUI `devhub-gpui` coexist as separate products. No identity rename, no config migration, no archival of `devhub`. Internal names stay as-is. The website will present GPUI as the primary "DevHub" and egui as a "Legacy" download.       |
| 2026-07-02 | Grant a writable exception for `[REDACTED]\devhub\web`                       | The Astro marketing site lives in the read-only egui repo. Website updates require editing it in place. The exception is scoped to `web/` only; egui Rust source remains read-only.                                                                              |
| 2026-07-02 | Retire `prompt.md`; consolidate into `ADR.md`                                | `prompt.md` duplicated plan content and had gone stale (missing `gpui-component`, listed `main.rs` as the large file). Unique content (visual direction, chrome fallback inventory, validation gate, GPUI concepts exercised) merged into `ADR.md`.              |
| 2026-07-03 | Add `CancellationToken` primitive in `cancellation.rs`                       | Cooperative cancellation for all async operations (scan, tree, file, readme, search) — a shared `Arc<AtomicBool>` with `cancel()`/`is_cancelled()`/`check()`.                                                                                                    |
| 2026-07-03 | Wire cancellation through `discovery.rs`, `remote.rs`, `workspace.rs`        | Every cancellable operation gains a `_cancellable` variant taking `&CancellationToken`. Existing non-cancellable APIs default to a fresh no-op token.                                                                                                            |
| 2026-07-03 | Add cancellable variants to `scan.rs` and `app.rs` with "Stop" button        | Scan model preserves last-known-good projects on cancel. Titlebar shows a red "Stop" button when any operation is in-flight; cancels all tokens and resets loading states.                                                                                       |
| 2026-07-03 | Add `REMOTE_IGNORE_FUNCTION` to `remote.rs`                                  | POSIX shell function walks parent directories for `.git` and runs `git check-ignore -q`. Applied to remote project discovery, tree listing, and content search — matches local `ignore` crate behavior over SSH.                                                 |
| 2026-07-03 | Add `CREATE_NO_WINDOW` (0x08000000) to Windows SSH spawn                     | Prevents a console window from flashing when launching SSH subprocesses on Windows.                                                                                                                                                                              |
| 2026-07-03 | Add pin/unpin projects with `pinned_projects` in Config                      | Star icon in project rows replaces the selection chevron when pinned. Pin-first sort: pinned projects sort before unpinned, then by source → name → path.                                                                                                        |
| 2026-07-03 | Add hide/archive projects with `hidden_projects` in Config                   | Hidden projects are filtered from the project list. Settings panel shows a "HIDDEN PROJECTS" section with per-project EyeOff unhide button.                                                                                                                      |
| 2026-07-03 | Add right-click context menu with backdrop and bounds clamping               | Right-click any project row opens an absolutely-positioned menu (Pin/Hide). Backdrop click-outside closes. Position clamped to window bounds to prevent overflow.                                                                                                |
| 2026-07-03 | Theme filter and SSH/path inputs with `appearance(false)` and bg/border      | Filter input goes edge-to-edge with `.appearance(false)`. Remote name, host, and path inputs get `bg(surface_background)` + `border_color` + 24px height consistency.                                                                                            |
| 2026-07-03 | Remove hardcoded metadata header height `h(124px)`                           | The fixed height caused a divider overlap with the component README pane. Header now sizes to its content.                                                                                                                                                       |
| 2026-07-03 | Bump version to v1.1.0; close the parity project                             | Approved functional parity completed. Optional enhancements documented with complexity estimates. The temporary website deferral was superseded by the public-surface closure recorded below.                                                                    |
| 2026-07-03 | Close the public project surface                                             | The v1.1.0 archives and checksums are published; the live site presents GPUI as primary and the archived egui implementation as legacy. Future work is optional product investment, not unfinished parity.                                                       |

Versions to record during Phase 1:

| Item              | Selected version | Notes                                                    |
| ----------------- | ---------------: | -------------------------------------------------------- |
| Rust toolchain    |           1.93.1 | `stable-x86_64-pc-windows-msvc`                          |
| Cargo             |           1.93.1 | Generated a 688-package cross-platform lockfile          |
| `create-gpui-app` |            0.1.5 | Workspace shape retained; moving Git dependency replaced |
| `gpui`            |            0.2.2 | Published crates.io release recorded in `Cargo.lock`     |
| `gpui_platform`   |     Not selected | Not published separately on crates.io at this checkpoint |

## Source-reuse ledger

| New destination                                         | DevHub source                                                   | Adaptations                                                                                                                                                                                     | Tests added                                                            |
| ------------------------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `crates/devhub-core/src/discovery.rs`                   | `[REDACTED]\devhub\src\discovery\scanner.rs` local subset       | Removed egui/serde/tracing/anyhow/SSH/status coupling; Rust 2021 conditions; suppressed synthetic parent after child discovery; added `Serialize`/`Deserialize` derives for cache round-trip    | Four isolated filesystem tests                                         |
| `crates/devhub-core/src/config.rs`                      | `[REDACTED]\devhub\src\config.rs`                               | Distinct `"devhub-gpui"` identity; retained only local/SSH source fields; normalized and merged remote hosts without migrating legacy data                                                      | Six config tests                                                       |
| `crates/devhub-core/src/cache.rs`                       | `[REDACTED]\devhub\src\cache.rs`                                | Start at version 1 (distinct from egui v4); `Option`-returning API; no `anyhow` dependency                                                                                                      | Two cache tests (round-trip, wrong-version rejection)                  |
| `crates/devhub-gpui/src/app.rs` startup and scan flow   | Previous local UI-only fixture struct                           | Uses shared `Project`, `ProjectSource`, and `ProjectType`; production fixtures were removed; startup loads versioned config/cache and scans only explicit roots                                 | Startup and scan-state tests plus workspace compile/clippy             |
| `crates/devhub-gpui/src/platform.rs` (open_with_picker) | Windows `SHOpenWithDialog` API                                  | Isolated in platform.rs; runs on background thread; HWND passed as isize for Send safety                                                                                                        | Covered by workspace compile/clippy                                    |
| `crates/devhub-core/src/remote.rs`                      | `[REDACTED]\devhub\src\workspace.rs` and `discovery/scanner.rs` | Rebuilt around validated targets, cancellation, hard deadlines, bounded output, hierarchy preservation, remote ignore rules, partial scan errors, strict UTF-8/binary handling, and no `anyhow` | Parser, validation, quoting, hierarchy, cancellation, and ignore tests |
| `crates/devhub-gpui/src/app.rs` source picker           | `[REDACTED]\devhub\src\app.rs` picker behavior                  | Reimplemented in GPUI as one custom local-drive and SSH-directory browser; removed the Windows COM folder picker                                                                                | Covered by core picker-operation tests and workspace compile           |

## Functional parity matrix

| Capability                                | Status   | Notes                                                                              |
| ----------------------------------------- | -------- | ---------------------------------------------------------------------------------- |
| Local and SSH discovery                   | Complete | Explicit background scan, partial failures, stale-result rejection                 |
| Configurable local/SSH roots              | Complete | Unified custom picker and crash-safe persistence                                   |
| Local and per-host scan depth             | Complete | Independent values from 1 through 20                                               |
| Versioned configuration                   | Complete | Unversioned/v0 migrates atomically to v1; future versions are read/write protected |
| Versioned project cache                   | Complete | Separate `devhub-gpui` identity                                                    |
| Project filtering and keyboard navigation | Complete | Includes automatic list scrolling                                                  |
| Local/SSH tree, read, README, and search  | Complete | Bounded operations; README images remain offline placeholders                      |
| Zed local and SSH opening                 | Complete | User-confirmed native behavior for `zed <path>` and `zed ssh://…` targets          |
| Native `OPEN IN…` fallback                | Complete | User-confirmed; remote apps must understand `ssh://`                               |
| Project metadata                          | Complete | Source, host, Git remote, markers, modified time                                   |
| Legacy themes and appearance              | Complete | Five palettes; System/Dark/Light                                                   |
| First-time user experience                | Complete | No config and no cache opens Settings; no production fixtures                      |
| Operation cancellation                    | Complete | All in-flight scan, tree, file, readme, and search operations cancellable          |
| Remote `.gitignore` semantics             | Complete | `REMOTE_IGNORE_FUNCTION` walks parent dirs for `.git` and runs `git check-ignore`  |
| Pin/favorite projects                     | Complete | Persisted in config; star icon; pin-first sort                                     |
| Hide/archive projects                     | Complete | Persisted in config; filtered from list; EyeOff unhide in Settings                 |
| Right-click context menu                  | Complete | Backdrop dismissal; window-bounds clamping; Pin/Hide actions                       |
| SSH `CREATE_NO_WINDOW` (Windows)          | Complete | No console flash for SSH subprocesses                                              |
| Input field theming                       | Complete | Filter uses `appearance(false)`; SSH/path inputs use `surface_background`          |

## Known issues and open questions

- `cl.exe` and `link.exe` are not exposed on the ordinary PowerShell `PATH`, but
  Cargo successfully discovered the installed MSVC toolchain during compilation.
- `gpui 0.2.2` failed to discover `fxc.exe` because its build script's hard-coded
  fallback expects Windows SDK `10.0.26100.0`. This machine has the required x64
  compiler at `[Windows_SDK]\bin\10.0.19041.0\x64\fxc.exe`.
  Set `GPUI_FXC_PATH` to that file in the build shell. Keep this machine-specific
  path out of committed Cargo configuration.
- A normal `cargo run` can succeed in a fresh shell after GPUI's shaders have
  already been built and cached. A clean build may still require `GPUI_FXC_PATH`;
  cached success does not prove global SDK discovery is fixed.
- Sandboxed Cargo registry access failed with a Windows Schannel credential
  error; approved external network access resolved dependency metadata and the
  lockfile. This is an execution-environment restriction, not yet a project bug.
- `create-gpui-app 0.1.5` generates a moving Zed Git dependency. The scaffold
  was deliberately changed to the published `gpui 0.2.2` API and lockfile.
- Rustc overflowed its stack when the native GPUI binary contained unit tests.
  Pure scan-state tests now live in the package library and the binary declares
  `test = false`; all-target checking and clippy still compile the binary.
- Native interaction and visual behavior remain user-observed phase gates; a
  successful static build does not certify text hit testing or scroll behavior.
- SSH hosts currently require a POSIX `sh` environment and GNU-compatible
  `find`, `grep`, `stat`, `wc`, and `head`. Native Windows PowerShell-only SSH
  servers and SFTP transport are not implemented.
- Authentication must already work non-interactively through OpenSSH config,
  keys, or an agent. DevHub does not collect passwords or key passphrases. New
  host keys should be reviewed and accepted in a terminal before using the app.
- Remote ignore files are now interpreted over SSH via `REMOTE_IGNORE_FUNCTION`
  (`git check-ignore` parent-directory walk). The local `ignore` crate and
  remote shell function may diverge in edge cases.
- A "Stop" button in the titlebar cancels all tracked scan, source-picker,
  tree, file, README, and search operations.
  Individual per-operation cancellation (e.g. stop tree but not scan) is not
  implemented.
- Zed must be installed and its `zed` command available, or installed in a known
  Windows location, for the primary launch action. The current machine resolves
  `zed` to `[REDACTED]\Zed.exe`.
- Windows `OPEN IN…` can pass an SSH URI to a selected application, but the
  selected application must understand `ssh://`; this is not generic remote
  filesystem mounting.
- README preview deliberately does not load images. Markdown and HTML images show
  explicit alt-text placeholders, image syntax inside code is preserved, and
  linked-image destinations remain clickable. Offline local/SSH asset loading is
  not implemented.

## Closure

The DevHub-GPUI parity project closed on 2026-07-03. The approved product
workflow is implemented, the full local validation gate passes with 60 tests,
native Windows behavior has been user-verified, and v1.1.0 is published with
Windows, Linux, and macOS archives plus SHA-256 checksums. The live website
presents GPUI as the primary DevHub and keeps the egui implementation as the
legacy reference.

This ADR is now the historical engineering record rather than an active task
queue. The table at the beginning contains optional enhancements and platform
limits; none blocks closure. If development resumes, record a new dated decision
and validation baseline instead of silently reopening a completed phase.

---

## V2 Product ADR

| Field        | Value                             |
| ------------ | --------------------------------- |
| Status       | Accepted implementation record    |
| Started      | 2026-07-19                        |
| Consolidated | 2026-07-20                        |
| Baseline     | v1.1.0 parity release             |
| Direction    | Local-first developer project hub |

### V2 decision

V2 extends DevHub from a project catalog into the small workspace a developer
uses before opening an editor. It remains a project hub, not a second editor.
The product has five recurring jobs:

1. Find and select the right local or SSH project.
2. Show only enough project context to decide what to do next.
3. Read the README and source files without leaving the hub.
4. Complete the everyday Git loop for the selected repository.
5. Run an explicit project-rooted terminal when the developer asks for one.

Local work is the default. Reading a local repository, inspecting local Git
state, browsing cached projects, and opening a local editor require no network.
SSH access and Git remote actions are explicit. DevHub does not fetch images,
contact an origin, or start a remote session merely because a project was
selected.

Every feature must continue to earn its place through a repeated developer
task. Data availability alone is not a reason to display another label, badge,
pane, or settings control.

### V2 application shell

V2 adopts a compact contextual shell:

```text
title bar: project | branch | open actions | explicit operation controls
workspace: optional task context | main content
bottom flyout: selected-project terminal, only when requested
bottom strip: projects | overview | files | search | Git | history | commands
```

- The title-bar project control is the fast switcher.
- The resizable project catalog is transient and closes after selection.
- Overview, Files, Search, Git, and History are the durable workspace modes.
- Files, Search, and Git own independent context-pane widths.
- Navigation stays in the compact bottom strip rather than a persistent
  editor-style activity rail.
- Launchers are centered, compact, keyboard navigable, dismiss on outside
  click or `Escape`, and do not scroll the workspace behind them.
- Focused overlays own `Enter`; application-level key handling cannot turn a
  palette selection into an unrelated project launch.
- The last selected project is restored by path and source host. A missing
  project produces an actionable missing state instead of silently selecting a
  different project.

### V2 information and visual rules

- Persistent information must help the current task.
- Normal project surfaces omit raw markers, scan internals, redundant labels,
  and explanatory microcopy.
- Common actions use compact familiar icons with tooltips.
- Project, branch, command, theme, and editor launchers share the same geometry
  and interaction behavior; widths follow their content.
- Code, Markdown, Git diffs, history, settings, and terminal surfaces use the
  same spacing, typography, focus, selection, and status language.
- Loading and failure stay in the workspace that owns the operation. The
  title-bar stop control is reserved for an actual catalog scan.
- Dark and light themes are selected and previewed from the command palette.
  Settings contains source configuration, not duplicate appearance controls.

### V2 Markdown decision

The existing parser-backed Markdown component remains the preview baseline.
Raw mode uses the shared read-only source viewer. README images and media are
not loaded; they render as offline placeholders or explicit links. This keeps
project reading deterministic and local by default without embedding a browser
or introducing an asset-fetch policy.

Markdown work is judged by representative fixtures: headings, nested and task
lists, tables, fenced code, long tokens, Unicode, links, raw HTML degradation,
and large documents. Raw source remains the fallback whenever preview fidelity
is incomplete.

### V2 Git decision

V2 uses the installed Git CLI so commands honor the developer's credentials,
signing setup, attributes, remotes, hooks, and repository formats. Commands are
passed as argument arrays with bounded output, cancellation, and typed results.

The selected-repository workflow includes:

- Status for staged, unstaged, untracked, renamed, deleted, and conflicted files
- Per-file and repository-wide addition/deletion totals
- Flat and tree presentations of the change set
- Per-file and all-file stage/unstage actions
- Explicit discard confirmation
- Semantic unified diffs with old/new gutters and full-row change emphasis
- A compact multiline commit composer
- Existing-branch listing, filtering, marking, and switching
- Explicit Fetch and Push, including the common set-upstream path
- Automatic local status refresh and active-view SSH status polling
- Commit history loaded in 25-entry pages as scrolling reaches the end
- Commit topology and ref labels, selection, compact details, changed-file
  tree, copy hash, GitHub link when derivable, and per-file commit diff

Automatic status work never contacts the Git origin. Fetch and Push remain
explicit commands. Network failure reports a concise status and does not block
repository-local work. DevHub does not attempt to expose every Git command;
sustained conflict editing and uncommon repository administration remain editor
or terminal tasks.

### V2 terminal decision

The terminal is an explicit, resizable bottom flyout rooted in the selected
project. It uses the user's detected shell locally and an interactive SSH shell
for remote projects. Its PTY, ANSI state, Nerd Font rendering, bounded
scrollback, input, resize, and process lifecycle belong to the terminal module.

Collapsing the terminal preserves its session. Switching projects ends the
unpinned session so the next terminal command starts in the newly selected
project. Pinning preserves the owning session and keeps its host/path visible.
Closing or dropping a session terminates and reaps its child. No terminal is
started by project selection, Git inspection, or an editor launch.

### V2 editor handoff decision

Zed remains the primary handoff and supports both local paths and SSH projects.
`Open in` is a separate detected-editor launcher and does not repeat Zed.

Editors are discovered from operating-system application metadata rather than
a hardcoded product/path table. Code-family remote capability comes from the
editor's product metadata. JetBrains products are matched to project language
families using their declared modules, including mixed-language repositories.
Unsupported local-only editors are omitted for SSH projects. Editor processes
receive null stdio and, on Windows, `CREATE_NO_WINDOW`, so opening an editor does
not leave a console window attached for the IDE session.

### V2 safety boundaries

V2 does not include:

- Automatic Fetch, Push, Pull, or repository mutation
- Catalog-wide background Git activity
- Remote image or media fetching
- A full source editor or merge-conflict editor
- Branch creation merely to increase Git feature count
- Cloud accounts, telemetry, or hosted project state
- UI labels, controls, or panels without a demonstrated workflow

### V2 validation gate

The implementation is release-ready only when all static checks pass and the
native application is reviewed at compact and wide sizes in dark and light
themes:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

Native review covers keyboard focus and dismissal, project switching, pane
resizing, README/raw reading, file and search navigation, local and SSH Git,
history pagination and commit details, terminal lifecycle, local and remote
editor handoff, long content, and every loading/empty/error/cancelled state.

### V2 superseded historical decisions

- The generic Windows `SHOpenWithDialog` fallback is superseded by the
  metadata-driven in-app editor launcher.
- The manual key-event search field is superseded by normal IME-capable inputs.
- The permanent list/details layout is superseded by the transient catalog and
  contextual workspaces.
- The earlier v1.2 working plan is consolidated into this V2 record. Future
  work must satisfy the feature-admission criteria below and must not silently
  reopen V2 scope.

## v2 Decision Log

| Date       | Decision                                                     | Reason                                                                                             |
| ---------- | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- |
| 2026-07-19 | Keep DevHub local-first and Zed-first                        | Preserve the focused project-hub purpose                                                           |
| 2026-07-19 | Make Overview the default selected-project workspace         | The first screen should explain the selected project without duplicating catalog navigation        |
| 2026-07-19 | Keep the project catalog transient                           | Project browsing must not permanently reduce workspace width                                       |
| 2026-07-19 | Store project selection by stable catalog identity           | Filtering or sorting must never reinterpret the selected row as another project                    |
| 2026-07-19 | Use compact bottom navigation instead of a vertical rail     | Persistent chrome must stay quiet and preserve the project's own identity                          |
| 2026-07-19 | Use content-width title pills for project and Git context    | Context should remain scannable without reserving empty title-bar space                            |
| 2026-07-19 | Preview themes before committing them                        | Appearance selection should be immediate, reversible, and keyboard-first                           |
| 2026-07-19 | Keep appearance out of Settings                              | One command-palette chooser avoids duplicated state and configuration noise                        |
| 2026-07-19 | Scope Enter to the focused transient surface                 | A palette action must never fall through into a project launch                                     |
| 2026-07-19 | Require keyboard-first navigation and a command palette      | Developer speed and discoverability must share one command system                                  |
| 2026-07-19 | Build a parser-backed Markdown reader                        | README quality directly affects project understanding                                              |
| 2026-07-19 | Make daily Git workflows fully functional                    | Project understanding and repository action belong together in a developer hub                     |
| 2026-07-19 | Use system Git before adding a Git library                   | Preserve user configuration and avoid unnecessary dependency weight                                |
| 2026-07-19 | Load detailed Git data only for the selected project         | Useful context should not become catalog-wide process churn                                        |
| 2026-07-19 | Require every persistent UI element to justify its presence  | Less information produces a clearer and faster project decision                                    |
| 2026-07-20 | Refresh only the selected repository automatically           | Local edits and external staging should appear without catalog-wide Git work                       |
| 2026-07-20 | Poll SSH status only while Git is visible                    | Remote repositories need near-live state without a remote resident agent                           |
| 2026-07-20 | Keep Fetch and Push explicit                                 | Remote synchronization must never weaken the local-by-default contract                             |
| 2026-07-20 | Keep branch interaction focused on switching                 | Existing context switching is the project-hub workflow                                             |
| 2026-07-20 | Restore the last project or show an actionable missing state | Startup must preserve user context without silently selecting the wrong project                    |
| 2026-07-20 | Keep Git file actions on the change row and context menu     | Frequent actions stay direct while destructive work remains explicit                               |
| 2026-07-20 | Render syntax-aware unified diffs with line statistics       | Developers can inspect staged and unstaged file changes directly inside DevHub                     |
| 2026-07-20 | Add a bounded first commit-history slice                     | Developers can browse subjects, authors, dates, hashes, and basic file statistics without checkout |
| 2026-07-20 | Integrate a multiline commit box with a focused overlay      | Long messages get room without permanently enlarging the Changes pane                              |
| 2026-07-20 | Give the branch picker a content-specific narrow width       | A short branch list should not inherit command-palette dimensions                                  |
| 2026-07-20 | Accept a selected-project bottom terminal                    | Short build, test, and Git commands belong with the explicitly selected project                    |

## v2 Extension Development

| Field     | Value                                                          |
| --------- | -------------------------------------------------------------- |
| Status    | Accepted working plan                                          |
| Date      | 2026-07-20                                                     |
| Baseline  | Consolidated V2 product record in `ADR.md`                     |
| Target    | v3                                                             |
| Direction | A trustworthy project home with read-only project intelligence |

## Decision

V3 will make the existing project-hub workflow more dependable, more precise,
and easier to carry across supported platforms. It is not permission to add a
new collection of panels.

The product remains the place to answer five questions:

1. Which project should I open?
2. What state is it in?
3. Can I complete the small project, Git, or terminal task here?
4. Which editor should receive the project when sustained work begins?
5. How is this project put together?

V3 succeeds when these answers are fast and unsurprising across local and SSH
projects. A feature is not accepted because another developer tool has it. It
must shorten a repeated DevHub workflow and fit the existing shell without
adding persistent noise.

## Product Contract

DevHub remains:

- Local by default. Cached catalog, local files, README, local Git, and local
  editor handoff work without a network.
- Networked only on command. SSH browsing, remote terminals, Fetch, Push, and
  external links are explicit actions.
- Project intelligence is exposed read-only through an explicit MCP server. A
  headless stdio binary serves local editors without the application running;
  an in-app HTTP server on `127.0.0.1` starts only from the status-strip toggle,
  requires a generated bearer token, and serves the same read-only tools.
  Tailscale Serve owns tailnet HTTPS routing. DevHub stores no provider key,
  fetches no model catalog, and implements no inference client. When the server
  is off, no listener exists.
- Project-centered. The selected project owns Files, Search, Git, History,
  terminal, and editor context.
- Keyboard-first. Every frequent action is reachable through focused
  navigation or the command palette, with pointer and keyboard paths invoking
  the same command.
- Read-oriented. DevHub may inspect and manage a repository, but it does not
  become a general source editor.
- Quiet. Persistent UI is limited to current context, navigation, and
  actionable status.

Zed remains the primary handoff. The detected `Open in` launcher is a useful
secondary path, not a replacement for stable editor-specific behavior.

## V2 Baseline

V3 starts from the current V2 implementation, including:

- Transient project catalog and title-bar project switcher
- Overview, Files, Search, Git, and History workspaces
- Command palette and live theme selection
- Parser-backed README preview and shared raw/source viewer
- Local and SSH discovery, reading, search, Git, and terminal routing
- Flat/tree Git changes, line totals, per-file actions, semantic diffs, commit,
  branch switching, Fetch, and Push
- Selected-repository automatic status refresh without automatic origin access
- Commit graph/history with 25-item scroll pagination, compact details,
  changed-file tree, and commit file diff
- One explicit selected-project terminal with PTY input, resize, scrollback,
  pinning, and lifecycle ownership
- Metadata-driven local/remote editor launcher with project-aware JetBrains
  filtering

These are baseline behavior, not V3 feature proposals. Regressions in them are
P0 defects.

## Feature Admission

A new V3 item must satisfy all of the following:

1. It solves an observed repeated task or a demonstrated reliability defect.
2. Its outcome can be stated without referring to internal architecture.
3. It has a clear local and SSH behavior, including when the network is absent.
4. Its loading, empty, error, cancellation, and process-exit states are known.
5. It fits an existing workspace, launcher, command, or contextual detail
   surface.
6. It does not add catalog-wide work or a permanent control when selected-task
   work is sufficient.
7. It has an executable or native acceptance gate.

Items failing this test stay out of the roadmap.

## P0: V2 Trust Boundary

P0 closes the gap between a feature existing and the feature being safe to use
through a full development day.

### P0-001: Process ownership

- Editor handoff never opens a console or terminal window.
- Editor processes receive no inherited stdin, stdout, or stderr.
- On Windows, editor handoff uses `CREATE_NO_WINDOW`.
- The DevHub terminal starts only through the terminal command.
- Project switching, pinning, collapse, close, child exit, SSH disconnect, and
  application exit follow the terminal ownership contract in `ADR.md`.
- Every cancelled managed process is terminated and reaped.

### P0-002: Project identity and stale state

- Selection remains attached to path plus source host across filtering,
  sorting, scanning, and startup restore.
- Missing remembered projects show one repair/remove state.
- Switching projects invalidates stale README, tree, file, search, Git,
  history, diff, and unpinned terminal results.
- Background completion from the previous project cannot replace current
  content.

### P0-003: Git safety

- Stage, unstage, discard, commit, switch, Fetch, and Push preserve selection
  and commit text on failure.
- Destructive actions identify the affected path and require confirmation.
- Automatic refresh never executes a network-class command.
- Credential, signing, hook, conflict, detached-HEAD, and missing-upstream
  failures remain concise and actionable.
- Arbitrary spaces, Unicode, tabs, renames, binary files, and unborn branches
  are covered by fixtures.

### P0-004: Release truth

- `README.md`, `ADR.md`, release notes, and UI text describe only validated
  behavior.
- No working-tree-only behavior is presented as a published release.
- The full static gate and native review pass before a version or release claim
  changes.

P0 blocks all V3 feature work when a data-loss risk, involuntary process,
incorrect project identity, or false release claim is open.

## P1: V3 Product Work

P1 improves the workflows already earning daily space in DevHub.

### P1-001: Project understanding

Overview keeps only information useful before opening the project:

- README preview or raw source
- Project path and source
- Current branch and concise dirty state
- Latest commit subject/time when already loaded
- One primary Zed handoff and the secondary detected-editor launcher

Project type detection and editor compatibility use the complete discovered
marker set, including mixed-language repositories. Unknown types remain
unknown; weak evidence must not produce confident labels or irrelevant
JetBrains entries.

### P1-002: Markdown fidelity without media loading

Improve the existing parser-backed reader through fixtures and presentation,
not through a browser or a second rendering stack.

Required coverage:

- Nested ordered/unordered/task lists
- Tables with bounded horizontal overflow
- Fenced code and language highlighting
- Block quotes, links, strikethrough, Unicode, and long tokens
- Predictable degradation for raw HTML
- Large-document scrolling and selection
- Raw/preview focus and scroll behavior

Images and media remain offline placeholders or explicit links. V3 does not
load local or remote README media automatically.

### P1-003: Git workflow fidelity

Polish the existing Git workflow rather than expanding into every Git command:

- External local changes appear after the existing debounce.
- SSH status updates only while Git is visible and never contacts origin.
- Diff selection survives refresh when the same file still exists.
- Conflict, binary, rename, delete, long-line, and large-diff states remain
  readable and bounded.
- History graph lanes and ref labels remain correct for merges and pagination.
- Commit details, file tree, and file diff remain available without replacing
  or losing the history list.
- The commit composer remains compact while accepting at least two visible
  lines and preserving the complete message.

### P1-004: Terminal fidelity

- Default shell detection is platform-native and never relies on Windows
  `COMSPEC` as the preferred shell.
- Local sessions start in the exact selected project directory.
- SSH sessions start in the exact validated remote project directory.
- Resize updates both rendered cells and the underlying PTY.
- Clear, scrollback, Nerd Font glyphs, ANSI color, paste, control keys,
  navigation keys, and focus containment pass native review.
- An unpinned session survives collapse but ends on project change. A pinned
  session keeps its visible owner until explicitly closed.

### P1-005: Editor handoff fidelity

- Discovery uses operating-system application metadata and committed parser
  logic, never machine-specific editor paths.
- Zed is excluded from `Open in` because it already has the primary action.
- General-purpose editors may serve all detected project types.
- JetBrains products appear only for compatible project language families.
- Remote entries appear only when product metadata and transport support prove
  an SSH launch path.
- Launch failure stays in DevHub as concise status and never falls back to an
  arbitrary Documents folder or restricted blank project.

### P1-006: Cross-platform behavior

Windows, macOS, and Linux use the same product contract with platform-specific
process and application discovery behind narrow boundaries.

Native acceptance covers:

- Local editor discovery and launch
- Remote-capable editor filtering and launch
- Default shell selection and terminal input
- Window controls and compact/wide layout
- Local and SSH Git process behavior
- Configuration and cache paths

Unsupported platform behavior must be explicit. Silent fallback to a different
folder, shell, editor, or network action is a defect.

### P1-007: Measured responsiveness

Measure before adding caches, indexes, workers, or virtualization layers.

The acceptance catalog contains 100, 500, and 1,000 projects. Measure startup
from cache, project filtering, picker navigation, switching, README display,
Git refresh, and History pagination. Optimization is accepted only with a
reproducible before/after result and no stale-state regression.

### P1-008: Read-only project intelligence over MCP, and pre-handoff todos

Status: complete in v2.1.1. Further MCP work requires a demonstrated workflow
or security defect.

V2.1 replaces the V2 Ask Project panel and its OpenCode provider with a
read-only Model Context Protocol surface plus a lightweight per-project todo
panel. DevHub implements no chat interface, no inference client, no provider
key storage, and no model catalog. Clients bring their own models and agents;
DevHub supplies bounded project evidence.

Todo panel:

- The right-side panel slot formerly used by Ask Project becomes a
  per-project todo list: pre-handoff brain context for the user.
- Items are stored in a versioned `todos.toml` in the platform application
  data directory, keyed by project path and source host. Nothing is written
  into repositories; SSH projects need no remote writes.
- V1 scope: item text, done state, insertion order; add, toggle, delete.
  Completed items sink and render struck through.
- The panel replaces the Ask panel slot and width state, toggles with
  `ctrl-shift-t`, is closed by default, and is not a separate workspace.

MCP server:

- The same read-only tools are served in two shapes. A headless `devhub-mcp`
  stdio binary links devhub-core only, loads the same configuration, cache,
  and todos, and is spawned by MCP clients such as Zed. An in-app HTTP server
  binds `127.0.0.1`, starts and stops from a status-strip toggle, accepts
  reverse-proxy Host authorities, and shares the tool layer. Tailnet routing is
  delegated to Tailscale Serve; DevHub has no overlay-network awareness. HTTP
  uses stateless Streamable HTTP with JSON responses because these tools retain
  no client state and send no server-initiated messages; no long-lived SSE
  session is held through the reverse proxy.
- HTTP always requires `Authorization: Bearer <token>`. DevHub generates a
  256-bit token before the first listener starts and exposes masked copy and
  regeneration controls in Settings. Tokens never enter activity logs.
- All tools are read-only and bounded, and reuse the existing local and SSH
  tree, read, search, and Git paths with cancellation. Tool descriptions mark
  live SSH reads as network round-trips. `project_overview` includes one live
  bounded Git state call.
- Tool results carry structured path and line references plus the content
  itself; clients and editors own presentation. No deep-link or in-app
  citation navigation is implemented.
- Catalog answers come from the on-disk cache and stamp `catalog_as_of`;
  project content answers are computed live. A headless stdio session is
  therefore as capable as the running application, with catalog membership as
  fresh as the last scan.
- Every tool call appends to a bounded JSONL activity log in the application
  data directory. The status-strip indicator shows server state and opens a
  recent-activity overlay. Both server shapes write the log, so stdio
  activity is visible in the next application session.
- V1 tool inventory: `list_projects`, `project_overview`, `list_tree`,
  `read_file`, `search_content`, `git_status`, `git_diff`, `git_log`, and
  `list_todos` (read-only). Write tools are uncommitted P2 candidates.

Architecture diagrams (the GitDiagram port) are deferred, not cancelled; the
chat panel that hosted them no longer exists.

## P2: Evidence-Required Candidates

These are not committed features:

- Git worktree creation or management
- Multiple simultaneous terminal sessions per project
- Persistent full-repository content indexing
- Arbitrary commit snapshot browsing beyond the selected commit details
- User-defined editor executable mappings when metadata discovery fails
- More project metadata, dashboard summaries, or status badges

A P2 item moves to P1 only after normal use demonstrates repeated friction and
the smallest useful interaction is agreed first.

## P3: Out of scope

- A general-purpose code editor or merge editor
- Automatic Fetch, Pull, Push, cloning, or repository mutation
- Catalog-wide Git polling or background network activity
- Automatic README image, video, or remote asset loading
- Cloud project catalogs, DevHub accounts, project synchronization, or telemetry
- AI inference providers, API-key storage, model catalogs, chat interfaces,
  local model runtimes, or a DevHub-hosted inference service
- Autonomous agents, repository edits, model-triggered terminal commands,
  model-triggered Git mutation, or MCP write tools
- Plugin ecosystems or extension marketplaces
- Reimplementing the Git object database or credential system
- Adding UI simply to match Zed, VS Code, or a JetBrains product

These items require a new product decision, not opportunistic implementation.

## Milestones

### Milestone 0: V2 audit

- Reconcile implementation, `ADR.md`, README, and release state.
- Run the full static validation gate.
- Record native gaps without converting them into speculative features.

Gate: the baseline is truthful and every open P0 defect has an owner and a
reproduction.

### Milestone 1: Lifecycle and identity

- Complete P0-001 through P0-003.
- Test editor launch, terminal ownership, project switching, cancellation, and
  stale async completion locally and over SSH.

Gate: no action starts an unintended terminal/process, no old project result
appears in the new project, and Git failures preserve user state.

### Milestone 2: Reading and repository fidelity

- Complete P1-001 through P1-004.
- Review compact and wide layouts in every theme family.
- Exercise representative Markdown and Git fixtures.

Gate: project understanding, README reading, Git, History, and terminal work
remain coherent through long content and failure states.

### Milestone 3: Handoff and platform review

- Complete P1-005 and P1-006.
- Validate installed editor discovery without hardcoded local paths.
- Validate local and remote launch behavior on supported release platforms.

Gate: the same command opens the intended project in an eligible editor without
an extra console, wrong folder, or silent fallback.

### Milestone 4: Todo panel and MCP intelligence

Status: complete in v2.1.1.

- Remove the V2 Ask Project panel, OpenCode provider, credential storage, and
  their dependencies.
- Implement the versioned per-project todo store and the side panel.
- Implement the read-only tool layer over existing local/SSH paths.
- Implement the `devhub-mcp` stdio binary and the in-app HTTP server with
  status-strip toggle, required bearer token, and activity log.

Gate: tools answer from cache plus live bounded reads, every call is
read-only and logged, server-off means no listener, SSH tools behave under
latency and cancellation, and no inference or credential code remains.

### Milestone 5: Release closure and maintenance

- Measure P1-007 only after reproducible latency; P1-008 is complete.
- Run static, native, clean-install, and release-artifact validation.
- Update public documentation only after evidence exists.

Gate: no open P0/P1 item, no unexplained UI stall, no unintended network or
child process, and no release claim unsupported by validation.

## Next action

Close v2.1.4 with the full static gate and a native KDE Wayland Alt+Tab check.
After release, continue defect-only maintenance. P1 and P2 items remain
trigger-only; a new feature must satisfy Feature Admission with observed
evidence before implementation.

## Validation

Static gate:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

Native matrix:

- Compact and wide windows
- Every dark/light theme choice and system appearance
- Keyboard-only project, command, theme, branch, editor, and history launchers
- Local and SSH projects
- Clean, dirty, detached, unborn, conflict, and unavailable Git repositories
- Terminal open, input, clear, resize, collapse, pin, project switch, exit, and
  disconnect
- Editor discovery and launch with no console window
- Long paths, project names, branches, commit subjects, code lines, and errors
- Loading, empty, partial, cancelled, success, and failure states
- Todo add, toggle, delete, persistence across restarts, project switching,
  SSH projects, done-item ordering, and empty state
- `devhub-mcp` stdio session with a real client against local and SSH
  projects; tool bounding, binary refusal, cancellation, and cold-cache
  behavior
- HTTP server toggle on/off, `127.0.0.1` binding, tailnet Host-header routing,
  no listener when off, bearer-token rejection and acceptance, token generation,
  activity-log content from both
  server shapes, and tailnet exposure through `tailscale serve`
- Native release packages expose both executable identities with matching
  Windows PE icons, a branded Linux AppImage with XDG metadata and `.DirIcon`,
  and validated macOS app bundles and ICNS resources

## Decision Log

| Date       | Decision                                                 | Reason                                                                                                          |
| ---------- | -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| 2026-07-20 | Treat V2 as the v3 baseline, not an open feature backlog | Prevent completed work from being repeatedly redesigned                                                         |
| 2026-07-20 | Make process ownership and project identity P0           | Involuntary terminals and stale cross-project state break trust                                                 |
| 2026-07-20 | Keep network work explicit                               | Local-first describes default behavior, not the absence of SSH or Git remotes                                   |
| 2026-07-20 | Keep README media offline                                | Predictable reading matters more than automatic media loading                                                   |
| 2026-07-20 | Improve existing Git depth before adding commands        | The daily repository loop has more value than feature-count parity                                              |
| 2026-07-20 | Detect editors from platform/product metadata            | Machine-specific paths and generic native pickers launch the wrong targets                                      |
| 2026-07-20 | Filter specialized IDEs by project evidence              | An editor entry earns its place only when it can serve the selected project                                     |
| 2026-07-20 | Require measured evidence for performance architecture   | Catalog size alone does not justify indexing or background complexity                                           |
| 2026-07-20 | Keep P2 explicitly uncommitted                           | Candidate ideas must not silently become roadmap promises                                                       |
| 2026-07-20 | Add read-only natural-language project intelligence      | Project understanding is the most valuable missing hub workflow                                                 |
| 2026-07-20 | Use only OpenCode Zen and Go for V3 inference            | One provider and one key keep setup and implementation bounded                                                  |
| 2026-07-20 | Port GitDiagram's validated generation shape locally     | Diagrams should understand local and SSH projects without embedding a web app                                   |
| 2026-07-20 | Persist bounded project context locally                  | Reopening an unchanged project should not repeat repository analysis                                            |
| 2026-07-21 | Replace Ask Project with read-only MCP tools             | The chat harness was the unmaintainable part; retrieval already works and external agents orchestrate it better |
| 2026-07-21 | Delete the OpenCode provider and credential storage      | With MCP, clients bring their own models; DevHub stores no keys                                                 |
| 2026-07-21 | Serve tools as a stdio binary plus toggle-explicit in-app HTTP | stdio works with the app closed; the HTTP listener supports LAN and reverse-proxied tailnet access             |
| 2026-07-21 | Repurpose the side panel as a per-project todo list      | Pre-handoff context earns the slot the chat panel vacates                                                       |
| 2026-07-21 | Defer architecture diagrams                              | Diagram generation lost its host surface when the panel was removed                                             |
| 2026-07-21 | Replace MCP status indicator with an SVG icon button     | Matches other status-bar buttons and adds tooltip for left/right click actions                                  |
| 2026-07-21 | Use auto-grow multi-line input for the todo text field   | Wraps long text and grows vertically instead of overflowing horizontally                                        |
| 2026-07-21 | Submit todo on Shift+Enter in multi-line input           | Enter inserts newline; Shift+Enter adds the todo item, consistent with textarea convention                      |
| 2026-07-22 | Keep stdio as a small companion and HTTP inside DevHub   | Editors own stdio pipes; the explicit in-app toggle owns the tailnet HTTP lifetime                              |
| 2026-07-22 | Bind HTTP to localhost and require generated auth        | Tailscale Serve supplies HTTPS without exposing the backend directly to LAN peers                               |
| 2026-07-22 | Move to evidence-triggered maintenance after v2.1.1      | No remaining candidate feature earns permanent product surface without observed repeated friction               |
| 2026-07-22 | Package both executable identities with native icons     | PE resources, XDG metadata, and app bundles provide OS integration without adding a packaging framework          |
| 2026-07-22 | Replace raw archives with native release packages        | A per-user installer, one AppImage, and one DMG make the brand visible without merging the two runtime contracts |
| 2026-07-22 | Validate MCP paths independently of the server OS        | Linux servers must reject Windows absolute and traversal syntax before routing a request to a Windows SSH host   |
| 2026-07-22 | Require direct HTTP stress evidence for concurrency      | Client-side serialization should not trigger an unproven server architecture change                              |
