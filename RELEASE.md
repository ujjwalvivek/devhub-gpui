# DevHub 2.0

This update focuses on removal of the prior ai-harness scaffolding in favor of a focused, practical tool.

## Highlights

### DevHub MCP Server (`devhub-mcp`)

A new read-only MCP server exposing your entire DevHub catalog as AI-editor tools. Runs locally at `127.0.0.1:47821` and provides 9 tools:

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
"devhub-mcp": {
      "enabled": true,
      "remote": false,
      "command": "/path/to/executable",
      "args": [],
      "timeout": 30
    },
```

**Option B: DevHub HTTP (Streamable HTTP)**: toggle the MCP switch inside the DevHub desktop app, then point your editor to the running server:

```json
"devhub-mcp": {
      "type": "remote",
      "url": "http://127.0.0.1:47821/mcp"
    }
  }
```

### Todo List

Lightweight per-project todo tracking accessible via `devhub-mcp list_todos` and the in-editor todo panel.

## Artifacts

- Windows x64
- Linux x64
- macOS Apple Silicon

Each archive contains the executable, `README.md`, `RELEASE.md`, and `LICENSE`. SHA-256 hashes are published in `checksums.txt`.

## Requirements

- Git for repository workflows.
- OpenSSH configuration and key-based authentication for SSH projects.
- A POSIX remote environment with standard command-line tools.
- Linux requires a glibc-based system and a Vulkan-capable desktop stack.
- The macOS build is an unsigned portable binary, not a notarized app bundle.

Before running an archive, compare its SHA-256 hash with `checksums.txt`.
