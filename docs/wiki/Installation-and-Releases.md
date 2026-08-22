# Installation, runtime packages, and releases

sshxx is self-hosted by default. The supported topology always has a server
between viewers and the daemon; a browser or packaged client cannot connect
directly to `sshxx-daemon`.

## What must run

| Component             | Required | Placement and purpose                                                                                                                |
| --------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `sshxx-server`        | Yes      | Serves the Web client and coordinates the encrypted session. It may run on the terminal machine or a reachable server.               |
| `sshxx-daemon`        | Yes      | Connects outward to `sshxx-server`, persists the workspace, and bridges terminal/filesystem operations.                              |
| `sshxx-terminal-host` | Yes      | Runs beside the daemon and owns PTY/ConPTY processes independently of daemon and viewer restarts. The daemon discovers or starts it. |
| Web client            | Yes      | Built static files shipped with the runtime archive and served by `sshxx-server`; there is no separate Web installation.             |
| Tauri desktop client  | No       | Optional viewer. It connects to the same server and does not replace the server, daemon, or terminal host.                           |

For a single-machine workspace, all three runtime executables run locally. For a
remote self-hosted deployment, the server may run on a public or LAN host, while
the daemon and terminal host remain together on the machine whose shells and
files they expose.

The upstream sshx one-line installer only needed a local client because the
upstream project operated its public server. sshxx deliberately does not select
that service by default. A supported sshxx installation therefore includes its
own server.

## One-command local install

The primary distribution is a versioned GitHub Release runtime archive. The
installer verifies the archive against the release `SHA256SUMS`, installs it
under the current user's profile, and keeps stable command wrappers while
versions change.

Linux and macOS:

```shell
curl -fsSL https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.sh | sh -s -- --run
```

Windows PowerShell (x64):

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.ps1))) -Run
```

`--run`/`-Run` starts a server on `127.0.0.1:8051`, then runs the daemon in the
foreground. The daemon prints the session URL. Its workspace, encrypted SSH
profiles, history, and cache use the directory from which the command was run.
Stop the daemon with `Ctrl+C`; the installer also stops the temporary foreground
server.

To install without starting anything, omit `--run` or `-Run`. The resulting
commands are `sshxx-server`, `sshxx-daemon`, and `sshxx-terminal-host`. Review a
downloaded installer before executing it when required by your security policy.

The installer is intentionally not an npm package: the product contains native
Rust PTY/ConPTY executables, services, and static Web assets rather than a
JavaScript library. APT would cover only Debian-family systems. Package-manager
recipes may later wrap the same signed release artifacts, but GitHub Releases
remain the cross-platform source of truth.

## Release downloads

Each release contains:

- `sshxx-runtime-<version>-<target>.tar.gz` for Linux/macOS or `.zip` for
  Windows. Every runtime archive contains the three required executables, the
  built Web client, license, and READMEs.
- Optional Tauri desktop bundles for the supported desktop targets.
- `SHA256SUMS` covering every downloadable asset.
- GitHub artifact attestations for the release assets and checksum manifest.

Runtime targets are Linux x64/arm64, macOS Intel/Apple Silicon, and Windows x64.
Desktop bundles are produced for Linux x64, macOS Intel/Apple Silicon, and
Windows x64.

The current desktop artifacts are not production code-signed or notarized. macOS
uses ad-hoc signing and Windows may display SmartScreen warnings. Release
checksums and GitHub attestations provide build provenance and integrity checks,
but do not replace trusted platform code signing.

## Maintainer release flow

`.github/workflows/release.yaml` is tag-driven and uses this sequence:

1. Confirm the four module versions are identical and all normal validation has
   passed on `main`.
2. Create and push an annotated SemVer tag matching that version:

   ```shell
   git tag -a v0.7.0 -m "sshxx v0.7.0"
   git push origin v0.7.0
   ```

3. The workflow validates the tag, creates a draft Release, builds every runtime
   and desktop matrix entry, uploads assets, generates `SHA256SUMS`, and creates
   attestations.
4. Only after every required build succeeds does the workflow publish the
   Release and mark it latest. A failed run leaves the Release as a draft so it
   can be inspected or rerun without exposing an incomplete download page.

The example version must be replaced with the repository's current version.
Creating the tag is the explicit public-release action; ordinary pushes to
`main` never publish a Release.
