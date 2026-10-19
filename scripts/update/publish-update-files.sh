#!/usr/bin/env bash
# Builds, signs and attaches the in-client updater's files to a published release
# (docs/updater.md). build.yml runs it once the release exists:
#
#   publish-update-files.sh <tag> <stable|beta> <artifacts dir>
#
# <artifacts dir>/<platform>/GameData holds each Windows build. Needs GH_TOKEN and
# UPDATE_SIGNING_KEY, the Ed25519 private key (PEM) of the release-signing
# environment. Without the key the release simply gets no update files.
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

OUT="$WORK/out"
for PLATFORM in windows-x86 windows-x86_64; do
  python3 "$HERE/make_update_package.py" --gamedata "$ARTIFACTS/$PLATFORM/GameData" --platform "$PLATFORM" \
    --tag "$TAG" --channel "$CHANNEL" --commit "${GITHUB_SHA:-}" --output "$OUT"
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
