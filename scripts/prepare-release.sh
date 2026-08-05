#!/bin/sh
set -eu

version=${1:?usage: prepare-release.sh VERSION [DIST]}
dist=${2:-dist}
case "$version" in
  *[!0-9.]*|'') echo "version must contain only digits and dots" >&2; exit 2 ;;
esac

command -v cargo >/dev/null 2>&1 || { echo "cargo is required" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "sha256sum is required" >&2; exit 1; }
mkdir -p "$dist"

targets="x86_64-unknown-linux-gnu x86_64-unknown-linux-musl aarch64-unknown-linux-gnu aarch64-unknown-linux-musl"
for target in $targets; do
  cargo build --release --locked --target "$target"
  stage="$dist/kernox-monitor-$version-$target"
  rm -rf -- "$stage"
  mkdir -p "$stage"
  install -m 0755 "target/$target/release/kernox-monitor" "$stage/kernox-monitor"
  install -m 0644 LICENSE README.md "$stage/"
  tar -C "$dist" -czf "$stage.tar.gz" "$(basename "$stage")"
  rm -rf -- "$stage"
done

if command -v cargo-cyclonedx >/dev/null 2>&1; then
  cargo cyclonedx --format json --output-cdx
  cp kernox-monitor.cdx.json "$dist/kernox-monitor-$version.cdx.json"
else
  echo "cargo-cyclonedx is required for release SBOM generation" >&2
  exit 1
fi

(cd "$dist" && sha256sum ./*.tar.gz ./*.cdx.json > SHA256SUMS)

if [ -n "${MINISIGN_SECRET_KEY_FILE:-}" ]; then
  command -v minisign >/dev/null 2>&1 || { echo "minisign is required" >&2; exit 1; }
  minisign -S -s "$MINISIGN_SECRET_KEY_FILE" -m "$dist/SHA256SUMS"
else
  echo "MINISIGN_SECRET_KEY_FILE is unset; artifacts were not signed" >&2
fi
