#!/bin/sh
# Builds Meerkat.app from this checkout and installs it as
# ~/Applications/Meerkat.app for this Mac. The app reads the Meerkat config
# checkout MEERKAT_CONFIG_DIR names and asks the `gh` on its PATH for every
# GitHub request; an app opened from Finder gets neither from a shell, so
# both go into the bundle's LSEnvironment and the bundle is signed again
# (ad hoc, as nsh signs it).
# Usage: deploy/install-macos.sh <meerkat-config checkout>
set -eu
usage='usage: deploy/install-macos.sh <meerkat-config checkout>'
config=$(cd "${1:?$usage}" && pwd)
gh=$(command -v gh) || { echo "no gh on PATH" >&2; exit 1; }
cd "$(dirname "$0")/.."
out=$(mktemp -d /tmp/meerkat-macos.XXXXXX)
PATH=/opt/homebrew/opt/llvm/bin:$PATH nsh package --target macos -o "$out"
app=$out/Meerkat.app
plutil -insert LSEnvironment -json \
  "{\"MEERKAT_CONFIG_DIR\":\"$config\",\"PATH\":\"$(dirname "$gh"):/usr/bin:/bin:/usr/sbin:/sbin\"}" \
  "$app/Contents/Info.plist"
codesign --force --sign - "$app"
mkdir -p "$HOME/Applications"
rsync -a --delete "$app/" "$HOME/Applications/Meerkat.app/"
echo "installed $HOME/Applications/Meerkat.app from $(git rev-parse --short HEAD)"
