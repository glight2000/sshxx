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

## Runtime and desktop client are separate packages

The **Runtime** is the required self-hosted backend bundle. Every Runtime
archive contains:

- `sshxx-server`, which serves the bundled Web client and coordinates sessions;
- `sshxx-daemon`, which owns persistent workspace data and bridges operations;
- `sshxx-terminal-host`, which owns PTY/ConPTY and shell/SSH processes;
- the built Web client in `build/`, plus the license and both READMEs.

The optional **desktop client** is a separate Tauri package named
`sshxx-client`. It is only a viewer: installing it does not install or replace
the server, daemon, terminal host, or Web client. Both the desktop client and a
normal browser connect to `sshxx-server`; neither connects directly to the
daemon.

## Choose an installation mode

| Mode                   | Intended user                                   | Startup and supervision                                                                 | Update and removal                                                                   |
| ---------------------- | ----------------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| Foreground quick trial | First-time users, evaluation, and development   | The user starts server and daemon in two terminals; no service definitions are changed. | Check and update with the installer; stop processes and remove the Runtime manually. |
| Managed installation   | Stable personal use and unattended self-hosting | systemd, launchd, or Windows Task Scheduler supervises three independent jobs.          | `sshxx-service` provides status, logs, update, and guarded uninstall.                |

Start with the foreground mode when deciding whether sshxx fits a workflow. Use
the managed mode when the workspace should return after login or system boot and
have a repeatable operational lifecycle.

## Mode 1: foreground quick trial

The primary distribution is a versioned GitHub Release Runtime archive. The
installer selects the matching platform archive, verifies it against the release
`SHA256SUMS`, stores each version separately, and updates stable command
wrappers to the selected version. It downloads and installs only; starting the
Runtime is the next explicit step.

### Step 1: download and install

Linux and macOS:

```shell
curl -fsSL https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.sh | sh
```

The default version directory is `~/.local/share/sshxx/versions/<version>` and
the command links are placed in `~/.local/bin`. If necessary, make them visible
in the current shell:

```shell
export PATH="$HOME/.local/bin:$PATH"
```

Windows PowerShell (x64):

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.ps1)))
```

Windows installs under `%LOCALAPPDATA%\sshxx`, adds its command-wrapper
directory to the user `PATH`, and exposes `sshxx-server`, `sshxx-daemon`, and
`sshxx-terminal-host` in PowerShell. Open a new PowerShell window if another
process does not observe the updated `PATH` immediately.

Confirm that the installed command wrappers resolve:

```shell
sshxx-server --version
sshxx-daemon --version
sshxx-terminal-host --version
```

The remote one-liners are provided for convenience. Download and inspect
`scripts/install.sh` or `scripts/install.ps1` before executing it when required
by local security policy.

### Step 2: start the Runtime

Start the server in the first terminal. This loopback-only setting is the safe
default for a local workspace:

```shell
sshxx-server --listen 127.0.0.1
```

Choose a durable directory for the workspace, encrypted SSH profiles, terminal
history, and cache. In a second Linux/macOS terminal:

```shell
mkdir -p "$HOME/sshxx-workspace"
cd "$HOME/sshxx-workspace"
sshxx-daemon --server http://127.0.0.1:8051
```

In a second Windows PowerShell terminal:

```powershell
New-Item -ItemType Directory -Force ~/sshxx-workspace | Out-Null
Set-Location ~/sshxx-workspace
sshxx-daemon --server http://127.0.0.1:8051
```

The daemon discovers or starts its matching terminal host automatically. Do not
start `sshxx-terminal-host` separately for this local flow. Keep the server and
daemon terminals running.

### Step 3: verify the Runtime

Use all of the following checks:

1. The server terminal remains running and `http://127.0.0.1:8051` returns the
   sshxx page. From a command line, use
   `curl -fsS http://127.0.0.1:8051/ >/dev/null` on Linux/macOS or
   `Invoke-WebRequest http://127.0.0.1:8051/ | Out-Null` on PowerShell.
2. The daemon prints a complete session URL containing `/s/...#...`. The URL
   fragment is a bearer secret; do not put it in logs, screenshots, or source
   control.
3. Open that session URL in a browser. The connection indicator becomes online
   and a new terminal can be opened.
4. From the same workspace directory, `sshxx-daemon terminal-host status`
   reports the host version and hosted terminals.

`Ctrl+C` stops each foreground server or daemon. Stopping the daemon does not
stop a separately running terminal host, so compatible terminal processes can
survive a daemon restart. Restarting the terminal host is a separate,
potentially destructive operation described below.

For a temporary shortcut, adding `--run` to the Unix installer or `-Run` to the
PowerShell installer performs installation and then runs the local server and
daemon in one foreground flow. The two-step procedure above is preferred for
normal operation because installation, process lifetime, data location, and
verification remain explicit.

The installer is intentionally not an npm package: the product contains native
Rust PTY/ConPTY executables, services, and static Web assets rather than a
JavaScript library. APT would cover only Debian-family systems. Package-manager
recipes may later wrap the same release artifacts, but GitHub Releases remain
the cross-platform source of truth.

### Check and update a foreground installation

Check without changing the installation:

```shell
curl -fsSL https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.sh | sh -s -- --check
```

Windows PowerShell:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.ps1))) -Check
```

To update, rerun the normal step 1 installation command. Stop the foreground
daemon and server, return to the same workspace directory, and start both again.
The Runtime archive contains the Web build, so the restarted server serves the
updated Web client automatically. The separate desktop client is not changed.

The currently running compatible terminal host deliberately remains alive. Use
`sshxx-daemon terminal-host status` from the workspace to inspect it. Only after
the terminal list is empty should `sshxx-daemon terminal-host restart` activate
the installed host version. The non-forced restart refuses active terminals;
`restart --force` explicitly disconnects every hosted shell and application.

To remove a foreground installation, first close the daemon and server. Run
`sshxx-daemon terminal-host stop` from the workspace; it refuses while terminals
are active. Then remove the command links and Runtime directory. The defaults
are `~/.local/bin/sshxx-*` and `~/.local/share/sshxx` on Unix, or
`%LOCALAPPDATA%\sshxx` and its user `PATH` entry on Windows. Workspace data is a
separate directory and should be deleted only when no longer needed.

## Mode 2: managed long-term installation

The managed option performs the same verified Runtime download, then registers
and starts three independent platform jobs. It never combines terminal-host
ownership with daemon supervision.

Linux and macOS user scope:

```shell
curl -fsSL https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.sh | sh -s -- --managed
```

This defaults to `~/sshxx-workspace` and `127.0.0.1:8051`. Override those values
when needed:

```shell
curl -fsSL https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.sh | sh -s -- \
  --managed --workspace /absolute/workspace/path \
  --listen 127.0.0.1 --port 8051
```

The default `--scope user` uses a systemd user unit on Linux or a LaunchAgent on
macOS and starts after that user logs in. For an unattended Linux/macOS host,
use `--scope system`; the installer registers systemd system units or macOS
LaunchDaemons under the current account and requests `sudo` only for privileged
service-manager operations.

Windows PowerShell registers three current-user Task Scheduler jobs that start
at login:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.ps1))) -Managed
```

Windows intentionally does not run terminal shells under `LocalSystem`. A true
pre-login Windows service needs an explicitly provisioned service account and is
not created by this installer. Task Scheduler provides the safe per-user managed
mode without storing an account password in project configuration.

### Verify and operate managed Runtime

The installation completes only after starting the platform jobs and observing
the terminal host. Run the combined service, HTTP, and host check afterward:

```shell
sshxx-service status
```

The Web check must pass, all three jobs must be running, and terminal-host
status must be returned. Read the daemon output to recover the current session
URL:

```shell
sshxx-service logs
```

Other lifecycle commands are:

```shell
sshxx-service start
sshxx-service stop
sshxx-service restart
```

`stop` and `restart` affect server and daemon only. They deliberately leave the
terminal host and every hosted terminal running.

### Check and update managed Runtime and Web

```shell
sshxx-service check-update
sshxx-service update
sshxx-service status
```

`update` downloads and verifies the latest Runtime, repoints stable wrappers,
refreshes service definitions, and restarts server and daemon. This activates
the bundled Web build without a separate Web update. It does not restart a
running compatible terminal host. When `sshxx-daemon terminal-host status`
reports no terminals, an operator may explicitly activate the installed host:

```shell
sshxx-daemon terminal-host restart
```

### Uninstall managed Runtime

```shell
sshxx-service uninstall
```

The uninstaller unregisters all three platform jobs and removes the installed
Runtime and command wrappers. It preserves the configured workspace, SSH
profiles, history, and cache by default. To remove those data as well:

```shell
sshxx-service uninstall --purge-data
```

On PowerShell, use `sshxx-service uninstall -PurgeData`. Uninstall first asks
the host to stop safely and aborts if active terminals exist. `--force` on Unix
or `-Force` on PowerShell is the explicit destructive override; it disconnects
all hosted shells and applications.

## Install and run the desktop client

First start and verify a Runtime, then keep the session URL printed by its
daemon. Open the [latest Release](https://github.com/glight2000/sshxx/releases)
and download one desktop artifact:

| Platform           | Package to choose             | Install and run                                                                                     |
| ------------------ | ----------------------------- | --------------------------------------------------------------------------------------------------- |
| Linux              | `*.AppImage`                  | Make it executable with `chmod +x <file>.AppImage`, then run it directly.                           |
| Debian/Ubuntu      | `*.deb`                       | Install with `sudo apt install ./<file>.deb`, then launch `sshxx-client` from the application menu. |
| Fedora/RHEL family | `*.rpm`                       | Install with `sudo dnf install ./<file>.rpm`, then launch `sshxx-client` from the application menu. |
| macOS              | architecture-matching `*.dmg` | Open the DMG, copy `sshxx-client` to Applications, then launch it.                                  |
| Windows x64        | `*.msi` or `*-setup.exe`      | Run one installer, then launch `sshxx-client` from the Start menu. Do not install both formats.     |

After launch, paste the full session URL into **Connect to a terminal** and
select **Connect**. Successful operation means the workspace opens and its
connection indicator becomes online. The client can connect to a Runtime on
another machine when its server URL is reachable and appropriately protected;
use HTTPS/WSS outside localhost or a trusted isolated LAN.

The current desktop artifacts are not production code-signed or notarized. macOS
uses ad-hoc signing and Windows may display SmartScreen warnings. Install only
artifacts obtained from the project Release and verify `SHA256SUMS` when
platform trust policy requires it.

## Update the desktop client

The desktop client contains its own bundled viewer and is not updated when the
Runtime/Web bundle changes. Download the newest desktop package for the same
platform from Releases, close the running client, and install it over the
existing version using the same platform method. Launch it again and reconnect
with the existing session URL. Runtime and desktop versions may be upgraded
independently when their protocol versions remain compatible, but keeping them
on the same Release is the supported and easiest-to-diagnose configuration.

## Release downloads

Each release contains:

- `sshxx-runtime-<version>-<target>.tar.gz` for Linux/macOS or `.zip` for
  Windows. Every runtime archive contains the three required executables, the
  built Web client, platform installer/service scripts, license, and READMEs.
- Optional Tauri desktop bundles for the supported desktop targets.
- `SHA256SUMS` covering every downloadable asset.
- GitHub artifact attestations for the release assets and checksum manifest.

The version in an archive name is the suite Release version, not a promise that
every bundled executable changed. Client, server, daemon, and terminal-host
report independent implementation versions and may legitimately differ inside
one archive. This keeps an unchanged terminal-host stable across ordinary Web,
client, server, daemon, packaging, and documentation releases. Compatibility is
determined by versioned protocols rather than equality of component version
strings.

Runtime targets are Linux x64/arm64, macOS Intel/Apple Silicon, and Windows x64.
Desktop bundles are produced for Linux x64, macOS Intel/Apple Silicon, and
Windows x64.

Release checksums and GitHub attestations provide build provenance and integrity
checks, but do not replace trusted platform code signing.

## Maintainer release flow

`.github/workflows/release.yaml` is tag-driven and uses this sequence:

1. Update `release.json` for the suite tag. Independently bump only the client,
   server, daemon, terminal-host, or internal core packages that actually
   changed, then run the version-boundary tests and all normal validation on
   `main`.
2. Create and push an annotated SemVer tag matching that version:

   ```shell
   git tag -a vX.Y.Z -m "sshxx vX.Y.Z"
   git push origin vX.Y.Z
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
