#!/usr/bin/env bash
#
# rend2-sync.sh — port new rend2 commits from SomaZ/OpenJK (rend2-unified-wip)
# into this repo.
#
# This repo mirrors upstream's unified SP/MP layout, so patches apply with
# their paths unchanged:
#
#   shared/rd-rend2/**  ->  shared/rd-rend2/** (renderer core, as-is)
#   codemp/rd-rend2/**  ->  codemp/rd-rend2/** (MP wrapper, as-is)
#   code/rd-rend2/**    ->  dropped (SP-only wrapper; this repo has no SP)
#
# SP-specific code inside shared/rd-rend2 is guarded by REND2_SP and is
# carried along unchanged.
#
# State: scripts/rend2-sync/last-synced-commit holds the last upstream commit
# that was processed. Commits are applied oldest-first; the script stops at
# the first conflict so later commits are never applied out of order. Fix the
# conflicting commit by hand, write its hash into the state file, commit, and
# re-run to continue.
#
# Usage: scripts/rend2-sync/rend2-sync.sh
#   UPSTREAM_URL / UPSTREAM_BRANCH env vars override the default upstream.
#   Requires a clean working tree. Creates commits on the CURRENT branch.
#
set -euo pipefail

UPSTREAM_URL="${UPSTREAM_URL:-https://github.com/SomaZ/OpenJK.git}"
UPSTREAM_BRANCH="${UPSTREAM_BRANCH:-rend2-unified-wip}"
STATE_FILE="scripts/rend2-sync/last-synced-commit"
# Paths ported from upstream (shared/KHR holds the khrplatform.h glext.h needs).
PORT_PATHS="shared/rd-rend2 codemp/rd-rend2 shared/KHR"
SUMMARY_FILE="${SUMMARY_FILE:-}"

cd "$(git rev-parse --show-toplevel)"

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "error: working tree not clean" >&2
    exit 1
fi

echo "Fetching $UPSTREAM_URL $UPSTREAM_BRANCH ..."
git fetch --quiet "$UPSTREAM_URL" "$UPSTREAM_BRANCH"
UPSTREAM_HEAD=$(git rev-parse FETCH_HEAD)

LAST=$(tr -d ' \n' < "$STATE_FILE")
if [ -z "$LAST" ]; then
    echo "error: $STATE_FILE is empty; seed it with an upstream commit hash" >&2
    exit 1
fi

if ! git merge-base --is-ancestor "$LAST" "$UPSTREAM_HEAD"; then
    echo "error: state commit $LAST is not an ancestor of upstream HEAD." >&2
    echo "Upstream branch was likely rebased; re-baseline $STATE_FILE by hand." >&2
    exit 1
fi

COMMITS=$(git rev-list --reverse "$LAST..$UPSTREAM_HEAD" -- $PORT_PATHS)

if [ -z "$COMMITS" ]; then
    echo "Up to date with upstream ($UPSTREAM_HEAD); nothing to do."
    exit 0
fi

applied=()
sp_only=()
already_present=()
conflict=""
new_state="$LAST"

for c in $COMMITS; do
    subject=$(git log -1 --format='%h %s' "$c")

    # Patch limited to the paths we port; layout matches upstream, so no
    # path rewriting is needed.
    patch=$(git show --binary "$c" -- $PORT_PATHS)

    if ! printf '%s\n' "$patch" | grep -q '^diff --git'; then
        echo "SKIP (SP-only)  $subject"
        sp_only+=("$subject")
        new_state="$c"
        continue
    fi

    if printf '%s\n' "$patch" | git apply --3way --binary --quiet 2>/dev/null; then
        git add -A $PORT_PATHS
        # A 3-way apply of a commit whose changes we already have succeeds
        # but changes nothing; skip it rather than failing on an empty commit.
        if git diff --cached --quiet; then
            echo "SKIP (present)  $subject"
            already_present+=("$subject")
            new_state="$c"
            continue
        fi
        git commit --quiet \
            --author "$(git log -1 --format='%an <%ae>' "$c")" \
            -m "$(git log -1 --format=%B "$c")" \
            -m "(ported from SomaZ/OpenJK $c)"
        echo "APPLIED         $subject"
        applied+=("$subject")
        new_state="$c"
    else
        git reset --hard --quiet HEAD
        # If the patch reverse-applies, the tree already contains this
        # commit's end state (e.g. upstream removing REND2_SP scaffolding
        # this repo never had) — skip it instead of reporting a conflict.
        if printf '%s\n' "$patch" | git apply --reverse --check --binary 2>/dev/null; then
            echo "SKIP (present)  $subject"
            already_present+=("$subject")
            new_state="$c"
            continue
        fi
        echo "CONFLICT        $subject"
        conflict="$subject"
        break
    fi
done

if [ "$new_state" != "$LAST" ]; then
    echo "$new_state" > "$STATE_FILE"
    git add "$STATE_FILE"
    git commit --quiet -m "rend2-sync: advance upstream state to ${new_state:0:8}"
fi

remaining=0
if [ -n "$conflict" ]; then
    remaining=$(git rev-list --count "$new_state..$UPSTREAM_HEAD" -- $PORT_PATHS)
fi

echo
echo "=== rend2-sync summary ==="
echo "applied:  ${#applied[@]}"
echo "sp-only:  ${#sp_only[@]}"
echo "present:  ${#already_present[@]}"
if [ -n "$conflict" ]; then
    echo "stopped at conflict: $conflict ($remaining upstream commit(s) still pending)"
    echo "port it by hand, set $STATE_FILE to its full hash, commit, re-run."
fi

if [ -n "$SUMMARY_FILE" ]; then
    {
        echo "Automated port of rend2 commits from [SomaZ/OpenJK \`$UPSTREAM_BRANCH\`](https://github.com/SomaZ/OpenJK/tree/$UPSTREAM_BRANCH)."
        echo "\`shared/rd-rend2\` and \`codemp/rd-rend2\` are ported as-is; SP-only (\`code/rd-rend2\`) changes are dropped."
        echo
        if [ ${#applied[@]} -gt 0 ]; then
            echo "### Applied"
            for s in "${applied[@]}"; do echo "- $s"; done
            echo
        fi
        if [ ${#sp_only[@]} -gt 0 ]; then
            echo "### Skipped (SP-only)"
            for s in "${sp_only[@]}"; do echo "- $s"; done
            echo
        fi
        if [ ${#already_present[@]} -gt 0 ]; then
            echo "### Skipped (already present)"
            for s in "${already_present[@]}"; do echo "- $s"; done
            echo
        fi
        if [ -n "$conflict" ]; then
            echo "### :warning: Stopped at conflict"
            echo "- $conflict"
            echo
            echo "$remaining upstream commit(s) remain after this one. Port it manually,"
            echo "update \`$STATE_FILE\` to its full hash, commit, and re-run the workflow."
        fi
    } > "$SUMMARY_FILE"
fi

if [ -n "${GITHUB_OUTPUT:-}" ]; then
    {
        echo "applied=${#applied[@]}"
        echo "conflict=$conflict"
    } >> "$GITHUB_OUTPUT"
fi
