#!/usr/bin/env bash
# Merges a PR only when every CI check passed. The repo is private on a free
# plan, so GitHub cannot enforce required checks; this script is the guard.
# Usage: scripts/merge-pr.sh <pr-number>
set -euo pipefail

pr="${1:?usage: scripts/merge-pr.sh <pr-number>}"

echo "Waiting for checks on PR #$pr..."
if ! gh pr checks "$pr" --watch --fail-fast; then
  echo "PR #$pr has failing checks. Not merging." >&2
  exit 1
fi

pending=$(gh pr checks "$pr" --json bucket -q '[.[] | select(.bucket != "pass" and .bucket != "skipping")] | length')
if [ "$pending" != "0" ]; then
  echo "PR #$pr has $pending check(s) not green. Not merging." >&2
  exit 1
fi

gh pr merge "$pr" --merge --delete-branch
