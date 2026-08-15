#!/usr/bin/env bash
set -euo pipefail

WORKSPACE="/home/gagan/Workspace/nexus-kernel"
STATE_FILE="$WORKSPACE/.kilo/state/review-state.json"
LOCK_FILE="$WORKSPACE/.kilo/state/agent-lock"
LOG_FILE="$WORKSPACE/.kilo/state/audit-cycle.log"
MODEL_ROTATOR="$WORKSPACE/.kilo/scripts/model-rotator.sh"
GIT_COMMIT_SCRIPT="$WORKSPACE/.kilo/scripts/git-auto-commit.sh"

mkdir -p "$WORKSPACE/.kilo/state" "$WORKSPACE/.kilo/scripts"

log() {
  echo "[$(date -Iseconds)] $*" | tee -a "$LOG_FILE"
}

# Acquire file lock to avoid clashing with other agent
acquire_lock() {
  if [ -f "$LOCK_FILE" ]; then
    LOCK_PID=$(cat "$LOCK_FILE" 2>/dev/null || echo "")
    if [ -n "$LOCK_PID" ] && kill -0 "$LOCK_PID" 2>/dev/null; then
      log "Another agent (PID $LOCK_PID) is running. Skipping this cycle."
      exit 0
    else
      log "Stale lock file found (PID $LOCK_PID). Removing."
      rm -f "$LOCK_FILE"
    fi
  fi
  echo $$ > "$LOCK_FILE"
  log "Acquired lock (PID $$)"
}

release_lock() {
  rm -f "$LOCK_FILE"
  log "Released lock"
}

# Rotate model before starting cycle
rotate_model() {
  if [ -x "$MODEL_ROTATOR" ]; then
    NEW_MODEL=$("$MODEL_ROTATOR" rotate)
    log "Rotated to model: $NEW_MODEL"
  fi
}

# Run clippy and capture errors
run_clippy() {
  local errors
  errors=$(cargo clippy --workspace 2>&1 | grep -c "^error\[" || true)
  echo "$errors"
}

# Run tests and capture failures
run_tests() {
  local failures
  failures=$(cargo test --workspace 2>&1 | grep -E "^test result:.*failed" | grep -v "0 failed" | wc -l || true)
  echo "$failures"
}

# Check for new TODO/FIXME/HACK comments
check_todos() {
  local count
  count=$(grep -rn "TODO\|FIXME\|HACK\|XXX" crates/ --include="*.rs" 2>/dev/null | grep -v "test" | wc -l || true)
  echo "$count"
}

# Auto-commit and push changes
git_auto_commit() {
  if [ ! -x "$GIT_COMMIT_SCRIPT" ]; then
    log "Git auto-commit script not found or not executable: $GIT_COMMIT_SCRIPT"
    return 0
  fi

  log "Running git auto-commit..."
  if "$GIT_COMMIT_SCRIPT"; then
    log "Git auto-commit completed successfully"
  else
    log "Git auto-commit failed (non-critical)"
  fi
}

# Main audit cycle
audit_cycle() {
  log "=== Starting audit cycle ==="
  acquire_lock
  rotate_model

  # Read current state
  local clippy_errors test_failures todo_count fixes_applied=0

  log "Running clippy..."
  clippy_errors=$(run_clippy)
  log "Clippy errors: $clippy_errors"

  log "Running tests..."
  test_failures=$(run_tests)
  log "Test failures: $test_failures"

  log "Checking TODOs..."
  todo_count=$(check_todos)
  log "TODO comments: $todo_count"

  # Update state
  if command -v jq &>/dev/null; then
    jq --arg ts "$(date -Iseconds)" \
       --arg model "$(basename "$0")" \
       --argjson ce "$clippy_errors" \
       --argjson tf "$test_failures" \
       --argjson fa "$fixes_applied" \
       '.last_cycle = {timestamp: $ts, agent_id: $model, clippy_errors: $ce, test_failures: $tf, fixes_applied: $fa}' \
       "$STATE_FILE" > "${STATE_FILE}.tmp" && mv "${STATE_FILE}.tmp" "$STATE_FILE"
  fi

  # Auto-commit and push if all checks pass
  if [ "$clippy_errors" -eq 0 ] && [ "$test_failures" -eq 0 ]; then
    log "✅ All checks passing. Running git auto-commit..."
    git_auto_commit
  else
    log "⚠️  Issues found: $clippy_errors clippy errors, $test_failures test failures. Skipping git push."
  fi

  release_lock
  log "=== Cycle complete ==="
}

# Handle signals
trap release_lock EXIT INT TERM

# Run cycle
audit_cycle
