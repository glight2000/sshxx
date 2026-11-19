#!/bin/sh
# Start the existing development processes with project-pinned tools.
set -eu
cd -- "$(dirname -- "$0")"
if ! command -v mise >/dev/null 2>&1; then
  echo 'mise is required; see README.md for development setup.' >&2
  exit 1
fi
if [ "${1:-}" = build-runtime ]; then
  if [ "$#" -ne 2 ]; then
    echo 'Usage: ./start.sh build-runtime /absolute/path/to/new-build' >&2
    exit 2
  fi
  case $2 in /*) ;; *) echo 'Use an absolute output path.' >&2; exit 2 ;; esac
  output=$2
  if [ -e "$output" ] || [ -L "$output" ]; then
    echo 'Choose a new output directory for this build.' >&2
    exit 1
  fi
  cache=${XDG_CACHE_HOME:-"$HOME/.cache"}/sshxx
  target=${CARGO_TARGET_DIR:-"$cache/source-target"}
  export npm_config_cache=${npm_config_cache:-"$cache/npm"}
  mkdir -p -- "$cache" "$(dirname -- "$output")"
  # npm ci and Vite both replace checkout-local files.
  exec 9>"$cache/build-runtime.lock"
  flock -x 9
  mise exec -- npm ci
  mise exec -- npm run build
  mise exec -- cargo build --locked --release --target-dir "$target" \
    -p sshxx-server --bin sshxx-server -p sshxx-daemon --bin sshxx-daemon

  stage=$(mktemp -d -- "$(dirname -- "$output")/.sshxx-build.XXXXXXXX")
  trap 'rm -rf -- "$stage"' EXIT
  trap 'exit 1' HUP INT TERM
  test -s build/spa.html
  mkdir "$stage/bin"
  cp -R build "$stage/build"
  for name in sshxx-server sshxx-daemon; do
    install -m 755 "$target/release/$name" "$stage/bin/$name"
    "$stage/bin/$name" --version
  done
  # Publish the complete directory on the destination filesystem.
  mv -nT -- "$stage" "$output"
  if [ -d "$stage" ]; then
    echo 'Another build already occupies the output path.' >&2
    exit 1
  fi
  trap - EXIT HUP INT TERM
  printf 'Built source runtime in %s\n' "$output"
  exit 0
fi
exec mise exec -- dekit mprocs --config mprocs.yaml --log-dir logs/dev "$@"
