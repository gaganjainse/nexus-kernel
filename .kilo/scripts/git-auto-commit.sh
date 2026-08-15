#!/usr/bin/env bash
set -euo pipefail

WORKSPACE="/home/gagan/Workspace/nexus-kernel"
LOG_FILE="$WORKSPACE/.kilo/state/git-auto-commit.log"
BRANCH="${GIT_AUTO_COMMIT_BRANCH:-main}"
REMOTE="${GIT_AUTO_COMMIT_REMOTE:-origin}"
COMMIT_PREFIX="${GIT_AUTO_COMMIT_PREFIX:-[auto] audit cycle:}"

mkdir -p "$WORKSPACE/.kilo/state"

log() {
  echo "[$(date -Iseconds)] $*" | tee -a "$LOG_FILE"
}

cd "$WORKSPACE"

# Check if we're in a git repository
if ! git rev-parse --is-inside-work-tree &>/dev/null; then
  log "Not in a git repository. Skipping."
  exit 0
fi

# Get current branch
CURRENT_BRANCH=$(git symbolic-ref --short HEAD 2>/dev/null || echo "")
if [ -z "$CURRENT_BRANCH" ]; then
  log "Detached HEAD or no branch. Skipping."
  exit 0
fi

# Configure git user if not set
if ! git config user.email &>/dev/null; then
  git config user.email "nexusaos-bot@users.noreply.github.com"
fi
if ! git config user.name &>/dev/null; then
  git config user.name "NexusAOS Bot"
fi

# Check for changes
if git diff --quiet && git diff --cached --quiet; then
  log "No changes to commit."
  exit 0
fi

# Stage all changes
git add -A

# Create commit message with timestamp
TIMESTAMP=$(date -Iseconds)
COMMIT_MSG="${COMMIT_PREFIX} ${TIMESTAMP}"

# Commit
if git commit -m "$COMMIT_MSG"; then
  log "Committed: $COMMIT_MSG"
else
  log "Commit failed (maybe no changes after staging?)"
  exit 0
fi

# Push to remote
if git push "$REMOTE" "$CURRENT_BRANCH"; then
  log "Pushed to $REMOTE/$CURRENT_BRANCH"
else
  log "Push failed. Will retry next cycle."
  exit 0
fi

log "✅ Git sync completed successfully"
