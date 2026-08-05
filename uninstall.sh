#!/usr/bin/env bash
set -euo pipefail
mode=user
prefix=""
while (($#)); do
  case "$1" in
    --user) mode=user ;;
    --system) mode=system ;;
    --prefix) shift; prefix=${1:?missing path after --prefix} ;;
    -h|--help) printf 'Usage: ./uninstall.sh [--user|--system] [--prefix PATH]\n'; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done
[[ -n "$prefix" ]] || { [[ "$mode" == system ]] && prefix=/usr/local || prefix=${XDG_BIN_HOME:-"$HOME/.local"}; }
target="$prefix/bin/kernox-monitor"
runner=()
if [[ "$mode" == system && $EUID -ne 0 ]]; then runner=(sudo --); fi
[[ -e "$target" ]] || { printf 'Not installed: %s\n' "$target"; exit 0; }
"${runner[@]}" rm -- "$target"
printf 'Removed %s\n' "$target"
