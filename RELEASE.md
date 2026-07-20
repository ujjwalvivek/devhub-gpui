# DevHub 2.0

DevHub 2 turns the project catalog into a local-first developer workspace while remaining deliberately smaller than an editor.

## Highlights

- A compact project-centered shell with Overview, Files, Search, Git, and History workspaces.
- Keyboard-first navigation, command palette, project switcher, and live theme selection.
- Parser-backed README preview and a shared read-only source viewer.
- Complete everyday Git flow: changes, semantic diffs, stage, unstage, discard, commit, branch switching, Fetch, Push, and automatic local status refresh.
- Paginated commit history with topology, refs, commit details, changed files, and per-file diffs.
- One project-rooted local or SSH terminal with persistent collapse state and explicit process ownership.
- Zed-first local and SSH handoff plus a detected-editor launcher with project-aware compatibility filtering.

Network access remains explicit. DevHub does not automatically contact Git remotes or load README media.

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
