#!/usr/bin/env bash
# Puts Google's adb (SDK Platform-Tools, pinned + checksummed) next to the app for `tauri build`
# (docs/adb-sidecar.md). Release CI only; nothing it downloads is committed (app/src-tauri/binaries is ignored).
#   scripts/fetch-adb.sh <rust target triple> <out config json>
# Writes the Tauri config fragment (bundle.externalBin, and bundle.resources for the Windows DLLs) to <out>.
set -euo pipefail
triple="${1:?target triple}"
out="${2:?output config json}"
root="$(cd "$(dirname "$0")/.." && pwd)"
bin="$root/app/src-tauri/binaries"

version=37.0.1
case "$triple" in
  *-windows-*) os=win; sha=45f4d63113e895ebde0c90f194099a4676b6ac653bd28d54314a9e022bbc1a99; exe=.exe ;;
  *-apple-darwin) os=darwin; sha=ee39ad5967e95c2a07f04dbcbde96b1a0c916ba376096db5d2f498b7727a5d1d; exe= ;;
  *-linux-*) os=linux; sha=d230f13842f60f782a8645f9c813f8f845bf36089ea7289f28c48f17979313f1; exe= ;;
  *) echo "unsupported target: $triple" >&2; exit 1 ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
zip="$tmp/platform-tools.zip"
curl -fsSL --retry 3 -o "$zip" "https://dl.google.com/android/repository/platform-tools_r${version}-${os}.zip"
if command -v sha256sum >/dev/null; then got=$(sha256sum "$zip" | cut -d' ' -f1); else got=$(shasum -a 256 "$zip" | cut -d' ' -f1); fi
[ "$got" = "$sha" ] || { echo "platform-tools checksum mismatch: $got" >&2; exit 1; }
if command -v unzip >/dev/null; then unzip -q "$zip" -d "$tmp"; else 7z x -y -o"$tmp" "$zip" >/dev/null; fi

mkdir -p "$bin"
cp "$tmp/platform-tools/adb$exe" "$bin/adb-$triple$exe"
chmod +x "$bin/adb-$triple$exe"
# The Platform-Tools license notice ships with the binary.
cp "$tmp/platform-tools/NOTICE.txt" "$bin/adb-NOTICE.txt"

resources='{"binaries/adb-NOTICE.txt":"adb-NOTICE.txt",'
if [ "$os" = win ]; then
  # adb.exe loads these from its own folder; Tauri resources land next to the exe on Windows.
  for dll in "$tmp"/platform-tools/*.dll; do
    name="$(basename "$dll")"
    cp "$dll" "$bin/$name"
    resources="$resources\"binaries/$name\":\"$name\","
  done
fi
resources="${resources%,}}"
printf '{"bundle":{"externalBin":["binaries/adb"],"resources":%s}}\n' "$resources" > "$out"
echo "adb $version for $triple:"; ls -la "$bin"; cat "$out"
