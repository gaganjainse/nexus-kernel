#!/usr/bin/env bash
set -euo pipefail

WORKSPACE="/home/gagan/Workspace/nexus-kernel"
STATE_FILE="$WORKSPACE/.kilo/state/review-state.json"

get_current_model() {
  if command -v jq &>/dev/null && [ -f "$STATE_FILE" ]; then
    jq -r '.current_model // "kilo-gateway-free-1"' "$STATE_FILE"
  else
    echo "kilo-gateway-free-1"
  fi
}

set_current_model() {
  local model="$1"
  if command -v jq &>/dev/null && [ -f "$STATE_FILE" ]; then
    local tmp="${STATE_FILE}.tmp"
    jq --arg m "$model" '.current_model = $m' "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"
  fi
}

list_models() {
  if command -v jq &>/dev/null && [ -f "$STATE_FILE" ]; then
    jq -r '.model_rotation[]' "$STATE_FILE"
  else
    echo "kilo-gateway-free-1"
    echo "kilo-gateway-free-2"
    echo "kilo-gateway-free-3"
    echo "nvidia-nim-free"
  fi
}

check_nvidia_available() {
  # Check if NVIDIA model endpoint is responding
  local nvidia_url="${NVIDIA_API_BASE:-https://integrate.api.nvidia.com/v1}"
  if curl -s --max-time 5 "${nvidia_url}/models" >/dev/null 2>&1; then
    echo "available"
  else
    echo "unavailable"
  fi
}

rotate_model() {
  local current
  current=$(get_current_model)
  local models
  mapfile -t models < <(list_models)
  local total=${#models[@]}
  
  if [ "$total" -eq 0 ]; then
    echo "kilo-gateway-free-1"
    return
  fi

  # If current is NVIDIA and it's unavailable, fall back to Kilo Gateway
  if [[ "$current" == nvidia-nim-* ]]; then
    local nvidia_status
    nvidia_status=$(check_nvidia_available)
    if [ "$nvidia_status" = "unavailable" ]; then
      log "NVIDIA model unavailable, falling back to Kilo Gateway"
      set_current_model "kilo-gateway-free-1"
      echo "kilo-gateway-free-1"
      return
    fi
  fi

  local current_idx=0
  for i in "${!models[@]}"; do
    if [ "${models[$i]}" = "$current" ]; then
      current_idx=$i
      break
    fi
  done

  local next_idx=$(( (current_idx + 1) % total ))
  local next_model="${models[$next_idx]}"
  
  # If next is NVIDIA, check availability first
  if [[ "$next_model" == nvidia-nim-* ]]; then
    local nvidia_status
    nvidia_status=$(check_nvidia_available)
    if [ "$nvidia_status" = "unavailable" ]; then
      # Skip NVIDIA, go to next available
      next_idx=$(( (next_idx + 1) % total ))
      next_model="${models[$next_idx]}"
    fi
  fi
  
  set_current_model "$next_model"
  echo "$next_model"
}

case "${1:-}" in
  current)
    get_current_model
    ;;
  list)
    list_models
    ;;
  rotate|"")
    rotate_model
    ;;
  check-nvidia)
    check_nvidia_available
    ;;
  *)
    echo "Usage: $0 {current|list|rotate|check-nvidia}"
    exit 1
    ;;
esac
