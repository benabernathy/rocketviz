#!/usr/bin/env bash
# Signs every Mach-O file in the bundled Java runtime for notarization: hardened runtime, secure timestamp, and the
# JIT entitlements on executables. Run before `tauri build`, which copies resources as they are and signs only the app.
#
#   scripts/macos-sign-runtime.sh "Developer ID Application: Name (TEAMID)" engines/openrocket/build/jre
#   scripts/macos-sign-runtime.sh - engines/openrocket/build/jre      # ad-hoc, for local testing
set -euo pipefail
identity="$1"
runtime="$2"
entitlements="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/macos/java.entitlements"
timestamp=(--timestamp)
[[ "$identity" == "-" ]] && timestamp=()   # ad-hoc signatures can't be timestamped

# codesign reports every re-signed file on stderr; keep only real problems.
quiet() { "$@" 2> >(grep -v "replacing existing signature" >&2); }

count=0
# Libraries first, then executables (inside out).
while IFS= read -r -d '' f; do
  if file -b "$f" | grep -q "Mach-O"; then
    if file -b "$f" | grep -q "executable"; then
      quiet codesign --force --options runtime "${timestamp[@]}" --entitlements "$entitlements" --sign "$identity" "$f"
    else
      quiet codesign --force --options runtime "${timestamp[@]}" --sign "$identity" "$f"
    fi
    count=$((count + 1))
  fi
done < <(find "$runtime" -type f \( -name "*.dylib" -print0 \) ; find "$runtime" -type f ! -name "*.dylib" -print0)
echo "Signed $count Mach-O files in $runtime"
