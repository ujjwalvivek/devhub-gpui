# DevHub-GPUI

A local-first project hub for local and SSH repositories, built in Rust with GPUI. DevHub covers the work around a project without trying to replace the editor.

## Features

- Discover, cache, filter, pin, and hide projects across local roots and SSH hosts.
- Read READMEs and source files, browse trees, and search project content.
- Inspect Git changes and diffs; stage, discard, commit, switch branches, fetch, push, and browse commit history.
- Run one project-rooted local or SSH terminal that survives panel collapse.
- Open projects directly in Zed or another compatible detected editor.
- Navigate common workflows from the keyboard and command palette.
- Expose bounded, read-only project intelligence through MCP.
- Keep per-project todos beside the project panel for quick context handoff.

Local files, cached projects, and repository-local Git work stay offline. SSH, Fetch, Push, and external links use the network only when requested. README media is not loaded automatically.

## Requirements

- A current stable Rust toolchain and the native build tools for your platform.
- OpenSSH configuration and key-based authentication for SSH projects.
- A POSIX remote environment with standard command-line tools, or a Windows SSH host with Git for Windows installed.

The in-app MCP server listens only on `127.0.0.1:47821` when enabled and always requires a bearer token. DevHub generates and saves a 256-bit token on first start; it can be copied or regenerated from Settings. Clients send it as `Authorization: Bearer <token>`. For tailnet access, run `tailscale serve --bg http://127.0.0.1:47821` and use the resulting HTTPS URL ending in `/mcp`.
HTTP tool calls use stateless Streamable HTTP JSON responses, so reverse proxies do not need to preserve a long-lived MCP session.

Windows builds of GPUI 0.2.2 require `fxc.exe`. Set `GPUI_FXC_PATH` when the Windows SDK compiler is not discovered automatically.

## Build

```powershell
cargo run --release -p devhub-gpui
```

Packaged builds are available from
[GitHub Releases](https://github.com/ujjwalvivek/devhub-gpui/releases).

Releases use one native package per platform:

- Windows: `DevHub-Setup-<version>-x64.exe` installs both icon-bearing
  executables for the current user.
- Linux: `DevHub-<version>-x86_64.AppImage` is the branded desktop artifact and
  contains both executables. Run it normally for DevHub or pass `--mcp-stdio`
  when configuring a local MCP client. Its desktop metadata and `.DirIcon`
  provide the DevHub icon to supporting launchers and file managers.
- macOS: `DevHub-<version>-arm64.dmg` contains branded `DevHub.app` and
  `DevHub MCP.app` bundles. Configure stdio clients with
  `/Applications/DevHub MCP.app/Contents/MacOS/devhub-mcp`.

Linux ELF and macOS Mach-O command files do not carry a Finder-style icon
resource themselves. The AppImage and `.app` bundles are therefore the branded
files users see; extracting and browsing their internal command binaries can
still show a generic executable icon.

## Validate

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

Architecture and product decisions are recorded in [ADR.md](ADR.md).
