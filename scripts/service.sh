#!/bin/sh

set -eu

PROGRAM=${0##*/}
COMMAND=${1:-}
if [ -n "$COMMAND" ]; then shift; fi

usage() {
	cat <<'EOF'
Manage a persistent sshxx Runtime with the platform service manager.

Usage: sshxx-service <command> [options]

Commands:
  install       Register and start server, terminal-host, and daemon services.
  start         Start all services without disrupting an already-running host.
  stop          Stop server and daemon; the terminal host remains running.
  restart       Restart server and daemon; the terminal host remains running.
  status        Show service, HTTP, and terminal-host status.
  logs          Follow service logs.
  check-update  Compare the installed Runtime with the latest GitHub Release.
  update        Install the latest Runtime and restart server and daemon.
  uninstall     Remove services and Runtime; workspace data is kept by default.

Install options:
  --workspace PATH       Durable daemon data directory.
  --scope user|system    Login service or system boot service (default: user).
  --listen ADDRESS       Server listen address (default: 127.0.0.1).
  --port PORT            Server port (default: 8051).

Uninstall options:
  --force                Disconnect active hosted terminals.
  --purge-data           Also remove the configured workspace directory.
EOF
}

case "$COMMAND" in
install | start | stop | restart | status | logs | check-update | update | uninstall) ;;
-h | --help | help | "")
	usage
	exit 0
	;;
*)
	echo "Unknown command: $COMMAND" >&2
	usage >&2
	exit 2
	;;
esac

resolve_path() {
	path=$1
	while [ -L "$path" ]; do
		directory=$(CDPATH='' cd -- "$(dirname -- "$path")" && pwd)
		target=$(readlink "$path")
		case "$target" in
		/*) path=$target ;;
		*) path=$directory/$target ;;
		esac
	done
	CDPATH='' cd -- "$(dirname -- "$path")" && printf '%s/%s\n' "$(pwd)" "$(basename -- "$path")"
}

SCRIPT_PATH=$(resolve_path "$0")
SCRIPT_DIRECTORY=$(dirname -- "$SCRIPT_PATH")
if [ -n "${SSHXX_INSTALL_ROOT:-}" ]; then
	INSTALL_ROOT=$SSHXX_INSTALL_ROOT
else
	INSTALL_ROOT=$(CDPATH='' cd -- "$SCRIPT_DIRECTORY/../../.." && pwd)
fi
CONFIG_DIRECTORY=$INSTALL_ROOT/service

validate_line_value() {
	name=$1
	value=$2
	case "$value" in
	*"
"*)
		echo "$name must not contain a newline" >&2
		exit 2
		;;
	esac
}

read_config() {
	[ -d "$CONFIG_DIRECTORY" ] || {
		echo "Managed services are not installed. Run '$PROGRAM install' first." >&2
		exit 1
	}
	WORKSPACE=$(cat "$CONFIG_DIRECTORY/workspace")
	SCOPE=$(cat "$CONFIG_DIRECTORY/scope")
	LISTEN=$(cat "$CONFIG_DIRECTORY/listen")
	PORT=$(cat "$CONFIG_DIRECTORY/port")
	BIN_DIR=$(cat "$CONFIG_DIRECTORY/bin-dir")
}

WORKSPACE=${SSHXX_WORKSPACE:-"$HOME/sshxx-workspace"}
SCOPE=user
LISTEN=127.0.0.1
PORT=8051
BIN_DIR=${SSHXX_BIN_DIR:-"$HOME/.local/bin"}
FORCE=false
PURGE_DATA=false

if [ "$COMMAND" = install ]; then
	while [ "$#" -gt 0 ]; do
		case "$1" in
		--workspace)
			[ "$#" -ge 2 ] || {
				echo "--workspace requires a value" >&2
				exit 2
			}
			WORKSPACE=$2
			shift 2
			;;
		--scope)
			[ "$#" -ge 2 ] || {
				echo "--scope requires a value" >&2
				exit 2
			}
			SCOPE=$2
			shift 2
			;;
		--listen)
			[ "$#" -ge 2 ] || {
				echo "--listen requires a value" >&2
				exit 2
			}
			LISTEN=$2
			shift 2
			;;
		--port)
			[ "$#" -ge 2 ] || {
				echo "--port requires a value" >&2
				exit 2
			}
			PORT=$2
			shift 2
			;;
		--bin-dir)
			[ "$#" -ge 2 ] || {
				echo "--bin-dir requires a value" >&2
				exit 2
			}
			BIN_DIR=$2
			shift 2
			;;
		*)
			echo "Unknown install option: $1" >&2
			exit 2
			;;
		esac
	done
	case "$SCOPE" in user | system) ;; *)
		echo "--scope must be user or system" >&2
		exit 2
		;;
	esac
	case "$LISTEN" in '' | *[!0-9A-Fa-f:.]*)
		echo "--listen must be an IPv4 or IPv6 address" >&2
		exit 2
		;;
	esac
	case "$PORT" in '' | *[!0-9]*)
		echo "--port must be numeric" >&2
		exit 2
		;;
	esac
	if [ "$PORT" -lt 1 ] || [ "$PORT" -gt 65535 ]; then
		echo "--port is out of range" >&2
		exit 2
	fi
	for pair in "workspace:$WORKSPACE" "listen:$LISTEN" "bin-dir:$BIN_DIR"; do
		validate_line_value "${pair%%:*}" "${pair#*:}"
	done
	mkdir -p "$WORKSPACE" "$CONFIG_DIRECTORY"
	WORKSPACE=$(CDPATH='' cd -- "$WORKSPACE" && pwd)
	BIN_DIR=$(CDPATH='' cd -- "$BIN_DIR" 2>/dev/null && pwd || printf '%s\n' "$BIN_DIR")
	printf '%s\n' "$WORKSPACE" >"$CONFIG_DIRECTORY/workspace"
	printf '%s\n' "$SCOPE" >"$CONFIG_DIRECTORY/scope"
	printf '%s\n' "$LISTEN" >"$CONFIG_DIRECTORY/listen"
	printf '%s\n' "$PORT" >"$CONFIG_DIRECTORY/port"
	printf '%s\n' "$BIN_DIR" >"$CONFIG_DIRECTORY/bin-dir"
	chmod 700 "$CONFIG_DIRECTORY"
	chmod 600 "$CONFIG_DIRECTORY"/*
elif [ "$COMMAND" = uninstall ]; then
	while [ "$#" -gt 0 ]; do
		case "$1" in
		--force)
			FORCE=true
			shift
			;;
		--purge-data)
			PURGE_DATA=true
			shift
			;;
		*)
			echo "Unknown uninstall option: $1" >&2
			exit 2
			;;
		esac
	done
	read_config
else
	[ "$#" -eq 0 ] || {
		echo "$COMMAND does not accept options" >&2
		exit 2
	}
	read_config
fi

case $(uname -s) in
Linux) PLATFORM=linux ;;
Darwin) PLATFORM=macos ;;
*)
	echo "Managed Runtime services are supported on Linux and macOS by this script." >&2
	exit 1
	;;
esac

CURRENT_VERSION=$(cat "$INSTALL_ROOT/current-version")
CURRENT_DIRECTORY=$INSTALL_ROOT/versions/$CURRENT_VERSION
SERVER=$INSTALL_ROOT/bin/sshxx-server
DAEMON=$INSTALL_ROOT/bin/sshxx-daemon
HOST=$INSTALL_ROOT/bin/sshxx-terminal-host
HOST_STATE=$WORKSPACE/cache/terminal-host

server_url() {
	case "$LISTEN" in
	0.0.0.0) printf 'http://127.0.0.1:%s\n' "$PORT" ;;
	:: | "[::]") printf 'http://[::1]:%s\n' "$PORT" ;;
	*:*) printf 'http://[%s]:%s\n' "$LISTEN" "$PORT" ;;
	*) printf 'http://%s:%s\n' "$LISTEN" "$PORT" ;;
	esac
}

wait_for_host() {
	attempt=0
	until (cd "$WORKSPACE" && "$HOST" status --state-dir "$HOST_STATE") >/dev/null 2>&1; do
		attempt=$((attempt + 1))
		[ "$attempt" -lt 50 ] || {
			echo "sshxx-terminal-host did not become ready" >&2
			return 1
		}
		sleep 0.1
	done
}

check_http() {
	url=$(server_url)/
	if curl -fsS "$url" >/dev/null 2>&1; then
		echo "Web check: PASS ($url)"
	else
		echo "Web check: FAIL ($url)" >&2
		return 1
	fi
}

systemd_escape() {
	printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/%/%%/g'
}

write_systemd_units() {
	unit_directory=$1
	wanted_by=$2
	user_lines=$3
	mkdir -p "$unit_directory"
	server_q=$(systemd_escape "$SERVER")
	host_q=$(systemd_escape "$HOST")
	daemon_q=$(systemd_escape "$DAEMON")
	workspace_q=$(systemd_escape "$WORKSPACE")
	host_state_q=$(systemd_escape "$HOST_STATE")
	daemon_server_q=$(systemd_escape "$(server_url)")
	cat >"$unit_directory/sshxx-server.service" <<EOF
[Unit]
Description=sshxx self-hosted session server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
$user_lines
ExecStart="$server_q" --listen $LISTEN --port $PORT
Restart=on-failure
RestartSec=2

[Install]
WantedBy=$wanted_by
EOF
	cat >"$unit_directory/sshxx-terminal-host.service" <<EOF
[Unit]
Description=sshxx persistent terminal host

[Service]
Type=simple
$user_lines
WorkingDirectory="$workspace_q"
ExecStart="$host_q" serve --state-dir "$host_state_q"
Restart=on-failure
RestartSec=2

[Install]
WantedBy=$wanted_by
EOF
	cat >"$unit_directory/sshxx-daemon.service" <<EOF
[Unit]
Description=sshxx terminal and filesystem daemon
After=network-online.target sshxx-server.service sshxx-terminal-host.service
Wants=network-online.target sshxx-server.service sshxx-terminal-host.service

[Service]
Type=simple
$user_lines
WorkingDirectory="$workspace_q"
ExecStart="$daemon_q" --server "$daemon_server_q"
Restart=on-failure
RestartSec=2

[Install]
WantedBy=$wanted_by
EOF
}

linux_control() {
	if [ "$SCOPE" = user ]; then
		systemctl --user "$@"
	else
		sudo systemctl "$@"
	fi
}

linux_install() {
	command -v systemctl >/dev/null 2>&1 || {
		echo "systemctl is required" >&2
		exit 1
	}
	if [ "$SCOPE" = user ]; then
		unit_directory=${XDG_CONFIG_HOME:-"$HOME/.config"}/systemd/user
		write_systemd_units "$unit_directory" default.target ""
	else
		temporary=$(mktemp -d)
		trap 'rm -rf "$temporary"' EXIT HUP INT TERM
		service_user=${SUDO_USER:-$(id -un)}
		service_group=$(id -gn "$service_user")
		write_systemd_units "$temporary" multi-user.target "User=$service_user
Group=$service_group"
		sudo install -m 0644 "$temporary"/*.service /etc/systemd/system/
	fi
	linux_control daemon-reload
	linux_control enable sshxx-server.service sshxx-terminal-host.service sshxx-daemon.service
	linux_control restart sshxx-server.service
	linux_control start sshxx-terminal-host.service
	wait_for_host
	linux_control restart sshxx-daemon.service
	echo "Registered Linux $SCOPE services."
}

linux_start() {
	linux_control start sshxx-server.service sshxx-terminal-host.service
	wait_for_host
	linux_control start sshxx-daemon.service
}

linux_stop() {
	linux_control stop sshxx-daemon.service sshxx-server.service
	echo "Stopped daemon and server; terminal-host remains running."
}

linux_restart() {
	linux_control restart sshxx-server.service sshxx-daemon.service
	echo "Restarted daemon and server; terminal-host was not restarted."
}

linux_status() {
	linux_control --no-pager --full status sshxx-server.service sshxx-terminal-host.service sshxx-daemon.service || true
	check_http
	(cd "$WORKSPACE" && "$HOST" status --state-dir "$HOST_STATE")
}

linux_logs() {
	if [ "$SCOPE" = user ]; then
		journalctl --user -f -u sshxx-server.service -u sshxx-terminal-host.service -u sshxx-daemon.service
	else
		sudo journalctl -f -u sshxx-server.service -u sshxx-terminal-host.service -u sshxx-daemon.service
	fi
}

linux_uninstall() {
	linux_control disable --now sshxx-daemon.service sshxx-server.service >/dev/null 2>&1 || true
	if [ "$FORCE" = true ]; then force_argument=--force; else force_argument=; fi
	if ! (cd "$WORKSPACE" && "$HOST" stop --state-dir "$HOST_STATE" $force_argument); then
		linux_control start sshxx-server.service sshxx-daemon.service >/dev/null 2>&1 || true
		echo "Uninstall stopped because terminal-host still owns active terminals." >&2
		exit 1
	fi
	linux_control disable --now sshxx-terminal-host.service >/dev/null 2>&1 || true
	if [ "$SCOPE" = user ]; then
		unit_directory=${XDG_CONFIG_HOME:-"$HOME/.config"}/systemd/user
		rm -f "$unit_directory/sshxx-server.service" "$unit_directory/sshxx-terminal-host.service" "$unit_directory/sshxx-daemon.service"
	else
		sudo rm -f /etc/systemd/system/sshxx-server.service /etc/systemd/system/sshxx-terminal-host.service /etc/systemd/system/sshxx-daemon.service
	fi
	linux_control daemon-reload
}

xml_escape() {
	printf '%s' "$1" | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g' -e "s/'/\&apos;/g"
}

write_launchd_plist() {
	role=$1
	output=$2
	label=io.sshxx.$role
	case "$role" in
	server)
		program=$SERVER
		arguments="<string>--listen</string><string>$(xml_escape "$LISTEN")</string><string>--port</string><string>$PORT</string>"
		;;
	terminal-host)
		program=$HOST
		arguments="<string>serve</string><string>--state-dir</string><string>$(xml_escape "$HOST_STATE")</string>"
		;;
	daemon)
		program=$DAEMON
		arguments="<string>--server</string><string>$(xml_escape "$(server_url)")</string>"
		;;
	esac
	if [ "$SCOPE" = system ]; then
		service_user=${SUDO_USER:-$(id -un)}
		service_group=$(id -gn "$service_user")
		identity="<key>UserName</key><string>$(xml_escape "$service_user")</string><key>GroupName</key><string>$(xml_escape "$service_group")</string>"
	else
		identity=
	fi
	mkdir -p "$INSTALL_ROOT/logs"
	cat >"$output" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$label</string>
<key>ProgramArguments</key><array><string>$(xml_escape "$program")</string>$arguments</array>
<key>WorkingDirectory</key><string>$(xml_escape "$WORKSPACE")</string>
$identity
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
<key>StandardOutPath</key><string>$(xml_escape "$INSTALL_ROOT/logs/$role.log")</string>
<key>StandardErrorPath</key><string>$(xml_escape "$INSTALL_ROOT/logs/$role.log")</string>
</dict></plist>
EOF
}

launch_domain() {
	if [ "$SCOPE" = user ]; then printf 'gui/%s\n' "$(id -u)"; else printf 'system\n'; fi
}

launch_control() {
	if [ "$SCOPE" = system ]; then sudo launchctl "$@"; else launchctl "$@"; fi
}

macos_install() {
	temporary=$(mktemp -d)
	trap 'rm -rf "$temporary"' EXIT HUP INT TERM
	for role in server terminal-host daemon; do write_launchd_plist "$role" "$temporary/io.sshxx.$role.plist"; done
	domain=$(launch_domain)
	if [ "$SCOPE" = user ]; then
		destination=$HOME/Library/LaunchAgents
		mkdir -p "$destination"
		for role in server terminal-host daemon; do
			install -m 0644 "$temporary/io.sshxx.$role.plist" "$destination/"
		done
	else
		destination=/Library/LaunchDaemons
		for role in server terminal-host daemon; do
			sudo install -o root -g wheel -m 0644 "$temporary/io.sshxx.$role.plist" "$destination/"
		done
	fi
	launch_control bootout "$domain/io.sshxx.daemon" >/dev/null 2>&1 || true
	launch_control bootout "$domain/io.sshxx.server" >/dev/null 2>&1 || true
	launch_control bootstrap "$domain" "$destination/io.sshxx.server.plist"
	if ! launch_control print "$domain/io.sshxx.terminal-host" >/dev/null 2>&1; then
		launch_control bootstrap "$domain" "$destination/io.sshxx.terminal-host.plist"
	fi
	wait_for_host
	launch_control bootstrap "$domain" "$destination/io.sshxx.daemon.plist"
	echo "Registered macOS $SCOPE launchd services."
}

macos_start() {
	domain=$(launch_domain)
	if [ "$SCOPE" = user ]; then destination=$HOME/Library/LaunchAgents; else destination=/Library/LaunchDaemons; fi
	for role in server terminal-host; do
		if ! launch_control print "$domain/io.sshxx.$role" >/dev/null 2>&1; then
			launch_control bootstrap "$domain" "$destination/io.sshxx.$role.plist"
		else
			launch_control kickstart "$domain/io.sshxx.$role"
		fi
	done
	wait_for_host
	if ! launch_control print "$domain/io.sshxx.daemon" >/dev/null 2>&1; then
		launch_control bootstrap "$domain" "$destination/io.sshxx.daemon.plist"
	else
		launch_control kickstart "$domain/io.sshxx.daemon"
	fi
}

macos_stop() {
	domain=$(launch_domain)
	launch_control bootout "$domain/io.sshxx.daemon" >/dev/null 2>&1 || true
	launch_control bootout "$domain/io.sshxx.server" >/dev/null 2>&1 || true
	echo "Stopped daemon and server; terminal-host remains running."
}

macos_restart() {
	domain=$(launch_domain)
	if [ "$SCOPE" = user ]; then destination=$HOME/Library/LaunchAgents; else destination=/Library/LaunchDaemons; fi
	for role in server daemon; do
		if launch_control print "$domain/io.sshxx.$role" >/dev/null 2>&1; then
			launch_control kickstart -k "$domain/io.sshxx.$role"
		else
			launch_control bootstrap "$domain" "$destination/io.sshxx.$role.plist"
		fi
	done
	echo "Restarted daemon and server; terminal-host was not restarted."
}

macos_status() {
	domain=$(launch_domain)
	for role in server terminal-host daemon; do launch_control print "$domain/io.sshxx.$role" | sed -n '1,18p'; done
	check_http
	(cd "$WORKSPACE" && "$HOST" status --state-dir "$HOST_STATE")
}

macos_logs() {
	tail -n 100 -F "$INSTALL_ROOT/logs/server.log" "$INSTALL_ROOT/logs/terminal-host.log" "$INSTALL_ROOT/logs/daemon.log"
}

macos_uninstall() {
	if [ "$FORCE" = true ]; then force_argument=--force; else force_argument=; fi
	if ! (cd "$WORKSPACE" && "$HOST" stop --state-dir "$HOST_STATE" $force_argument); then
		echo "Uninstall stopped because terminal-host still owns active terminals." >&2
		exit 1
	fi
	domain=$(launch_domain)
	if [ "$SCOPE" = user ]; then destination=$HOME/Library/LaunchAgents; else destination=/Library/LaunchDaemons; fi
	for role in daemon server terminal-host; do
		launch_control bootout "$domain/io.sshxx.$role" >/dev/null 2>&1 || true
		if [ "$SCOPE" = user ]; then rm -f "$destination/io.sshxx.$role.plist"; else sudo rm -f "$destination/io.sshxx.$role.plist"; fi
	done
}

latest_version() {
	release_url=$(curl -fsSL -o /dev/null -w '%{url_effective}' https://github.com/glight2000/sshxx/releases/latest)
	printf '%s\n' "${release_url##*/v}"
}

check_update() {
	latest=$(latest_version)
	echo "Installed Runtime: $CURRENT_VERSION"
	echo "Latest Runtime:    $latest"
	if [ "$CURRENT_VERSION" = "$latest" ]; then
		echo "Runtime is up to date."
	else
		echo "Runtime update available. Run: sshxx-service update"
	fi
}

update_runtime() {
	installer=$CURRENT_DIRECTORY/scripts/install.sh
	[ -x "$installer" ] || {
		echo "Installed Runtime does not contain its installer: $installer" >&2
		exit 1
	}
	exec "$installer" --install-root "$INSTALL_ROOT" --bin-dir "$BIN_DIR" --managed --workspace "$WORKSPACE" --scope "$SCOPE" --listen "$LISTEN" --port "$PORT"
}

safe_remove_workspace() {
	[ "$PURGE_DATA" = true ] || return 0
	case "$WORKSPACE" in / | "$HOME" | "")
		echo "Refusing to purge unsafe workspace path: $WORKSPACE" >&2
		exit 1
		;;
	esac
	if [ "$WORKSPACE" != "$INSTALL_ROOT" ]; then
		rm -rf -- "$WORKSPACE"
		echo "Removed workspace data: $WORKSPACE"
	fi
}

remove_runtime() {
	case "$WORKSPACE/" in
	"$INSTALL_ROOT"/*)
		if [ "$PURGE_DATA" != true ]; then
			echo "Refusing to remove Runtime because the preserved workspace is inside it: $WORKSPACE" >&2
			echo "Move the workspace outside $INSTALL_ROOT or rerun with --purge-data." >&2
			exit 1
		fi
		;;
	esac
	for command_name in sshxx-server sshxx-daemon sshxx-terminal-host sshxx-service; do
		link=$BIN_DIR/$command_name
		if [ -L "$link" ] && [ "$(readlink "$link")" = "$INSTALL_ROOT/bin/$command_name" ]; then
			rm -f "$link"
		fi
	done
	case "$INSTALL_ROOT" in / | "$HOME" | "")
		echo "Refusing to remove unsafe install root: $INSTALL_ROOT" >&2
		exit 1
		;;
	esac
	rm -rf -- "$INSTALL_ROOT"
	echo "Removed sshxx Runtime. Workspace data was preserved at $WORKSPACE"
}

case "$COMMAND:$PLATFORM" in
install:linux) linux_install ;;
install:macos) macos_install ;;
start:linux) linux_start ;;
start:macos) macos_start ;;
stop:linux) linux_stop ;;
stop:macos) macos_stop ;;
restart:linux) linux_restart ;;
restart:macos) macos_restart ;;
status:linux) linux_status ;;
status:macos) macos_status ;;
logs:linux) linux_logs ;;
logs:macos) macos_logs ;;
check-update:*) check_update ;;
update:*) update_runtime ;;
uninstall:linux)
	linux_uninstall
	safe_remove_workspace
	remove_runtime
	;;
uninstall:macos)
	macos_uninstall
	safe_remove_workspace
	remove_runtime
	;;
esac
