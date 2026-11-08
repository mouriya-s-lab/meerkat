#!/bin/sh
# Builds Meerkat.app into <dir> and lets it load plain http:// addresses.
# The council, guardians and dispatch addresses in the config's
# targets.macos are the operator's HTTP endpoints; App Transport Security
# blocks them in a bundled app unless its Info.plist allows them. nsh 0.4.0
# has no nsh.json key for Info.plist entries, so the entry is added after
# packaging and the bundle is signed again (ad hoc, as nsh signs it).
# Usage: deploy/package-macos.sh <dir>
set -eu
out=${1:?usage: deploy/package-macos.sh <dir>}
cd "$(dirname "$0")/.."
PATH=/opt/homebrew/opt/llvm/bin:$PATH nsh package --target macos -o "$out"
app=$out/Meerkat.app
plutil -insert NSAppTransportSecurity -json '{"NSAllowsArbitraryLoads":true}' "$app/Contents/Info.plist"
codesign --force --sign - "$app"
