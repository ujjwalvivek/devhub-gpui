# DevHub GPUI

An independent GPUI-based successor experiment for DevHub. The existing egui
application remains the behavioral reference; this workspace does not modify it.

## Current capabilities

- Discover projects from configurable local roots and SSH hosts and cache the results.
- Add local and SSH roots through one custom source picker.
- Open source settings automatically on a true first launch; production startup
  never injects fixture projects.
- Open source settings automatically on a true first launch; production startup
  never injects fixture projects.
- Filter and select projects; open local or SSH projects directly in Zed, with
  Windows "Open with" as a best-effort fallback.
- Browse bounded local and remote project trees.
- Read bounded UTF-8 text files with line numbers, selection/copy, wrapping,
  language-aware highlighting, and search-hit positioning.
- Preview local or remote README Markdown with a raw/preview toggle and explicit
  offline placeholders for Markdown/HTML images.
- Search local or remote project content in the background.
- Persist configuration and the project cache under the distinct
  `devhub-gpui` platform identity.
- Configure separate local and per-SSH-host scan depths.
- Display project source, host, Git remote, markers, and last-modified metadata.
- Select the five legacy DevHub palettes in System, Dark, or Light appearance.
- Render a compact client-drawn Windows shell inspired by Zed's visual
  principles.

The document viewer is intentionally read-only. It uses `gpui-component`'s
rope-backed code editor with line numbers, wrapping, selection/copy, and
Tree-sitter highlighting. README previews use its selectable, virtualized
Markdown view.

## Requirements

- Rust 1.93.1 or a compatible current stable MSVC toolchain
- Visual Studio C++ build tools and a Windows 10/11 SDK
- `fxc.exe` from the Windows SDK for GPUI 0.2.2 shader compilation
- Windows OpenSSH (`ssh.exe`) and key/config-based non-interactive authentication
  for SSH sources
- A POSIX `sh` remote environment with GNU-compatible `find`, `grep`, `stat`,
  `wc`, and `head`
- Zed with its `zed` CLI available to open projects directly. Remote projects
  use Zed's supported `ssh://user@host/path` target format.

If GPUI cannot locate `fxc.exe`, set it for the active PowerShell session:

```powershell
$env:GPUI_FXC_PATH='C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\fxc.exe'
```

Keep this machine-specific path out of committed Cargo configuration.

## Run

```powershell
cargo run --release -p devhub-gpui
```

Scanning and other filesystem access occur only after an explicit user action.
Scan requires at least one configured local or SSH source; it never falls back
to scanning the process working directory.
Scan requires at least one configured local or SSH source; it never falls back
to scanning the process working directory.
Configuration is written under the platform configuration directory using the
`devhub-gpui` identity, so it cannot overwrite the egui DevHub configuration.
SSH uses `BatchMode=yes`, the user's existing OpenSSH configuration and keys,
strict host-key behavior from OpenSSH, connection deadlines, and bounded output.
README images are never fetched. Preview mode shows their alt text as an offline
placeholder; linked-image destinations remain ordinary clickable links.

## Validation

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```
