# DevHub 2.1.1

This maintenance update hardens project identity, SSH platform behavior, process ownership, and MCP exposure without expanding DevHub's product scope.

## Highlights

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

**Option A: Standalone (stdio)**: build the binary with `cargo build --release -p devhub-mcp` and add it as a local MCP server in your editor:

```json
{
  "devhub-mcp": {
    "enabled": true,
    "remote": false,
    "command": "/path/to/executable",
    "args": [],
    "timeout": 30
  }
}
```

**Option B: DevHub HTTP (Streamable HTTP)**: toggle the MCP switch inside the DevHub desktop app, then point your editor to the running server:

```json
{
  "devhub-mcp": {
    "type": "remote",
    "url": "http://127.0.0.1:47821/mcp",
    "headers": {
      "Authorization": "Bearer <token-from-devhub-settings>"
    }
  }
}
```

DevHub generates a 256-bit HTTP token on first start. Copy or regenerate it from Settings. For tailnet access, run `tailscale serve --bg http://127.0.0.1:47821`, use the resulting HTTPS URL ending in `/mcp`, and send the same bearer header. Reverse-proxy Host headers are accepted; direct LAN binding is intentionally disabled.

### Todo List

Lightweight per-project todo tracking accessible via `devhub-mcp list_todos` and the in-editor todo panel.

## Artifacts

- Windows x64
- Linux x64
- macOS Apple Silicon

Every archive contains both executable identities, `README.md`, `RELEASE.md`, and `LICENSE`. Platform integration is deliberately native and small:

- Windows embeds the DevHub icon in both `.exe` files.
- Linux includes XDG desktop metadata and named SVG icons for both executables. The MCP entry is `NoDisplay` because clients launch it over stdio.
- macOS includes ad hoc signed `DevHub.app` and `DevHub-MCP.app` bundles with matching icons. Top-level command symlinks preserve direct terminal and MCP-client execution.

SHA-256 hashes are published in `checksums.txt`.

## Requirements

- Git for repository workflows.
- OpenSSH configuration and key-based authentication for SSH projects.
- A POSIX remote environment with standard command-line tools, or a Windows SSH host with Git for Windows installed.
- Linux requires a glibc-based system and a Vulkan-capable desktop stack.
- The macOS bundles are ad hoc signed for bundle integrity but are not Developer ID signed or notarized.

Before running an archive, compare its SHA-256 hash with `checksums.txt`.
