#!/bin/sh

set -eu

REPOSITORY="glight2000/sshxx"
INSTALL_ROOT=${SSHXX_INSTALL_ROOT:-"$HOME/.local/share/sshxx"}
BIN_DIR=${SSHXX_BIN_DIR:-"$HOME/.local/bin"}
VERSION=""
RUN_AFTER_INSTALL=false
MANAGED=false
WORKSPACE=${SSHXX_WORKSPACE:-"$HOME/sshxx-workspace"}
SCOPE=user
LISTEN=127.0.0.1
PORT=8051
CHECK_ONLY=false

usage() {
	cat <<'EOF'
Install a self-hosted sshxx runtime bundle from GitHub Releases.

Usage: install.sh [options]

  --version VERSION    Install a specific release; defaults to the latest.
  --install-root PATH  Versioned runtime location.
  --bin-dir PATH       Command-wrapper location; add it to PATH.
  --run                Start a local server and daemon after installation.
  --managed            Register and start platform-managed background services.
  --workspace PATH     Managed daemon data directory.
  --scope user|system  Managed service scope (default: user).
  --listen ADDRESS     Managed server listen address (default: 127.0.0.1).
  --port PORT          Managed server port (default: 8051).
  --check              Check the installed version against the latest Release.
EOF
}

while [ "$#" -gt 0 ]; do
	case "$1" in
	--version)
		[ "$#" -ge 2 ] || {
			echo "--version requires a value" >&2
			exit 2
		}
		VERSION=$2
		shift 2
		;;
	--install-root)
		[ "$#" -ge 2 ] || {
			echo "--install-root requires a value" >&2
			exit 2
		}
		INSTALL_ROOT=$2
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
	--run)
		RUN_AFTER_INSTALL=true
		shift
		;;
	--managed)
		MANAGED=true
		shift
		;;
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
	--check)
		CHECK_ONLY=true
		shift
		;;
	-h | --help)
		usage
		exit 0
		;;
	*)
		echo "Unknown option: $1" >&2
		usage >&2
		exit 2
		;;
	esac
done

if { [ "$RUN_AFTER_INSTALL" = true ] && [ "$MANAGED" = true ]; } ||
	{ [ "$CHECK_ONLY" = true ] && { [ "$RUN_AFTER_INSTALL" = true ] || [ "$MANAGED" = true ]; }; }; then
	echo "--run, --managed, and --check are mutually exclusive" >&2
	exit 2
fi

for command in curl tar; do
	command -v "$command" >/dev/null 2>&1 || {
		echo "Required command is missing: $command" >&2
		exit 1
	}
done

case "$(uname -s):$(uname -m)" in
Linux:x86_64 | Linux:amd64)
	TARGET="x86_64-unknown-linux-gnu"
	;;
Linux:aarch64 | Linux:arm64)
	TARGET="aarch64-unknown-linux-gnu"
	;;
Darwin:x86_64 | Darwin:amd64)
	TARGET="x86_64-apple-darwin"
	;;
Darwin:arm64 | Darwin:aarch64)
	TARGET="aarch64-apple-darwin"
	;;
*)
	echo "Unsupported platform: $(uname -s) $(uname -m)" >&2
	exit 1
	;;
esac

if [ -z "$VERSION" ]; then
	RELEASE_URL=$(curl -fsSL -o /dev/null -w '%{url_effective}' \
		"https://github.com/$REPOSITORY/releases/latest")
	VERSION=${RELEASE_URL##*/}
fi
VERSION=${VERSION#v}

case "$VERSION" in
*[!0-9.]* | .* | *..* | *. | *.*.*.*)
	echo "Invalid release version: $VERSION" >&2
	exit 1
	;;
esac

if [ "$CHECK_ONLY" = true ]; then
	if [ -f "$INSTALL_ROOT/current-version" ]; then
		INSTALLED_VERSION=$(cat "$INSTALL_ROOT/current-version")
	else
		INSTALLED_VERSION="not installed"
	fi
	echo "Installed Runtime: $INSTALLED_VERSION"
	echo "Latest Runtime:    $VERSION"
	if [ "$INSTALLED_VERSION" = "$VERSION" ]; then
		echo "Runtime is up to date."
	else
		echo "Runtime update available. Rerun this installer without --check."
	fi
	exit 0
fi
case "$VERSION" in
*.*.*) ;;
*)
	echo "Invalid release version: $VERSION" >&2
	exit 1
	;;
esac

ASSET="sshxx-runtime-$VERSION-$TARGET.tar.gz"
BASE_URL="https://github.com/$REPOSITORY/releases/download/v$VERSION"
TEMP_DIR=$(mktemp -d)
cleanup_temp() {
	rm -rf "$TEMP_DIR"
}
trap cleanup_temp EXIT HUP INT TERM

echo "Downloading sshxx v$VERSION for $TARGET..."
curl -fsSL "$BASE_URL/$ASSET" -o "$TEMP_DIR/$ASSET"
curl -fsSL "$BASE_URL/SHA256SUMS" -o "$TEMP_DIR/SHA256SUMS"

EXPECTED=$(awk -v name="$ASSET" '$2 == name || $2 == "*" name { print $1; exit }' \
	"$TEMP_DIR/SHA256SUMS")
[ -n "$EXPECTED" ] || {
	echo "SHA256SUMS does not contain $ASSET" >&2
	exit 1
}

if command -v sha256sum >/dev/null 2>&1; then
	ACTUAL=$(sha256sum "$TEMP_DIR/$ASSET" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
	ACTUAL=$(shasum -a 256 "$TEMP_DIR/$ASSET" | awk '{ print $1 }')
else
	echo "A SHA-256 tool is required (sha256sum or shasum)." >&2
	exit 1
fi

[ "$ACTUAL" = "$EXPECTED" ] || {
	echo "Checksum verification failed for $ASSET" >&2
	exit 1
}

ARCHIVE_ROOT="sshxx-runtime-$VERSION-$TARGET"
mkdir -p "$TEMP_DIR/extract"
tar -xzf "$TEMP_DIR/$ASSET" -C "$TEMP_DIR/extract"
SOURCE_DIR="$TEMP_DIR/extract/$ARCHIVE_ROOT"
if [ ! -x "$SOURCE_DIR/bin/sshxx-daemon" ] ||
	[ ! -x "$SOURCE_DIR/bin/sshxx-terminal-host" ] ||
	[ ! -x "$SOURCE_DIR/bin/sshxx-server" ] ||
	[ ! -f "$SOURCE_DIR/build/spa.html" ]; then
	echo "Release archive is incomplete: $ASSET" >&2
	exit 1
fi
if [ "$MANAGED" = true ] &&
	{ [ ! -x "$SOURCE_DIR/scripts/install.sh" ] || [ ! -x "$SOURCE_DIR/scripts/service.sh" ]; }; then
	echo "Release v$VERSION predates managed installation support; install the latest v0.8.0+ Runtime." >&2
	exit 1
fi

VERSION_DIR="$INSTALL_ROOT/versions/$VERSION"
mkdir -p "$INSTALL_ROOT/versions" "$INSTALL_ROOT/bin" "$BIN_DIR"
if [ ! -d "$VERSION_DIR" ]; then
	mv "$SOURCE_DIR" "$VERSION_DIR"
elif [ ! -x "$VERSION_DIR/bin/sshxx-daemon" ] ||
	[ ! -x "$VERSION_DIR/bin/sshxx-terminal-host" ] ||
	[ ! -x "$VERSION_DIR/bin/sshxx-server" ] ||
	[ ! -f "$VERSION_DIR/build/spa.html" ]; then
	echo "Existing installation is incomplete: $VERSION_DIR" >&2
	echo "Move it aside and rerun the installer." >&2
	exit 1
fi
if [ "$MANAGED" = true ] &&
	{ [ ! -x "$VERSION_DIR/scripts/install.sh" ] || [ ! -x "$VERSION_DIR/scripts/service.sh" ]; }; then
	echo "Installed Runtime v$VERSION does not support managed services." >&2
	exit 1
fi
printf '%s\n' "$VERSION" >"$INSTALL_ROOT/current-version"

cat >"$INSTALL_ROOT/bin/sshxx-launcher" <<'EOF'
#!/bin/sh
set -eu

COMMAND=${0##*/}
SCRIPT_PATH=$0
while [ -L "$SCRIPT_PATH" ]; do
	SCRIPT_DIRECTORY=$(CDPATH= cd -- "$(dirname -- "$SCRIPT_PATH")" && pwd)
	LINK_TARGET=$(readlink "$SCRIPT_PATH")
	case "$LINK_TARGET" in
	/*) SCRIPT_PATH=$LINK_TARGET ;;
	*) SCRIPT_PATH=$SCRIPT_DIRECTORY/$LINK_TARGET ;;
	esac
done

ROOT=$(CDPATH= cd -- "$(dirname -- "$SCRIPT_PATH")/.." && pwd)
VERSION=$(cat "$ROOT/current-version")
case "$COMMAND" in
sshxx-daemon | sshxx-terminal-host)
	exec "$ROOT/versions/$VERSION/bin/$COMMAND" "$@"
	;;
sshxx-server)
	cd "$ROOT/versions/$VERSION"
	exec ./bin/sshxx-server "$@"
	;;
sshxx-service)
	export SSHXX_INSTALL_ROOT=$ROOT
	exec "$ROOT/versions/$VERSION/scripts/service.sh" "$@"
	;;
*)
	echo "Unsupported sshxx command: $COMMAND" >&2
	exit 1
	;;
esac
EOF

chmod 755 "$INSTALL_ROOT/bin/sshxx-launcher"
ln -sfn sshxx-launcher "$INSTALL_ROOT/bin/sshxx-daemon"
ln -sfn sshxx-launcher "$INSTALL_ROOT/bin/sshxx-terminal-host"
ln -sfn sshxx-launcher "$INSTALL_ROOT/bin/sshxx-server"
if [ -x "$VERSION_DIR/scripts/service.sh" ]; then
	ln -sfn sshxx-launcher "$INSTALL_ROOT/bin/sshxx-service"
fi
link_command() {
	source=$1
	destination=$2
	if [ -e "$destination" ] || [ -L "$destination" ]; then
		if [ ! -L "$destination" ] || [ "$(readlink "$destination")" != "$source" ]; then
			echo "Refusing to replace an existing command: $destination" >&2
			exit 1
		fi
	fi
	ln -sfn "$source" "$destination"
}

link_command "$INSTALL_ROOT/bin/sshxx-daemon" "$BIN_DIR/sshxx-daemon"
link_command "$INSTALL_ROOT/bin/sshxx-terminal-host" "$BIN_DIR/sshxx-terminal-host"
link_command "$INSTALL_ROOT/bin/sshxx-server" "$BIN_DIR/sshxx-server"
if [ -x "$VERSION_DIR/scripts/service.sh" ]; then
	link_command "$INSTALL_ROOT/bin/sshxx-service" "$BIN_DIR/sshxx-service"
fi

echo "Installed sshxx v$VERSION in $VERSION_DIR"
echo "Commands are available in $BIN_DIR"
case ":$PATH:" in
*:"$BIN_DIR":*) ;;
*) echo "Add $BIN_DIR to PATH before opening a new terminal." ;;
esac

if [ "$MANAGED" = true ]; then
	SSHXX_INSTALL_ROOT=$INSTALL_ROOT SSHXX_BIN_DIR=$BIN_DIR \
		"$VERSION_DIR/scripts/service.sh" install \
		--workspace "$WORKSPACE" --scope "$SCOPE" \
		--listen "$LISTEN" --port "$PORT" --bin-dir "$BIN_DIR"
	echo "Verify the managed Runtime with: sshxx-service status"
	exit 0
fi

if [ "$RUN_AFTER_INSTALL" != true ]; then
	echo "Next, start the Runtime in two terminals:"
	echo "  1. sshxx-server --listen 127.0.0.1"
	echo "  2. cd <workspace-directory>"
	echo "     sshxx-daemon --server http://127.0.0.1:8051"
	echo "The daemon starts sshxx-terminal-host automatically and prints the session URL."
	if [ -x "$VERSION_DIR/scripts/service.sh" ]; then
		echo "For managed auto-start instead, rerun with --managed."
	fi
	exit 0
fi

cleanup_temp
trap - EXIT HUP INT TERM

echo "Starting a local sshxx server on http://127.0.0.1:8051..."
(cd "$VERSION_DIR" && exec ./bin/sshxx-server --listen 127.0.0.1) &
SERVER_PID=$!
cleanup_server() {
	kill "$SERVER_PID" 2>/dev/null || true
	wait "$SERVER_PID" 2>/dev/null || true
}
trap cleanup_server EXIT HUP INT TERM

attempt=0
until curl -fsS http://127.0.0.1:8051/ >/dev/null 2>&1; do
	if ! kill -0 "$SERVER_PID" 2>/dev/null; then
		wait "$SERVER_PID" || true
		echo "sshxx-server exited before becoming ready; is port 8051 already in use?" >&2
		exit 1
	fi
	attempt=$((attempt + 1))
	if [ "$attempt" -ge 50 ]; then
		echo "sshxx-server did not become ready." >&2
		exit 1
	fi
	sleep 0.1
done

echo "Starting sshxx-daemon; its local data will be stored in $(pwd)."
"$VERSION_DIR/bin/sshxx-daemon" --server http://127.0.0.1:8051
