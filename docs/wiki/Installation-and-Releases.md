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

### Settings restart versus installation

Daemon 0.11.5 requires a server supporting independent terminal output
encryption epochs. Upgrade server and Web client before starting this daemon;
old viewers receive an upgrade error for new encrypted streams. Existing
workspace files and legacy server snapshots remain readable. Host 0.10.4 also
fixes final-output ordering, but activating that separate host executable still
requires an empty terminal list or an explicit destructive restart.

Starting with Release v0.13.2 (client 0.13.1, daemon/server 0.11.2, host
0.10.2), Settings can reload the installed daemon or host executable. These
controls do not download a release and do not restart server. After a manual
download/install, they can activate the corresponding installed program;
`sshxx-service update` already restarts server and daemon, so another daemon
restart is unnecessary.

The first upgrade from earlier controls needs a real external service/process
restart; the old buttons only reset runtime connections/state. On Windows, rerun
the installer to refresh the restart-aware command wrappers too. Do not restart
host while an installation command is still running inside one of its terminals:
that terminates the command along with every hosted task. Complete the install
first and save/finish tasks, or operate from an independent SSH/system terminal.
See
[lifecycle, handoff, and failure boundaries](Architecture-and-State.md#terminal-host-lifecycle-and-upgrades).

### Web-triggered updates

Settings separates the client build's Release version, the running daemon's
Runtime archive version, and independent client/server/daemon/host versions.
Host versions refresh automatically after external restarts; a failed probe
shows `unknown`. Runtime status updates approximately every 15 seconds.

**Update Runtime & restart** starts an independently managed Linux/systemd job.
It installs using the existing checksum-verifying updater, restarts server and
daemon, and **must not restart terminal-host**. Closing a browser or restarting
daemon does not stop the job. Settings shows ready/updating/completed/failed;
after completion, use **Reload Web client**. Separately packaged apps still
require their own installation update. macOS, Windows, source/foreground builds
and installations without an authorized job use the existing manual updater.

This is deliberately opt-in. An administrator must first provision a fixed
`sshxx-update.service` and enable the daemon's `SSHXX_WEB_UPDATE` environment
variable. The client cannot supply an executable, arguments or a download URL.
Do not grant unrestricted passwordless sudo or let the daemon edit a privileged
job, updater, its dependencies, or their parent directories (including ACLs).

For a **system service**, adapt this unit to the existing installation and
service account, and install it as a root-owned file at
`/etc/systemd/system/sshxx-update.service`:

```ini
[Unit]
Description=sshxx authorized Runtime update
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
RemainAfterExit=yes
Environment=HOME=/home/sshxx
Environment=SUDO_USER=sshxx
ExecStart=/opt/sshxx/bin/sshxx-service update
TimeoutStartSec=30min
StandardInput=null
# Existing deployment updaters may print protected session URLs to stdout.
StandardOutput=null
StandardError=journal
```

`ExecStart` may instead call the deployment's **existing** trusted updater with
its fixed noninteractive arguments. Do not duplicate its implementation. Verify
that it preserves the host and returns failure on an unsuccessful deployment.
Keep `RemainAfterExit=yes` so completed results survive systemd garbage
collection and daemon restarts. The daemon restarts this job only when it is not
updating; conflicting queued systemd jobs are rejected, not replaced. Do not
enable this unit at boot or bind its lifetime to the daemon service.

Use `sudo visudo -f /etc/sudoers.d/sshxx-web-update` to authorize only the fixed
operation for the actual daemon account (replace `sshxx`):

```sudoers
sshxx ALL=(root) NOPASSWD: /usr/bin/systemctl restart --no-block --job-mode=fail sshxx-update.service
```

Use `sudo systemctl edit sshxx-daemon.service` to add:

```ini
[Service]
Environment=SSHXX_WEB_UPDATE=system
```

Then run `sudo systemctl daemon-reload` and restart **daemon only** to load the
new environment. This setup requires the new daemon/server implementation to be
installed first; older releases cannot bootstrap this feature from the Web.

For **user services**, place the analogous job in
`~/.config/systemd/user/sshxx-update.service`, use the existing user
installation's absolute `sshxx-service update` path, omit `SUDO_USER`, set
`SSHXX_WEB_UPDATE=user` in the user daemon's drop-in, and use
`systemctl --user`. No sudo rule is needed. Its job must target that user's
service deployment only.

Inspect job results with
`systemctl show sshxx-update.service -p ActiveState -p Result -p ExecMainStatus`
and protected local logs with `journalctl -u sshxx-update.service` (add `--user`
for user scope). Repeated starts while the oneshot is running do not create
parallel updates. A failed update is not automatically retried; inspect it
before retrying. Remove the environment opt-in and sudo rule to revoke
Web-triggered updates. Do not remove/stop an actively running job merely to hide
its button.

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

## v0.13.5 upgrade notes

| Component               | Version | Changes                                                                           |
| ----------------------- | ------- | --------------------------------------------------------------------------------- |
| Suite / Runtime archive | 0.13.5  | Terminal output isolation, recovery and bounded history                           |
| Web / Tauri client      | 0.13.4  | Fresh renderer recovery, successful-parse acknowledgements and checkpoint history |
| Daemon                  | 0.11.5  | Independent output encryption epochs and bounded embedded terminal checkpoints    |
| Server                  | 0.11.5  | Retention-gap detection, compatible output routing and subscription cleanup       |
| Internal core           | 0.11.5  | Additive output epoch and checkpoint protocol fields                              |
| Terminal host           | 0.10.4  | Drain final PTY output before reporting process exit                              |

Terminal output now uses a fresh encryption key for each daemon/terminal output
incarnation. Server retention gaps and host replay gaps rebuild the affected
viewer instead of silently joining discontinuous output. Failed or disposed
renderers do not acknowledge successful parsing, and replacing or disconnecting
a subscription cancels its pending forwarding task. Host exit notifications wait
for the PTY reader to finish forwarding the final output.

The daemon embeds the pinned terminal parser to provide a current checkpoint and
bounded, on-demand history. Unsupported or expired checkpoints fall back to
ordered retained output. A raw retained tail cannot reconstruct missing terminal
state. Parser and recovery tests pass; sustained multi-device, background-tab,
and application business-flow validation remain outstanding.

Upgrade **server and Web client before daemon**. New daemons require
output-epoch support from the server; old viewers receive an upgrade message for
new encrypted streams. Existing workspace files and legacy server snapshots
remain readable. Source builds require `npm ci` before compiling the daemon;
installed Runtime binaries need no Node.js service.

Installing this release preserves a compatible running host. Activating the
host's final-output fix requires a separate host restart, which disconnects all
hosted processes. Finish or save work first; use a non-forced restart after the
terminal list becomes empty, or explicitly acknowledge the loss with `--force`.

Downloads contain the supported Runtime/Web bundle and optional Tauri desktop
client. Electron, Godot and Pocket experiments are not built or packaged.

## v0.13.4 upgrade notes

| Component               | Version | Changes                                                                                                     |
| ----------------------- | ------- | ----------------------------------------------------------------------------------------------------------- |
| Suite / Runtime archive | 0.13.4  | Terminal paste recovery and local canvas interaction fixes                                                  |
| Web / Tauri client      | 0.13.3  | Independent page-camera fades, visibility repaint, Shift+Esc, trackpad routing and HTTP preview diagnostics |
| Daemon                  | 0.11.4  | Retained-output paste-mode checkpoints and encrypted forwarding                                             |
| Server                  | 0.11.4  | Compatible encrypted paste-mode metadata in output subscriptions and snapshots                              |
| Internal core           | 0.11.4  | Additive output checkpoint protocol fields                                                                  |
| Terminal host           | 0.10.3  | Bounded paste-mode parser/checkpoints across retained-output pruning and reattachment                       |

Update server and daemon together, then reload the Web client or update the
packaged client. Page switching keeps all components mounted: outgoing and
incoming pages each retain their own camera during the fade. Plain Escape stays
inside the terminal/editor; Shift+Escape clears desktop focus and selection.
Trackpad gestures pan when starting outside the focused window, while
mouse-wheel focus routing is unchanged.

The paste fix addresses a verified case where pruning older output loses the
bracketed-paste enable sequence. It does not claim that every large-paste issue
has the same cause. Older compatible peers remain usable; complete recovery
through host retention/reattachment requires the updated host. Installing the
archive does **not** replace the running host. Finish or save tasks before the
explicit host restart: all hosted PTY/SSH processes will be disconnected. A
compatible old host can remain running until that maintenance window.

Occasional unsolicited scrolling and the reported split/frozen output region
remain unconfirmed issues, not advertised fixes. The page-visibility repaint fix
is covered locally but still needs verification against affected workloads. HTTP
pages blocked inside HTTPS workspaces now show an explanation; the release does
not weaken browser security or configure a deployment's HTTPS proxy.

## v0.13.3 upgrade notes

| Component               | Version | Changes                                                         |
| ----------------------- | ------- | --------------------------------------------------------------- |
| Suite / Runtime archive | 0.13.3  | Version reporting and opt-in managed Web updates                |
| Web / Tauri client      | 0.13.2  | Live versions, update controls, emoji/kaomoji in chat and notes |
| Daemon                  | 0.11.3  | Live host probes and administrator-managed update job bridge    |
| Server                  | 0.11.3  | Live Runtime status for existing and newly connected viewers    |
| Internal core           | 0.11.3  | Compatible Runtime status and update-action protocol additions  |
| Terminal host           | 0.10.2  | Unchanged; no host restart required                             |

Update server and daemon together, then reload the Web client. A host upgraded
externally is now reported live instead of retaining the daemon's startup
version. Settings labels the suite and module versions separately; different
numbers are expected.

The first installation of this release still uses the existing manual updater.
Web-triggered updates require the administrator setup described above and do not
automatically grant privileges. The fixed Linux/systemd job survives a browser
disconnect or daemon restart and must preserve the running terminal host.
Standalone packaged clients are updated separately.

## v0.13.2 upgrade notes

| Module                  | Version | Changes                                                     |
| ----------------------- | ------- | ----------------------------------------------------------- |
| Suite / Runtime archive | 0.13.2  | Updated runtime, desktop client, installers and manuals     |
| Web / Tauri client      | 0.13.1  | Persistent chat, note attachments and real restart controls |
| Daemon                  | 0.11.2  | Attachment storage, chat persistence and executable restart |
| Server                  | 0.11.2  | Chat history, attachment routing and restart coordination   |
| Terminal host           | 0.10.2  | Explicit restart can reload the installed host executable   |
| Internal core           | 0.11.2  | Compatible chat, attachment and restart protocol additions  |

Chat is a desktop sidebar or phone fullscreen surface with the latest 500
messages persisted by the daemon. Chat and notes accept image, video and file
attachments; audio sending/recording is not included. See the
[feature guide](Features#workspace-chat-and-attachments) for limits, permissions
and cleanup behavior.

Upgrade server and daemon together and reload the Web client (or update the
desktop client). Existing workspaces load without a manual migration. Runtime
archive and component versions are deliberately independent: do not reject a
checksum-verified archive merely because a binary reports a different version
from its archive name.

Settings now reloads the installed daemon/host executable, rather than merely
reconnecting or clearing in-memory state. The first upgrade from older releases
requires an external service/process restart; on Windows, rerun the installer to
refresh command wrappers. See
[restart versus installation](#settings-restart-versus-installation).

Installing this Runtime does not require immediately restarting a compatible
host. Keep it running to preserve PTYs; chat and attachments do not depend on
the new host. Activating the host restart implementation requires a planned host
restart, which disconnects **all hosted processes**. Finish or save tasks first
and do not restart the host from a terminal still running the update command.

## v0.13.1 upgrade notes

The v0.13.0 tag was pushed, but its Release quality gate failed and no download
Release was published. v0.13.1 retains that tag and retries the complete release
pipeline after stabilizing the hosted-runner integration tests: derive the
session key once instead of running Argon2 on every output chunk, accumulate
split output markers, and report a closed output channel without spinning. The
original I/O timeouts and process/history assertions remain in place.

Only the suite archive version advances to **0.13.1**. The Web/Tauri client
remains **0.13.0**, daemon/server/core **0.11.1**, and terminal-host **0.10.1**;
test-only changes do not alter shipped runtime behavior. The features and
upgrade guidance below apply unchanged. No terminal-host restart is required
when upgrading from v0.12.0.

## v0.13.0 upgrade notes

| Module                  | Version | Changes                                                        |
| ----------------------- | ------- | -------------------------------------------------------------- |
| Suite / Runtime archive | 0.13.0  | Updated client, bundle and manuals                             |
| Web / Tauri client      | 0.13.0  | Phone gestures, fullscreen, special keys and note improvements |
| Daemon                  | 0.11.1  | Unchanged from v0.12.0                                         |
| Server                  | 0.11.1  | Unchanged from v0.12.0                                         |
| Terminal host           | 0.10.1  | Unchanged from v0.12.0                                         |
| Internal core           | 0.11.1  | Unchanged from v0.12.0                                         |

- Phone taps focus components without moving the canvas or entering fullscreen.
  Fullscreen is explicit, with a top-left Restore button and keyboard-aware
  content layout. Standard toolbar/search and page tabs remain in canvas view.
- Two-finger canvas pan/zoom works over focused content. Fullscreen blocks
  canvas and outer browser zoom while preserving content scrolling. Movement
  previews avoid changing CSS variables inherited by every component; all pages
  and subscriptions stay mounted.
- Phone terminals provide arrow/control keys and Text + Enter, Paste only and
  Direct keys modes. Fullscreen and canvas view reuse one keypad implementation.
- Selecting text within a note paragraph preserves native selection and copy;
  crossing paragraphs still selects blocks. Notes disable browser spellchecking
  and use lighter paragraph styling with 15px text and 26px line spacing.

From v0.12.0, update the served Web build (or install the matching desktop
client) and reload the browser. The runtime archive includes this updated Web
build with the unchanged backend binaries. No protocol or workspace migration is
needed, and **do not restart terminal-host for this client-only update**. If
upgrading from an older release, also read the v0.12.0 notes below. Experimental
Electron and Godot clients remain at 0.1.0, outside the standard release matrix.

## v0.12.0 upgrade notes

| Module                  | Version | Changes                                                                                                                       |
| ----------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Suite / Runtime archive | 0.12.0  | Release bundle and updated manuals                                                                                            |
| Web / Tauri client      | 0.12.0  | Phone navigation and terminal reader, local shortcuts, output/paste recovery, bounded history cleanup, and page repaint fixes |
| Daemon                  | 0.11.1  | Retained-output gap recovery and progress diagnostics                                                                         |
| Server                  | 0.11.1  | Compatible output-gap recovery and subscription diagnostics                                                                   |
| Terminal host           | 0.10.1  | Connection/subscription cleanup, cancelled terminal-creation cleanup, and output progress diagnostics                         |
| Internal core           | 0.11.1  | Additive output-recovery protocol fields                                                                                      |

Experimental Electron and Godot clients remain at 0.1.0 and are not part of the
standard release build matrix.

Upgrade daemon and server together to repair gaps older than the retained output
range; a Web-only update cannot fix an upstream sequence gap. Compatible running
terminal hosts remain attached across a daemon update. Installing the new host
binary does not activate its fixes in an already running host: schedule the
[host upgrade procedure](Architecture-and-State.md#terminal-host-lifecycle-and-upgrades)
after finishing or saving tasks, because restarting it disconnects every hosted
process. The new daemon/server recovery protocol also works with the previous
compatible host, so its restart can be deferred. No recovery action promises an
exact TUI screen snapshot after old output has been discarded.

The phone terminal reader is a local presentation/input surface, not a shared
PTY resize. Page switches retain mounted components and output subscriptions;
the larger multi-client endurance-test expansion remains deferred.

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
