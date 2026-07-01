# DevHub GPUI

An independent GPUI-based successor experiment for DevHub. The existing egui
application remains the behavioral reference; this workspace does not modify it.

## Current capabilities

- Discover local projects from configurable scan roots and cache the results.
- Filter, select, and open projects with configured editor commands.
- Browse a bounded, gitignore-aware project tree.
- Read bounded UTF-8 text files with line numbers, selection/copy, wrapping,
  language-aware highlighting, and search-hit positioning.
- Preview README Markdown locally with a raw/preview toggle; remote images are
  deliberately omitted.
- Search local project content in the background.
- Persist configuration and the project cache under the distinct
  `devhub-gpui` platform identity.
- Render a compact client-drawn Windows shell inspired by Zed's visual
  principles.

The document viewer is intentionally read-only. It uses `gpui-component`'s
rope-backed code editor with line numbers, wrapping, selection/copy, and
Tree-sitter highlighting. README previews use its selectable, virtualized
Markdown view. Remote SSH workflows are deferred to Remediation Phase 3.

## Requirements

- Rust 1.93.1 or a compatible current stable MSVC toolchain
- Visual Studio C++ build tools and a Windows 10/11 SDK
- `fxc.exe` from the Windows SDK for GPUI 0.2.2 shader compilation

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
Configuration is written under the platform configuration directory using the
`devhub-gpui` identity, so it cannot overwrite the egui DevHub configuration.

## Validation

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

See [plan.md](plan.md) for the living architecture record, completed milestones,
known limitations, and the approved remediation phases.
