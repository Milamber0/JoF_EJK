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
# Only upstream's first-parent history (SomaZ's own commits and his merge
# commits) is walked. When he merges JACoders/OpenJK master in, the merged-in
# commits still use the old codemp/rd-rend2/tr_*.cpp layout and can never
# apply here; instead each merge is ported as one patch: its net change
# against the first parent, which upstream already resolved into the new
# layout.
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
# Files a ported commit touches outside PORT_PATHS are not ported; SP-only
# code/ and codeJK2/ paths are dropped on purpose and not reported.
OUTSIDE_RE="^($(printf '%s' "$PORT_PATHS" | tr ' ' '|')|code|codeJK2)/"

# First-parent upstream commits after $1 whose net change (against their first
# parent) touches PORT_PATHS, oldest first. A merge commit's net change is the
# merged-in work as upstream resolved it, so merges are handled like any
# other commit.
list_commits() {
    local c
    for c in $(git rev-list --reverse --first-parent "$1..$UPSTREAM_HEAD"); do
        if ! git diff --quiet "$c^1" "$c" -- $PORT_PATHS; then
            echo "$c"
        fi
    done
}

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

COMMITS=$(list_commits "$LAST")

if [ -z "$COMMITS" ]; then
    echo "Up to date with upstream ($UPSTREAM_HEAD); nothing to do."
    exit 0
fi

applied=()
sp_only=()
already_present=()
outside=()
conflict=""
new_state="$LAST"

note_outside() {
    if [ -n "$2" ]; then
        echo "  ! also touches (not ported): $2"
        outside+=("$1 — not ported: \`${2% }\`")
    fi
}

for c in $COMMITS; do
    subject=$(git log -1 --format='%h %s' "$c")
    # Engine-side files (e.g. shared/sdl, codemp/rd-common) this commit also
    # changes. They are not ported, so list them for a human to port by hand.
    others=$(git diff --name-only "$c^1" "$c" | grep -v -E "$OUTSIDE_RE" | tr '\n' ' ' || true)

    # Net change against the first parent, limited to the paths we port. For
    # an ordinary commit this is its own diff; for a merge it is everything
    # the merge brought in. Layout matches upstream, so no path rewriting.
    patch=$(git diff --binary "$c^1" "$c" -- $PORT_PATHS)

    # Feed grep from a here-string, not a pipe: with pipefail, grep -q exiting
    # on the first match SIGPIPEs printf on large patches (>64 KB) and the
    # commit would be misreported as SP-only and silently skipped.
    if ! grep -q '^diff --git' <<<"$patch"; then
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
            note_outside "$subject" "$others"
            new_state="$c"
            continue
        fi
        extra_msg=()
        if git rev-parse --quiet --verify "$c^2" >/dev/null; then
            # Merge: list the merged-in commits that touched rend2.
            merged=$(git log --format='- %h %s' "$c^1..$c^2" -- $PORT_PATHS || true)
            if [ -n "$merged" ]; then
                extra_msg=(-m "Includes:"$'\n'"$merged")
            fi
        fi
        git commit --quiet \
            --author "$(git log -1 --format='%an <%ae>' "$c")" \
            -m "$(git log -1 --format=%B "$c")" \
            "${extra_msg[@]}" \
            -m "(ported from SomaZ/OpenJK $c)"
        echo "APPLIED         $subject"
        applied+=("$subject")
        note_outside "$subject" "$others"
        new_state="$c"
    else
        git reset --hard --quiet HEAD
        # If the patch reverse-applies, the tree already contains this
        # commit's end state (e.g. upstream removing REND2_SP scaffolding
        # this repo never had) — skip it instead of reporting a conflict.
        if printf '%s\n' "$patch" | git apply --reverse --check --binary 2>/dev/null; then
            echo "SKIP (present)  $subject"
            already_present+=("$subject")
            note_outside "$subject" "$others"
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
    remaining=$(list_commits "$new_state" | wc -l | tr -d " ")
fi

echo
echo "=== rend2-sync summary ==="
echo "applied:  ${#applied[@]}"
echo "sp-only:  ${#sp_only[@]}"
echo "present:  ${#already_present[@]}"
echo "outside:  ${#outside[@]} commit(s) also touch files outside the ported paths"
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
        if [ ${#outside[@]} -gt 0 ]; then
            echo "### :warning: Engine-side changes not ported"
            echo "These commits also change files outside \`$PORT_PATHS\`. Only their"
            echo "renderer part was ported; port the rest by hand if rend2 depends on it."
            for s in "${outside[@]}"; do echo "- $s"; done
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
        echo "outside=${#outside[@]}"
    } >> "$GITHUB_OUTPUT"
fi
