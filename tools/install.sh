#!/usr/bin/env bash
#
# CachyOS + dots-hyprland — Automated Installer
# Architecture inspired by topgrade (topgrade-rs/topgrade)
# For: MSI Sword 16 HX B14VEKG
#
# Step-based, idempotent, config-driven, with retry/reporting/notifications.
#
set -euo pipefail

# =============================================================================
# Configuration (topgrade-style TOML config + env overrides)
# =============================================================================
CONFIG_FILE="/etc/cachyos-install.toml"
USER_CONFIG_FILE="/home/gagan/.config/cachyos-install/config.toml"

# Defaults
INSTALL_USER="${INSTALL_USER:-gagan}"
REPO_ROOT="${REPO_ROOT:-/home/gagan/Workspace/nexus-kernel}"
TOOLS_DIR="${REPO_ROOT}/tools/msi-mux-switcher"
CONFIG_DIR="/home/gagan/.config/hypr/config"
MODES_DIR="${CONFIG_DIR}/modes"
SENTINEL_DIR="/var/cache/cachyos-install"
STAGE_FILE="${SENTINEL_DIR}/completed_stages"
LOG_FILE="/var/log/cachyos-install.log"

# Runtime flags (from config/env)
ASSUME_YES="${ASSUME_YES:-false}"
ASK_RETRY="${ASK_RETRY:-true}"
AUTO_RETRY="${AUTO_RETRY:-0}"
DRY_RUN="${DRY_RUN:-false}"
SKIP_AI="${SKIP_AI:-false}"
ONLY_STEPS="${ONLY_STEPS:-}"
DISABLE_STEPS="${DISABLE_STEPS:-}"
FIRST_STEPS="${FIRST_STEPS:-}"
LAST_STEPS="${LAST_STEPS:-}"
PRE_COMMANDS="${PRE_COMMANDS:-}"
POST_COMMANDS="${POST_COMMANDS:-}"
IGNORE_FAILURES="${IGNORE_FAILURES:-}"
SHOW_SKIPPED="${SHOW_SKIPPED:-true}"
CLEANUP="${CLEANUP:-false}"
NOTIFY_END="${NOTIFY_END:-always}"
RUN_IN_TMUX="${RUN_IN_TMUX:-false}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# =============================================================================
# Logging + reporting (topgrade-style)
# =============================================================================
log_info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_step()  { echo -e "${GREEN}[STEP]${NC}   $*"; }

require_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (sudo)"
        exit 1
    fi
}

detect_user() {
    if [[ -n "${SUDO_USER:-}" ]]; then
        INSTALL_USER="${SUDO_USER}"
    elif [[ -d /home/gagan ]]; then
        INSTALL_USER="gagan"
    else
        log_error "Cannot detect user. Run with sudo or set INSTALL_USER."
        exit 1
    fi
    export HOME="/home/${INSTALL_USER}"
    log_info "Target user: ${INSTALL_USER} (HOME=${HOME})"
}

# =============================================================================
# Step registry (topgrade-style enum + runner pattern)
# =============================================================================
# Steps are defined as an ordered list of (key, run_func, description).
# The runner iterates them, respects disable/only/first/last, and tracks results.

declare -A STEP_DESCRIPTIONS
declare -A STEP_COMPLETED
declare -A STEP_FAILED
declare -A STEP_SKIPPED

STEP_ORDER=(
    "preflight"
    "system_update"
    "nvidia_drivers"
    "dots_hyprland"
    "mux_switcher"
    "hyprland_mux_config"
    "ai_stack"
    "directories"
    "ssh_restore"
    "git_clone"
    "power_management"
    "nvchad"
    "post_commands"
    "verify"
)

STEP_DESCRIPTIONS=(
    [preflight]="Pre-flight checks"
    [system_update]="System update (pacman -Syu)"
    [nvidia_drivers]="NVIDIA drivers + hybrid graphics"
    [dots_hyprland]="dots-hyprland (illogical-impulse)"
    [mux_switcher]="Custom MUX switcher"
    [hyprland_mux_config]="Hyprland MUX configuration"
    [ai_stack]="AI/ML stack (CUDA, PyTorch, Ollama)"
    [directories]="Directory organization"
    [ssh_restore]="SSH keys restore"
    [git_clone]="GitHub repo cloning"
    [power_management]="Power management + utilities"
    [nvchad]="NvChad (Neovim)"
    [post_commands]="Post-commands"
    [verify]="Final verification"
)

# =============================================================================
# Config loader (topgrade-style layered config)
# =============================================================================
load_config() {
    log_info "Loading configuration..."
    
    # System config
    if [[ -f "${CONFIG_FILE}" ]]; then
        log_info "Loading system config: ${CONFIG_FILE}"
        # Parse TOML-ish key=value lines (simple parser)
        while IFS='=' read -r key value; do
            key=$(echo "$key" | tr -d ' []"' | xargs)
            value=$(echo "$value" | tr -d ' []"' | xargs)
            [[ -z "$key" || "$key" == \#* ]] && continue
            case "$key" in
                assume_yes) ASSUME_YES="$value" ;;
                ask_retry) ASK_RETRY="$value" ;;
                auto_retry) AUTO_RETRY="$value" ;;
                dry_run) DRY_RUN="$value" ;;
                skip_ai) SKIP_AI="$value" ;;
                only_steps) ONLY_STEPS="$value" ;;
                disable_steps) DISABLE_STEPS="$value" ;;
                first_steps) FIRST_STEPS="$value" ;;
                last_steps) LAST_STEPS="$value" ;;
                pre_commands) PRE_COMMANDS="$value" ;;
                post_commands) POST_COMMANDS="$value" ;;
                ignore_failures) IGNORE_FAILURES="$value" ;;
                show_skipped) SHOW_SKIPPED="$value" ;;
                cleanup) CLEANUP="$value" ;;
                notify_end) NOTIFY_END="$value" ;;
                run_in_tmux) RUN_IN_TMUX="$value" ;;
                install_user) INSTALL_USER="$value" ;;
                repo_root) REPO_ROOT="$value" ;;
            esac
        done < <(grep -E '^[^#\[]' "${CONFIG_FILE}" 2>/dev/null || true)
    fi
    
    # User config
    if [[ -f "${USER_CONFIG_FILE}" ]]; then
        log_info "Loading user config: ${USER_CONFIG_FILE}"
        while IFS='=' read -r key value; do
            key=$(echo "$key" | tr -d ' []"' | xargs)
            value=$(echo "$value" | tr -d ' []"' | xargs)
            [[ -z "$key" || "$key" == \#* ]] && continue
            case "$key" in
                skip_ai) SKIP_AI="$value" ;;
                only_steps) ONLY_STEPS="$value" ;;
                disable_steps) DISABLE_STEPS="$value" ;;
                assume_yes) ASSUME_YES="$value" ;;
            esac
        done < <(grep -E '^[^#\[]' "${USER_CONFIG_FILE}" 2>/dev/null || true)
    fi
    
    # Env overrides
    : "${ASSUME_YES:=false}"
    : "${ASK_RETRY:=true}"
    : "${AUTO_RETRY:=0}"
    : "${DRY_RUN:=false}"
    : "${SKIP_AI:=false}"
    : "${SHOW_SKIPPED:=true}"
    : "${CLEANUP:=false}"
    : "${NOTIFY_END:=always}"
    
    export ASSUME_YES ASK_RETRY AUTO_RETRY DRY_RUN SKIP_AI ONLY_STEPS DISABLE_STEPS
    export FIRST_STEPS LAST_STEPS PRE_COMMANDS POST_COMMANDS IGNORE_FAILURES
    export SHOW_SKIPPED CLEANUP NOTIFY_END RUN_IN_TMUX
    
    log_info "Config loaded: dry_run=${DRY_RUN}, skip_ai=${SKIP_AI}, assume_yes=${ASSUME_YES}"
}

# =============================================================================
# Step execution engine (topgrade-style runner)
# =============================================================================
is_step_enabled() {
    local step="$1"
    
    # Check disable list
    if [[ ",${DISABLE_STEPS}," == *",${step},"* ]]; then
        return 1
    fi
    
    # Check only list
    if [[ -n "${ONLY_STEPS}" ]]; then
        if [[ ",${ONLY_STEPS}," != *",${step},"* ]]; then
            return 1
        fi
    fi
    
    return 0
}

is_step_completed() {
    local step="$1"
    [[ -f "${STAGE_FILE}" ]] && grep -qx "${step}" "${STAGE_FILE}"
}

mark_step_completed() {
    local step="$1"
    mkdir -p "${SENTINEL_DIR}"
    echo "${step}" >> "${STAGE_FILE}"
    sort -u "${STAGE_FILE}" -o "${STAGE_FILE}"
}

run_step() {
    local step="$1"
    local run_func="$2"
    local desc="${STEP_DESCRIPTIONS[$step]:-$step}"
    local attempt=0
    local max_attempts="${AUTO_RETRY}"
    [[ "${ASK_RETRY}" == "true" ]] && max_attempts=999
    
    # Check if enabled
    if ! is_step_enabled "$step"; then
        if [[ "${SHOW_SKIPPED}" == "true" ]]; then
            log_warn "SKIP: ${desc} (disabled)"
        fi
        STEP_SKIPPED["$step"]=1
        return 0
    fi
    
    # Check if already completed (idempotent)
    if is_step_completed "$step"; then
        if [[ "${SHOW_SKIPPED}" == "true" ]]; then
            log_warn "SKIP: ${desc} (already completed)"
        fi
        STEP_SKIPPED["$step"]=1
        return 0
    fi
    
    # Dry run
    if [[ "${DRY_RUN}" == "true" ]]; then
        log_info "DRY RUN: Would execute: ${desc}"
        STEP_SKIPPED["$step"]=1
        return 0
    fi
    
    # Execute with retry
    while true; do
        attempt=$((attempt + 1))
        log_step "RUN: ${desc}"
        
        if "$run_func"; then
            mark_step_completed "$step"
            STEP_COMPLETED["$step"]=1
            log_ok "PASS: ${desc}"
            return 0
        else
            STEP_FAILED["$step"]=1
            log_error "FAIL: ${desc} (attempt ${attempt})"
            
            # Check if should retry
            if [[ "$step" == "verify" ]]; then
                return 1
            fi
            
            if [[ ",${IGNORE_FAILURES}," == *",${step},"* ]]; then
                log_warn "IGNORE: ${desc} (in ignore_failures list)"
                mark_step_completed "$step"
                return 0
            fi
            
            if [[ $attempt -lt $max_attempts ]]; then
                if [[ "${ASK_RETRY}" == "true" ]]; then
                    read -p "Retry ${desc}? [Y/n] " -n 1 -r
                    echo
                    [[ ! $REPLY =~ ^[Yy]$ ]] && break
                fi
                log_warn "Retrying ${desc} in 3s..."
                sleep 3
            else
                break
            fi
        fi
    done
    
    log_error "Step failed after ${attempt} attempts: ${desc}"
    return 1
}

# =============================================================================
# Step implementations
# =============================================================================
step_preflight() {
    require_root
    detect_user
    
    # Network check
    if ! ping -c 1 archlinux.org >/dev/null 2>&1; then
        log_error "No network connectivity"
        return 1
    fi
    log_ok "Network reachable"
    
    # GPU detection
    log_info "GPUs detected:"
    lspci | grep -i vga || true
    
    # OS check
    if [[ -f /etc/os-release ]]; then
        . /etc/os-release
        log_info "OS: ${NAME:-Unknown} ${VERSION:-}"
    fi
    
    # Create sentinel dir
    mkdir -p "${SENTINEL_DIR}"
    
    return 0
}

step_system_update() {
    # Initialize pacman keyring if needed
    if [[ ! -d /etc/pacman.d/gnupg ]]; then
        log_info "Initializing pacman keyring..."
        pacman-key --init
        pacman-key --populate cachyos
    fi
    
    # Full system update
    pacman -Syu --noconfirm
    return $?
}

step_nvidia_drivers() {
    # Install drivers
    pacman -S --noconfirm --needed \
        nvidia-dkms \
        linux-cachyos-headers \
        nvidia-utils \
        lib32-nvidia-utils
    
    # Configure mkinitcpio for hybrid graphics
    local mkinitcpio="/etc/mkinitcpio.conf"
    if ! grep -q 'MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)' "${mkinitcpio}"; then
        log_info "Configuring mkinitcpio for hybrid graphics..."
        sed -i 's/^MODULES=.*/MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)/' "${mkinitcpio}"
    fi
    
    # Rebuild initramfs
    mkinitcpio -P
    
    # Add kernel parameters for Limine
    local boot_entries="/boot/loader/entries"
    if [[ -d "${boot_entries}" ]]; then
        for conf in "${boot_entries}"/*.conf; do
            [[ -f "${conf}" ]] || continue
            if ! grep -q 'nvidia-drm.modeset=1' "${conf}"; then
                sed -i 's|^options |options nvidia-drm.modeset=1 nvidia.NVreg_PreserveVideoMemoryAllocations=1 |' "${conf}" || true
            fi
        done
    fi
    
    # Enable ec_sys for MUX switcher
    if ! grep -q 'options ec_sys write_support=1' /etc/modprobe.d/ec_sys.conf 2>/dev/null; then
        echo "options ec_sys write_support=1" > /etc/modprobe.d/ec_sys.conf
    fi
    
    # Ensure debugfs and efivarfs mounted
    if ! mount | grep -q 'debugfs on /sys/kernel/debug'; then
        mount -t debugfs none /sys/kernel/debug || true
    fi
    if ! mount | grep -q 'efivarfs on /sys/firmware/efi/efivars'; then
        mount -t efivarfs efivarfs /sys/firmware/efi/efivars || true
    fi
    
    return 0
}

step_dots_hyprland() {
    # Install Hyprland dependencies
    pacman -S --noconfirm --needed \
        hyprland \
        uwsm \
        xdg-desktop-portal-hyprland \
        qt5-wayland \
        qt6-wayland \
        xwayland \
        polkit \
        kitty \
        waybar \
        rofi \
        firefox \
        grim \
        slurp \
        swappy \
        pamixer \
        brightnessctl \
        playerctl \
        grimblast \
        swww \
        wofi \
        dunst \
        libnotify \
        python-gobject \
        gtk3 \
        mesa \
        mesa-amber
    
    # Run dots-hyprland installer as user
    log_info "Running dots-hyprland installer..."
    if sudo -u "${INSTALL_USER}" bash -lc "bash <(curl -s https://ii.clsty.link/get)"; then
        return 0
    else
        log_warn "dots-hyprland installer had issues"
        return 1
    fi
}

step_mux_switcher() {
    if [[ ! -f "${TOOLS_DIR}/msi-mux-switcher.py" ]]; then
        log_error "MUX switcher not found at ${TOOLS_DIR}/msi-mux-switcher.py"
        return 1
    fi
    
    install -m 755 "${TOOLS_DIR}/msi-mux-switcher.py" /usr/local/bin/msi-mux-switcher
    return 0
}

step_hyprland_mux_config() {
    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    
    mkdir -p "${MODES_DIR}"
    
    cat > "${MODES_DIR}/hybrid.lua" <<'EOF'
-- Hybrid: Intel iGPU primary, NVIDIA offload
env = AQ_DRM_DEVICES, /dev/dri/igpu:/dev/dri/dgpu
env = LIBVA_DRIVER_NAME, nvidia
env = __GLX_VENDOR_LIBRARY_NAME, nvidia
env = MOZ_ENABLE_WAYLAND, 1
env = GDK_BACKEND, wayland,x11,*
env = QT_QPA_PLATFORM, wayland;xcb
EOF

    cat > "${MODES_DIR}/dgpu.lua" <<'EOF'
-- dGPU: NVIDIA direct via MUX
env = AQ_DRM_DEVICES, /dev/dri/dgpu
env = LIBVA_DRIVER_NAME, nvidia
env = __GLX_VENDOR_LIBRARY_NAME, nvidia
env = GBM_BACKEND, nvidia-drm
env = MOZ_ENABLE_WAYLAND, 1
env = GDK_BACKEND, wayland,x11,*
env = QT_QPA_PLATFORM, wayland;xcb
env = WLR_NO_HARDWARE_CURSORS, 1
EOF

    cat > /etc/udev/rules.d/igpu-device-path.rules <<'EOF'
KERNEL=="card*", KERNELS=="0000:00:02.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/igpu"
EOF

    cat > /etc/udev/rules.d/dgpu-device-path.rules <<'EOF'
KERNEL=="card*", KERNELS=="0000:01:00.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/dgpu"
EOF

    udevadm control --reload-rules
    udevadm trigger
    
    chown -R "${INSTALL_USER}:${INSTALL_USER}" "${user_home}/.config"
    return 0
}

step_ai_stack() {
    if [[ "${SKIP_AI}" == "true" ]]; then
        log_warn "Skipping AI stack (SKIP_AI=true)"
        return 0
    fi
    
    pacman -S --noconfirm --needed cuda cudnn
    pacman -S --noconfirm --needed ollama ollama-cuda
    systemctl enable --now ollama.service
    usermod -aG video,render "${INSTALL_USER}"
    
    pacman -S --noconfirm --needed \
        python \
        python-pip \
        python-virtualenv \
        python-wheel \
        jupyterlab
    
    log_info "Installing PyTorch with CUDA..."
    sudo -u "${INSTALL_USER}" bash -lc "pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128"
    
    log_info "Pulling default Ollama model (qwen2.5:7b)..."
    sudo -u "${INSTALL_USER}" bash -lc "ollama pull qwen2.5:7b"
    
    return 0
}

step_directories() {
    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    
    local dirs=(
        "${user_home}/Workspace"
        "${user_home}/Projects"
        "${user_home}/Models/ollama"
        "${user_home}/Models/huggingface"
        "${user_home}/Models/checkpoints"
        "${user_home}/Datasets/raw"
        "${user_home}/Datasets/processed"
        "${user_home}/Datasets/experiments"
        "${user_home}/Documents"
        "${user_home}/Downloads"
        "${user_home}/Pictures"
        "${user_home}/Videos"
        "${user_home}/Music"
    )
    
    for d in "${dirs[@]}"; do
        mkdir -p "${d}"
        chown "${INSTALL_USER}:${INSTALL_USER}" "${d}"
    done
    
    if [[ -d "${user_home}/.ollama" && ! -e "${user_home}/Models/ollama" ]]; then
        ln -s "${user_home}/.ollama" "${user_home}/Models/ollama"
        chown -h "${INSTALL_USER}:${INSTALL_USER}" "${user_home}/Models/ollama"
    fi
    
    return 0
}

step_ssh_restore() {
    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    local backup="${user_home}/Downloads/ssh-backup.tar.gz"
    
    if [[ -f "${backup}" ]]; then
        log_info "Found SSH backup at ${backup}"
        tar xzf "${backup}" -C "${user_home}"
        chown -R "${INSTALL_USER}:${INSTALL_USER}" "${user_home}/.ssh"
        chmod 700 "${user_home}/.ssh"
        chmod 600 "${user_home}/.ssh/id_ed25519" 2>/dev/null || true
        chmod 644 "${user_home}/.ssh/id_ed25519.pub" 2>/dev/null || true
        return 0
    else
        log_warn "No SSH backup found at ${backup}"
        return 0
    fi
}

step_git_clone() {
    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    local workspace="${user_home}/Workspace"
    
    mkdir -p "${workspace}"
    chown "${INSTALL_USER}:${INSTALL_USER}" "${workspace}"
    
    # Configure git if missing
    if ! sudo -u "${INSTALL_USER}" git config --global user.name >/dev/null 2>&1; then
        sudo -u "${INSTALL_USER}" git config --global user.name "gaganjainse"
        sudo -u "${INSTALL_USER}" git config --global user.email "gaganjainse@users.noreply.github.com"
    fi
    
    local repos=(
        "nexus-kernel:git@github.com:gaganjainse/nexus-kernel.git"
        "NexusAOS:git@github.com:gaganjainse/NexusAOS.git"
        "SeshaOS:git@github.com:gaganjainse/SeshaOS.git"
    )
    
    for entry in "${repos[@]}"; do
        local name="${entry%%:*}"
        local url="${entry##*:}"
        local dest="${workspace}/${name}"
        
        if [[ -d "${dest}/.git" ]]; then
            log_info "Pulling ${name}..."
            sudo -u "${INSTALL_USER}" bash -lc "cd ${dest} && git pull --ff-only" || true
        else
            log_info "Cloning ${name}..."
            sudo -u "${INSTALL_USER}" bash -lc "git clone ${url} ${dest}" || true
        fi
    done
    
    return 0
}

step_power_management() {
    pacman -S --noconfirm --needed power-profiles-daemon
    systemctl enable --now power-profiles-daemon.service
    powerprofilesctl set balanced || true
    
    # Development utilities
    pacman -S --noconfirm --needed \
        git curl wget htop btop ranger ripgrep fd fzf bat eza zoxide starship \
        tmux unzip zip p7zip jq yq \
        ttf-font-nerd-fonts noto-fonts noto-fonts-emoji ttf-jetbrains-mono-nerd
    
    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    
    # zoxide init
    if ! sudo -u "${INSTALL_USER}" grep -q 'zoxide init' "${user_home}/.bashrc" 2>/dev/null; then
        echo 'eval "$(zoxide init bash)"' >> "${user_home}/.bashrc"
    fi
    
    # starship init
    if ! sudo -u "${INSTALL_USER}" grep -q 'starship init' "${user_home}/.bashrc" 2>/dev/null; then
        echo 'eval "$(starship init bash)"' >> "${user_home}/.bashrc"
    fi
    
    if [[ ! -f "${user_home}/.config/starship.toml" ]]; then
        mkdir -p "${user_home}/.config"
        cat > "${user_home}/.config/starship.toml" <<'EOF'
[character]
success_symbol = "[❯](bold green)"
error_symbol = "[❯](bold red)"

[directory]
truncation_length = 4
style = "bold cyan"

[git_branch]
style = "bold purple"

[python]
style = "bold yellow"
EOF
        chown "${INSTALL_USER}:${INSTALL_USER}" "${user_home}/.config/starship.toml"
    fi
    
    return 0
}

step_nvchad() {
    pacman -S --noconfirm --needed neovim git
    
    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    local nvim_dir="${user_home}/.config/nvim"
    
    if [[ ! -d "${nvim_dir}" ]]; then
        log_info "Cloning NvChad..."
        sudo -u "${INSTALL_USER}" bash -lc "git clone https://github.com/NvChad/NvChad.git ${nvim_dir}"
        log_info "Running NvChad install..."
        sudo -u "${INSTALL_USER}" bash -lc "nvim '+MasonInstallAll' '+qall' 2>/dev/null" || true
    else
        log_info "NvChad already installed, updating..."
        sudo -u "${INSTALL_USER}" bash -lc "cd ${nvim_dir} && git pull --ff-only" || true
    fi
    
    return 0
}

step_post_commands() {
    if [[ -z "${POST_COMMANDS}" ]]; then
        return 0
    fi
    
    log_info "Running post-commands..."
    eval "${POST_COMMANDS}"
    return $?
}

step_verify() {
    log_info "=== Final Verification ==="
    
    # NVIDIA
    if command -v nvidia-smi >/dev/null 2>&1; then
        nvidia-smi || log_warn "nvidia-smi failed"
    else
        log_warn "nvidia-smi not found"
    fi
    
    # Hyprland
    if [[ -d "${CONFIG_DIR}" ]]; then
        log_ok "Hyprland config directory exists"
    else
        log_warn "Hyprland config directory not found"
    fi
    
    # MUX switcher
    if command -v msi-mux-switcher >/dev/null 2>&1; then
        msi-mux-switcher status || true
    else
        log_warn "msi-mux-switcher not installed"
    fi
    
    # Ollama
    if systemctl is-active --quiet ollama.service; then
        log_ok "Ollama service running"
    else
        log_warn "Ollama service not running"
    fi
    
    # PyTorch
    sudo -u "${INSTALL_USER}" bash -lc \
        "python -c 'import torch; print(\"PyTorch CUDA:\", torch.cuda.is_available(), torch.cuda.get_device_name(0) if torch.cuda.is_available() else \"N/A\")'" 2>/dev/null || true
    
    # Git
    sudo -u "${INSTALL_USER}" git config --global user.name 2>/dev/null || true
    
    # Repos
    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    [[ -d "${user_home}/Workspace/nexus-kernel" ]] && log_ok "nexus-kernel repo present"
    [[ -d "${user_home}/Workspace/NexusAOS" ]] && log_ok "NexusAOS repo present"
    [[ -d "${user_home}/Workspace/SeshaOS" ]] && log_ok "SeshaOS repo present"
    
    return 0
}

# =============================================================================
# Step ordering (topgrade-style first/last/only)
# =============================================================================
get_ordered_steps() {
    local steps=("${STEP_ORDER[@]}")
    
    # Apply first/last ordering
    if [[ -n "${FIRST_STEPS}" ]]; then
        local first=()
        local rest=()
        for s in "${steps[@]}"; do
            if [[ ",${FIRST_STEPS}," == *",${s},"* ]]; then
                first+=("$s")
            else
                rest+=("$s")
            fi
        done
        steps=("${first[@]}" "${rest[@]}")
    fi
    
    if [[ -n "${LAST_STEPS}" ]]; then
        local last=()
        local rest=()
        for s in "${steps[@]}"; do
            if [[ ",${LAST_STEPS}," == *",${s},"* ]]; then
                last+=("$s")
            else
                rest+=("$s")
            fi
        done
        steps=("${rest[@]}" "${last[@]}")
    fi
    
    echo "${steps[@]}"
}

# =============================================================================
# Report generation (topgrade-style)
# =============================================================================
print_report() {
    echo ""
    log_info "=== Installation Report ==="
    
    local total=0
    local passed=0
    local failed=0
    local skipped=0
    
    for step in "${STEP_ORDER[@]}"; do
        total=$((total + 1))
        local desc="${STEP_DESCRIPTIONS[$step]:-$step}"
        
        if [[ -n "${STEP_COMPLETED[$step]:-}" ]]; then
            echo -e "  ${GREEN}✓${NC} ${desc}"
            passed=$((passed + 1))
        elif [[ -n "${STEP_FAILED[$step]:-}" ]]; then
            echo -e "  ${RED}✗${NC} ${desc}"
            failed=$((failed + 1))
        elif [[ -n "${STEP_SKIPPED[$step]:-}" ]]; then
            echo -e "  ${YELLOW}○${NC} ${desc}"
            skipped=$((skipped + 1))
        else
            echo -e "  ${YELLOW}?${NC} ${desc}"
        fi
    done
    
    echo ""
    log_info "Total: ${total} | Passed: ${passed} | Failed: ${failed} | Skipped: ${skipped}"
    
    if [[ $failed -gt 0 ]]; then
        log_error "Installation completed with ${failed} failure(s)"
        return 1
    else
        log_ok "Installation completed successfully"
        return 0
    fi
}

# =============================================================================
# Pre/post commands (topgrade-style hooks)
# =============================================================================
run_pre_commands() {
    if [[ -z "${PRE_COMMANDS}" ]]; then
        return 0
    fi
    log_info "Running pre-commands..."
    eval "${PRE_COMMANDS}"
}

run_post_commands() {
    if [[ -z "${POST_COMMANDS}" ]]; then
        return 0
    fi
    log_info "Running post-commands..."
    eval "${POST_COMMANDS}"
}

# =============================================================================
# Notification (topgrade-style)
# =============================================================================
send_notification() {
    local title="$1"
    local message="$2"
    
    if command -v notify-send >/dev/null 2>&1; then
        notify-send "${title}" "${message}" 2>/dev/null || true
    fi
}

# =============================================================================
# Cleanup (topgrade-style)
# =============================================================================
run_cleanup() {
    if [[ "${CLEANUP}" != "true" ]]; then
        return 0
    fi
    
    log_info "Running cleanup..."
    
    # Clean pacman cache
    pacman -Scc --noconfirm || true
    
    # Remove old logs
    find /var/log -name "*.gz" -mtime +7 -delete 2>/dev/null || true
    
    log_ok "Cleanup complete"
}

# =============================================================================
# Main runner
# =============================================================================
main() {
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --skip-ai) SKIP_AI=true; shift ;;
            --dry-run) DRY_RUN=true; shift ;;
            --stage) shift; START_STAGE="$1"; shift ;;
            --assume-yes) ASSUME_YES=true; shift ;;
            --no-retry) ASK_RETRY=false; AUTO_RETRY=0; shift ;;
            --cleanup) CLEANUP=true; shift ;;
            --notify) NOTIFY_END="always"; shift ;;
            --help|-h)
                echo "Usage: $0 [--skip-ai] [--dry-run] [--stage N] [--assume-yes] [--no-retry] [--cleanup]"
                echo "  --skip-ai      Skip AI stack installation"
                echo "  --dry-run      Show what would be done"
                echo "  --stage N      Start from stage N (1-14)"
                echo "  --assume-yes   Don't ask for confirmation"
                echo "  --no-retry     Don't retry failed steps"
                echo "  --cleanup      Clean up after installation"
                echo "  --notify       Send desktop notifications"
                exit 0
                ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
    done
    
    # Load config
    load_config
    
    # Banner
    echo ""
    log_info "========================================"
    log_info " CachyOS + dots-hyprland Installer"
    log_info " MSI Sword 16 HX B14VEKG"
    log_info "========================================"
    echo ""
    
    if [[ "${DRY_RUN}" == "true" ]]; then
        log_warn "DRY RUN MODE — no changes will be made"
        echo ""
    fi
    
    # Pre-flight
    run_pre_commands
    
    # Run steps
    local steps
    read -ra steps <<< "$(get_ordered_steps)"
    
    local failed=0
    for step in "${steps[@]}"; do
        if [[ -n "${START_STAGE:-}" ]]; then
            # Map stage number to step name
            local step_num
            for i in "${!STEP_ORDER[@]}"; do
                if [[ "${STEP_ORDER[$i]}" == "$step" ]]; then
                    step_num=$((i + 1))
                    break
                fi
            done
            if [[ -n "${step_num:-}" ]] && [[ "${step_num}" -lt "${START_STAGE}" ]]; then
                continue
            fi
        fi
        
        case "$step" in
            preflight) run_step "$step" step_preflight ;;
            system_update) run_step "$step" step_system_update ;;
            nvidia_drivers) run_step "$step" step_nvidia_drivers ;;
            dots_hyprland) run_step "$step" step_dots_hyprland ;;
            mux_switcher) run_step "$step" step_mux_switcher ;;
            hyprland_mux_config) run_step "$step" step_hyprland_mux_config ;;
            ai_stack) run_step "$step" step_ai_stack ;;
            directories) run_step "$step" step_directories ;;
            ssh_restore) run_step "$step" step_ssh_restore ;;
            git_clone) run_step "$step" step_git_clone ;;
            power_management) run_step "$step" step_power_management ;;
            nvchad) run_step "$step" step_nvchad ;;
            post_commands) run_step "$step" step_post_commands ;;
            verify) run_step "$step" step_verify ;;
            *)
                if [[ "${SHOW_SKIPPED}" == "true" ]]; then
                    log_warn "SKIP: Unknown step: $step"
                fi
                ;;
        esac
        
        if [[ -n "${STEP_FAILED[$step]:-}" ]]; then
            failed=$((failed + 1))
        fi
    done
    
    # Post-commands
    run_post_commands
    
    # Cleanup
    run_cleanup
    
    # Report
    local report_rc
    print_report || report_rc=$?
    
    # Notification
    if [[ "${NOTIFY_END}" == "always" ]] || [[ "${NOTIFY_END}" == "on_failure" && $failed -gt 0 ]]; then
        if [[ $failed -gt 0 ]]; then
            send_notification "CachyOS Install" "Installation completed with ${failed} failure(s)"
        else
            send_notification "CachyOS Install" "Installation completed successfully"
        fi
    fi
    
    # Final message
    echo ""
    if [[ $failed -eq 0 ]]; then
        log_ok "========================================"
        log_ok " Installation Complete"
        log_ok "========================================"
        log_info "Reboot one final time to enter Hyprland."
        log_info "After reboot:"
        log_info "  1. Select Hyprland session at login"
        log_info "  2. Test MUX: sudo msi-mux-switcher status"
        log_info "  3. Test AI: ollama run qwen2.5:7b"
        log_info "  4. Test GPU: prime-run glxinfo | grep NVIDIA"
        log_info "  5. Test Neovim: nvim"
        echo ""
        return 0
    else
        log_error "========================================"
        log_error " Installation Failed (${failed} steps)"
        log_error "========================================"
        log_info "Fix the errors above and re-run:"
        log_info "  sudo bash ${BASH_SOURCE[0]} --stage 1"
        echo ""
        return 1
    fi
}

main "$@"
