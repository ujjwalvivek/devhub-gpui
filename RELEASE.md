# DevHub 2.1.5

This update lets the HTTP MCP server run without the desktop window, so a service manager can keep it up across reboots.

## Highlights

### Headless HTTP MCP server

- `devhub-mcp --http` serves the same Streamable HTTP endpoint as the in-app switch, on `127.0.0.1` with the port and bearer token saved in DevHub's settings. If no token exists yet, it creates and saves one, as the app does on first start.
- The Linux AppImage forwards `--mcp-http` to it, next to the existing `--mcp-stdio`.
- The process exits with an error if the server stops, so systemd (or any supervisor) can restart it.
- Run either the headless server or the in-app switch, not both: they share the port, and whichever starts second reports the port as in use.

### DevHub MCP Server (`devhub-mcp`)

A read-only MCP server exposes your DevHub catalog as AI-editor tools. The in-app server listens on `127.0.0.1:47821`, requires bearer authentication, and provides 9 tools:

| Tool               | Description                                                |
| ------------------ | ---------------------------------------------------------- |
| `list_projects`    | Catalog of all projects with metadata                      |
| `project_overview` | README, layout, git state, recent commit, todos            |
| `list_tree`        | Gitignore-aware file tree (depth/hidden controls)          |
| `read_file`        | Read line ranges of text files with editor-navigable paths |
| `search_content`   | Full-text search across project files                      |
| `git_status`       | Live branch, upstream, ahead/behind, changed-file stats    |
| `git_diff`         | Unified diffs, optionally filtered by path                 |
| `git_log`          | Commit history pages                                       |
| `list_todos`       | Per-project DevHub todo items                              |

#### How to Use

There are two ways to connect DevHub to your editor:

**Option A: Standalone (stdio)**: use the installed MCP executable, or build it with `cargo build --release -p devhub-mcp`, and add it to Zed's `context_servers` settings:

```json
{
  "context_servers": {
    "devhub-mcp": {
      "command": "/path/to/devhub-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

Packaged stdio commands are:

- Windows: `C:\Users\<you>\AppData\Local\Programs\DevHub\devhub-mcp.exe`
- Linux: `/path/to/DevHub-2.1.5-x86_64.AppImage` with
  `"args": ["--mcp-stdio"]`
- macOS: `/Applications/DevHub MCP.app/Contents/MacOS/devhub-mcp`

**Option B: DevHub HTTP (Streamable HTTP)**: toggle the MCP switch inside the DevHub desktop app, then point your editor to the running server:

```json
{
  "context_servers": {
    "devhub-mcp": {
      "url": "http://127.0.0.1:47821/mcp",
      "headers": {
        "Authorization": "Bearer <token-from-devhub-settings>"
      }
    }
  }
}
```

**Option C: Headless HTTP (service)**: run the same server without the window, for example as a systemd user service:

```ini
[Unit]
Description=DevHub MCP over HTTP
After=network-online.target

[Service]
ExecStart=/path/to/DevHub-2.1.5-x86_64.AppImage --mcp-http
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
```

It uses the port and token from DevHub's settings, so clients configured for Option B keep working. Leave the in-app switch off while the service runs.

DevHub generates a 256-bit HTTP token on first start. Copy or regenerate it from Settings. For tailnet access, run `tailscale serve --bg http://127.0.0.1:47821`, use the resulting HTTPS URL ending in `/mcp`, and send the same bearer header. Reverse-proxy Host headers are accepted; direct LAN binding is intentionally disabled. HTTP tool calls use stateless Streamable HTTP JSON responses, avoiding long-lived MCP sessions through the proxy.

### Todo List

Lightweight per-project todo tracking accessible via `devhub-mcp list_todos` and the in-editor todo panel.

## Artifacts

- Windows x64: `DevHub-Setup-2.1.5-x64.exe`
- Linux x64: `DevHub-2.1.5-x86_64.AppImage`
- macOS Apple Silicon: `DevHub-2.1.5-arm64.dmg`

Every package contains both executable identities, `README.md`, `RELEASE.md`, and `LICENSE`. Platform integration remains native and small:

- The Windows installer installs both `.exe` files, each with the DevHub PE icon, plus Start menu and optional desktop shortcuts for the GUI.
- The Linux AppImage is the visible branded file and includes both ELF executables, XDG desktop metadata, and `.DirIcon`. Pass `--mcp-stdio` to dispatch directly to the companion server.
- The macOS disk image contains `DevHub.app` and `DevHub MCP.app`, both with matching ICNS resources. The latter exposes the stdio executable inside its normal app-bundle path.

Raw Linux ELF and macOS Mach-O files do not embed file-manager icon resources. Branding belongs to the AppImage, desktop entry, and `.app` bundle rather than to an extracted internal command file.

SHA-256 hashes are published in `checksums.txt`.

## Requirements

- Git for repository workflows.
- OpenSSH configuration and key-based authentication for SSH projects.
- A POSIX remote environment with standard command-line tools, or a Windows SSH host with Git for Windows installed.
- Linux requires a glibc-based system and a Vulkan-capable desktop stack.
- Release automation supports Authenticode signing on Windows and Developer ID signing plus notarization on macOS when maintainer credentials are configured. Otherwise Windows artifacts are unsigned and macOS bundles use ad hoc signing.

Maintainers configure Windows signing with the `WINDOWS_CERTIFICATE_BASE64` and
`WINDOWS_CERTIFICATE_PASSWORD` repository secrets. macOS uses
`MACOS_CERTIFICATE_BASE64`, `MACOS_CERTIFICATE_PASSWORD`, `APPLE_ID`,
`APPLE_TEAM_ID`, and `APPLE_APP_PASSWORD`.

Before running a package, compare its SHA-256 hash with `checksums.txt`.
