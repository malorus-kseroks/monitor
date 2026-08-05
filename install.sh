#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./install.sh [--user|--system] [--binary PATH] [--prefix PATH] [--rollback]

Installs an already-built kernox-monitor binary. This script never downloads
or bootstraps Rust. Use `cargo build --release --locked` first when installing
from source.
EOF
}

mode=user
rollback=false
binary="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/target/release/kernox-monitor"
prefix=""
while (($#)); do
  case "$1" in
    --user) mode=user ;;
    --system) mode=system ;;
    --binary) shift; binary=${1:?missing path after --binary} ;;
    --prefix) shift; prefix=${1:?missing path after --prefix} ;;
    --rollback) rollback=true ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [[ -z "$prefix" ]]; then
  if [[ "$mode" == system ]]; then prefix=/usr/local; else prefix=${XDG_BIN_HOME:-"$HOME/.local"}; fi
fi
destination="$prefix/bin/kernox-monitor"
mkdir_command=(mkdir -p "$(dirname "$destination")")
install_command=(install -m 0755 "$binary" "$destination.tmp")
move_command=(mv -f "$destination.tmp" "$destination")

runner=()
if [[ "$mode" == system && $EUID -ne 0 ]]; then
  command -v sudo >/dev/null 2>&1 || { printf 'System install requires root or sudo.\n' >&2; exit 1; }
  runner=(sudo --)
fi
if [[ "$rollback" == true ]]; then
  [[ -f "$destination.bak" ]] || { printf 'No rollback copy: %s\n' "$destination.bak" >&2; exit 1; }
  "${runner[@]}" cp -p -- "$destination.bak" "$destination.tmp"
  "${runner[@]}" mv -f -- "$destination.tmp" "$destination"
  "$destination" --version >/dev/null
  printf 'Rolled back %s\n' "$destination"
  exit 0
fi
[[ -f "$binary" ]] || { printf 'Binary not found: %s\n' "$binary" >&2; exit 1; }
"${runner[@]}" "${mkdir_command[@]}"
if [[ -e "$destination" ]]; then "${runner[@]}" cp -p -- "$destination" "$destination.bak"; fi
"${runner[@]}" "${install_command[@]}"
"${runner[@]}" "${move_command[@]}"
if ! "$destination" --version >/dev/null; then
  printf 'Installed binary failed verification; restoring previous state.\n' >&2
  if [[ -f "$destination.bak" ]]; then
    "${runner[@]}" cp -p -- "$destination.bak" "$destination"
  else
    "${runner[@]}" rm -f -- "$destination"
  fi
  exit 1
fi
printf 'Installed %s\n' "$destination"
case ":$PATH:" in
  *":$(dirname "$destination"):"*) ;;
  *) printf 'Add %s to PATH.\n' "$(dirname "$destination")" ;;
esac
