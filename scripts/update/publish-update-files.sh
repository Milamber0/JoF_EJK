#!/usr/bin/env bash
# Builds, signs and attaches the in-client updater's files to a published release
# (docs/updater.md). build.yml runs it once the release exists:
#
#   publish-update-files.sh <tag> <stable|beta> <artifacts dir>
#
# <artifacts dir>/<platform>/GameData holds each Windows build. Needs GH_TOKEN and
# UPDATE_SIGNING_KEY, the Ed25519 private key (PEM) of the release-signing
# environment. Without the key the release simply gets no update files.
#
# The manifest's remove list comes from the newest earlier releases' manifests
# (PREVIOUS_RELEASES of them, 100 by default). For testing, PREVIOUS_MANIFESTS
# names a folder of <tag>/JoF-update-<platform>.json(.sig) to use instead.
set -euo pipefail

TAG=$1
CHANNEL=$2
ARTIFACTS=$3
HERE=$(cd "$(dirname "$0")" && pwd)

if [ -z "${UPDATE_SIGNING_KEY:-}" ]; then
  echo "::warning::UPDATE_SIGNING_KEY is not set, so $TAG gets no update files and /update will not offer it."
  exit 0
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
KEY="$WORK/signing.pem"
(umask 077 && printf '%s\n' "$UPDATE_SIGNING_KEY" > "$KEY")

# Clients only accept the keys in update-signing-key.pub: never sign with another.
ACTUAL=$(openssl pkey -in "$KEY" -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n')
if ! grep -E '^[0-9a-fA-F]{64}$' "$HERE/update-signing-key.pub" | tr 'A-F' 'a-f' | grep -qx "$ACTUAL"; then
  echo "::error::UPDATE_SIGNING_KEY is not one of the keys in scripts/update/update-signing-key.pub"
  exit 1
fi
openssl pkey -in "$KEY" -pubout -out "$WORK/public.pem"

# Earlier releases' manifests: the files they shipped that this release no longer
# has are removed from players' GameData. Anyone who can push can replace a
# release's files, so only manifests signed with a key clients accept count; a
# forged one must never get a file deleted.
PREVIOUS="$WORK/previous"
VERIFIED="$WORK/verified"
mkdir -p "$PREVIOUS" "$VERIFIED"
if [ -n "${PREVIOUS_MANIFESTS:-}" ]; then
  cp -R "$PREVIOUS_MANIFESTS"/. "$PREVIOUS"/
else
  : > "$WORK/tags"
  : > "$WORK/previous-tags"
  if ! gh api --paginate "repos/$GITHUB_REPOSITORY/releases?per_page=100" \
      --jq '.[] | select(.draft | not) | select(any(.assets[]; .name | startswith("JoF-update-"))) | .tag_name' > "$WORK/tags"; then
    echo "::warning::could not list the earlier releases, so $TAG removes no files"
  fi
  { grep -E '^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[0-9]+)?$' "$WORK/tags" | grep -vxF "$TAG" | head -n "${PREVIOUS_RELEASES:-100}" > "$WORK/previous-tags"; } || true
  while read -r OLD; do
    mkdir -p "$PREVIOUS/$OLD"
    gh release download "$OLD" --repo "$GITHUB_REPOSITORY" --dir "$PREVIOUS/$OLD" \
      --pattern 'JoF-update-*.json' --pattern 'JoF-update-*.json.sig' \
      || echo "::warning::could not fetch the update manifests of $OLD"
  done < "$WORK/previous-tags"
fi
TRUSTED=0
for HEX in $(grep -E '^[0-9a-fA-F]{64}$' "$HERE/update-signing-key.pub" | grep -vE '^0{64}$'); do
  python3 -I -c 'import sys; sys.stdout.buffer.write(bytes.fromhex("302a300506032b6570032100" + sys.argv[1]))' "$HEX" > "$WORK/trusted-$TRUSTED.der"
  openssl pkey -pubin -inform DER -in "$WORK/trusted-$TRUSTED.der" -out "$WORK/trusted-$TRUSTED.pem"
  TRUSTED=$((TRUSTED + 1))
done
for MANIFEST in "$PREVIOUS"/*/JoF-update-*.json; do
  [ -f "$MANIFEST" ] || continue
  OLD=$(basename "$(dirname "$MANIFEST")")
  GOOD=
  for TRUSTED_PEM in "$WORK"/trusted-*.pem; do
    if [ -f "$MANIFEST.sig" ] && openssl pkeyutl -verify -rawin -pubin -inkey "$TRUSTED_PEM" -in "$MANIFEST" -sigfile "$MANIFEST.sig" >/dev/null 2>&1; then
      GOOD=1
      break
    fi
  done
  if [ -n "$GOOD" ]; then
    mkdir -p "$VERIFIED/$OLD"
    cp "$MANIFEST" "$VERIFIED/$OLD/"
  else
    echo "::warning::$OLD's $(basename "$MANIFEST") is not signed with a listed key, so it removes nothing"
  fi
done

OUT="$WORK/out"
for PLATFORM in windows-x86 windows-x86_64; do
  python3 "$HERE/make_update_package.py" --gamedata "$ARTIFACTS/$PLATFORM/GameData" --platform "$PLATFORM" \
    --tag "$TAG" --channel "$CHANNEL" --commit "${GITHUB_SHA:-}" \
    --previous "$VERIFIED" --retired "$HERE/retired-files.json" --output "$OUT"
  MANIFEST="$OUT/JoF-update-$PLATFORM.json"
  openssl pkeyutl -sign -rawin -inkey "$KEY" -in "$MANIFEST" -out "$MANIFEST.sig"
  openssl pkeyutl -verify -rawin -pubin -inkey "$WORK/public.pem" -in "$MANIFEST" -sigfile "$MANIFEST.sig"
done

if [ -n "${DRY_RUN:-}" ]; then
  mkdir -p "$DRY_RUN"
  cp "$OUT"/* "$DRY_RUN"/
  echo "DRY_RUN: wrote the update files to $DRY_RUN instead of uploading them."
  exit 0
fi
# The manifests go last: until one is up, clients treat the release as having no
# update files rather than a broken one.
for FILES in "$OUT"/*.zip "$OUT"/*.json.sig "$OUT"/*.json; do
  gh release upload "$TAG" "$FILES" --clobber --repo "$GITHUB_REPOSITORY"
done
