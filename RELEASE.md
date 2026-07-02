# DevHub GPUI 1.1.0

DevHub GPUI is a compact, Zed-first project hub for discovering and opening local and SSH-hosted projects. This release contains portable archives for Windows, Linux, and macOS.

## Downloads

Choose the archive matching your platform:

- `devhub-gpui-1.1.0-x86_64-pc-windows-msvc.zip` for Windows x64
- `devhub-gpui-1.1.0-x86_64-unknown-linux-gnu.tar.gz` for Linux x64
- `devhub-gpui-1.1.0-aarch64-apple-darwin.tar.gz` for macOS Apple Silicon

Each archive contains the executable, `README.md`, `RELEASE.md`, and `LICENSE`. The release also includes `checksums.txt` with SHA-256 hashes for every archive.

## Install

### Windows

1. Extract the ZIP to a stable location.
2. Run `devhub-gpui.exe`.
3. Windows release builds use the GUI subsystem and do not open a terminal
   window.

### Linux

1. Extract the archive.
2. Make the binary executable if required:

   ```sh
   chmod +x devhub-gpui
   ```

3. Run `./devhub-gpui`.

The Linux build requires a glibc-based x64 environment, a Vulkan-capable graphics stack, and normal X11 or Wayland desktop libraries.

### macOS

1. Extract the archive.
2. Run `./devhub-gpui` from the extracted directory.

The macOS artifact targets Apple Silicon. It is an unsigned portable binary, not a notarized `.app` bundle, so macOS may require explicit approval before first launch.

## First launch

The first launch opens source settings. Add at least one local folder or SSH source, choose its scan depth, save, and run a scan. DevHub GPUI never scans the current working directory as an implicit fallback. Configuration and cache data use the separate `devhub-gpui` platform identity. They do not overwrite the original DevHub application's data.

## Project parity

v1.1.0 completes the DevHub parity project. All core capabilities are ported:
scan cancellation with a "Stop" button, remote `.gitignore` semantics via
`git check-ignore`, pin/unpin and hide/archive projects with persistent config,
right-click context menus, themed input fields, and Windows SSH without console
window flash.

## Zed and SSH

Local projects open through the `zed` command. Remote projects use Zed's `ssh://user@host/path` target format.

SSH access must already work non-interactively through OpenSSH configuration, keys, or an agent. Remote project discovery requires `sh` and GNU-compatible `find`, `grep`, `stat`, `wc`, `head`, and `cat`. A PowerShell-only Windows SSH session is not compatible with the current remote discovery implementation.

## Verify downloads

Compare an archive's SHA-256 hash with `checksums.txt` before running it.

PowerShell:

```powershell
Get-FileHash .\devhub-gpui-1.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

Linux or macOS:

```sh
sha256sum devhub-gpui-1.1.0-x86_64-unknown-linux-gnu.tar.gz
```

## Upgrade and rollback

Portable releases do not modify themselves. To upgrade, extract the new version to a new directory and launch it against the existing `devhub-gpui` configuration.

To roll back, keep the previous extracted directory and launch its executable. Before downgrading, back up the `devhub-gpui` configuration directory. Older builds refuse to overwrite configuration files declaring a newer schema version.
