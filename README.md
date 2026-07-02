# DevHub GPUI

<p>
  <img src="https://echopoint.ujjwalvivek.com/svg/badges/stars?repo=devhub-gpui&logo=github&bg=111111&badgeColor=2b2b2b&textColor=e8e8e8&border=555555&borderWidth=2&rx=0&px=6&py=4" height="24" alt="stars">
  <img src="https://echopoint.ujjwalvivek.com/svg/badges/updated?repo=devhub-gpui&logo=github&bg=111111&badgeColor=2b2b2b&textColor=e8e8e8&border=555555&borderWidth=2&rx=0&px=6&py=4" height="24" alt="updated">
  <img src="https://echopoint.ujjwalvivek.com/svg/badges/health?repo=devhub-gpui&logo=github&bg=111111&badgeColor=2b2b2b&textColor=e8e8e8&border=555555&borderWidth=2&rx=0&px=6&py=4" height="24" alt="health">
  <img src="https://echopoint.ujjwalvivek.com/svg/badges/release?repo=devhub-gpui&logo=github&bg=111111&badgeColor=2b2b2b&textColor=e8e8e8&border=555555&borderWidth=2&rx=0&px=6&py=4" height="24" alt="releases">
</p>

A compact, Zed-first project hub built with GPUI. It began as an independent
successor experiment for the original egui DevHub and reached its approved
functional-parity scope in v1.1.0. The archived egui application remains the
historical behavioral reference.

## Current capabilities

- Discover projects from configurable local roots and SSH hosts and cache the results.
- Add local and SSH roots through one custom source picker.
- Open source settings automatically on a true first launch; production startup never injects fixture projects.
- Filter and select projects; open local or SSH projects directly in Zed, with Windows "Open with" as a best-effort fallback.
- Use the IME-capable project filter with Ctrl+F and virtualized project rows for large cached collections.
- Browse bounded local and remote project trees.
- Read bounded UTF-8 text files with line numbers, selection/copy, wrapping, language-aware highlighting, and search-hit positioning.
- Preview local or remote README Markdown with a raw/preview toggle and explicit offline placeholders for Markdown/HTML images.
- Search local or remote project content in the background.
- Persist configuration and the project cache under the distinct `devhub-gpui` platform identity.
- Version configuration independently from the cache: legacy unversioned files migrate atomically to schema version 1, while newer schemas are never overwritten by an older build.
- Configure separate local and per-SSH-host scan depths.
- Display project source, host, Git remote, markers, and last-modified metadata.
- Cancel active scans, source-picker requests, tree loads, file/README reads,
  and searches without allowing stale results to replace current state.
- Honor remote repository ignore rules through `git check-ignore` when Git is
  available on the SSH host.
- Pin important projects, hide archived projects, and manage both states through
  persistent configuration and project context menus.
- Select the five legacy DevHub palettes in System, Dark, or Light appearance.
- Render a compact client-drawn Windows shell inspired by Zed's visual principles.

The document viewer is intentionally read-only. It uses `gpui-component`'s rope-backed code editor with line numbers, wrapping, selection/copy, and Tree-sitter highlighting. README previews use its selectable, virtualized Markdown view.

## Requirements

- Rust 1.93.1 or a compatible current stable MSVC toolchain
- Visual Studio C++ build tools and a Windows 10/11 SDK
- `fxc.exe` from the Windows SDK for GPUI 0.2.2 shader compilation
- Windows OpenSSH (`ssh.exe`) and key/config-based non-interactive authentication for SSH sources
- A POSIX `sh` remote environment with GNU-compatible `find`, `grep`, `stat`,
  `wc`, `head`, and `cat`; remote Git is required for Git metadata and ignore-rule parity
- Zed with its `zed` CLI available to open projects directly. Remote projects use Zed's supported `ssh://user@host/path` target format.

If GPUI cannot locate `fxc.exe`, set it for the active PowerShell session:

```powershell
$env:GPUI_FXC_PATH='C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\fxc.exe'
```

Keep this machine-specific path out of committed Cargo configuration. For an isolated clean-install test that cannot touch normal application data, set an absolute `DEVHUB_GPUI_STATE_DIR` before starting the app.

## Run

```powershell
cargo run --release -p devhub-gpui
```

Scanning and other filesystem access occur only after an explicit user action. Scan requires at least one configured local or SSH source; it never falls back to scanning the process working directory. Configuration is written under the platform configuration directory using the `devhub-gpui` identity, so it cannot overwrite the egui DevHub configuration. Configuration files carry schema `version = 1`. Existing unversioned files are migrated on load using the same crash-safe replacement path as normal saves. Files from a future schema version are rejected and left unchanged. SSH uses `BatchMode=yes`, the user's existing OpenSSH configuration and keys, strict host-key behavior from OpenSSH, connection deadlines, and bounded output.

README images are never fetched. Preview mode shows their alt text as an offline placeholder; linked-image destinations remain ordinary clickable links.

## Validation

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

## Releases

Release builds use the GUI subsystem on Windows, so launching `devhub-gpui.exe` does not create a console window. Debug builds retain a console for diagnostics.

The GitHub Actions workflow builds and tests these standalone archives:

- Windows x64 (`x86_64-pc-windows-msvc`)
- Linux x64 (`x86_64-unknown-linux-gnu`)
- macOS Apple Silicon (`aarch64-apple-darwin`)
