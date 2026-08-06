#!/usr/bin/env bash
#
# CachyOS + dots-hyprland — Automated Installer
# For: MSI Sword 16 HX B14VEKG
# Generated: 2026-08-06
#
# Stages (idempotent — safe to re-run):
#   stage1  System update
#   stage2  NVIDIA drivers + hybrid graphics config
#   stage3  dots-hyprland (illogical-impulse)
#   stage4  Custom MUX switcher
#   stage5  Hyprland MUX configuration
#   stage6  AI/ML stack (CUDA, PyTorch, Ollama)
#   stage7  Directory organization
#   stage8  SSH restore
#   stage9  Clone GitHub repos
#   stage10 Power management + utilities
#   stage11 NvChad
#
# Usage:
#   sudo bash install.sh [--skip-ai] [--dry-run] [--stage N]
#
# Reboots are handled between stages where required.
#
set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(dirname "$(dirname "$SCRIPT_DIR")")"
readonly TOOLS_DIR="${REPO_ROOT}/tools/msi-mux-switcher"
readonly CONFIG_DIR="/home/gagan/.config/hypr/config"
readonly MODES_DIR="${CONFIG_DIR}/modes"
readonly SENTINEL_DIR="/tmp/cachyos-install"
readonly STAGE_FILE="${SENTINEL_DIR}/completed_stages"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# =============================================================================
# Helpers
# =============================================================================
log_info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

require_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (sudo)"
        exit 1
    fi
}

detect_user() {
    LOCAL_USER="${SUDO_USER:-gagan}"
    if [[ -z "${LOCAL_USER}" ]]; then
        # Try to detect from home directories
        if [[ -d /home/gagan ]]; then
            LOCAL_USER="gagan"
        else
            log_error "Cannot detect user. Run with sudo or set SUDO_USER."
            exit 1
        fi
    fi
    log_info "Target user: ${LOCAL_USER}"
    export HOME="/home/${LOCAL_USER}"
}

run() {
    local desc="$1"
    shift
    log_info "${desc}..."
    if "$@"; then
        log_ok "${desc}"
        return 0
    else
        log_error "${desc} failed"
        return 1
    fi
}

run_user() {
    local desc="$1"
    local user="${2:-${LOCAL_USER}}"
    local cmd="$3"
    log_info "${desc}..."
    if sudo -u "${user}" bash -lc "${cmd}" 2>&1; then
        log_ok "${desc}"
        return 0
    else
        log_error "${desc} failed"
        return 1
    fi
}

mark_stage_complete() {
    local stage="$1"
    mkdir -p "${SENTINEL_DIR}"
    echo "${stage}" >> "${STAGE_FILE}"
    sort -u "${STAGE_FILE}" -o "${STAGE_FILE}"
}

is_stage_complete() {
    local stage="$1"
    [[ -f "${STAGE_FILE}" ]] && grep -qx "${stage}" "${STAGE_FILE}"
}

# =============================================================================
# Pre-flight checks
# =============================================================================
preflight_checks() {
    log_info "=== Pre-flight Checks ==="
    require_root
    detect_user

    # Network
    if ! ping -c 1 archlinux.org >/dev/null 2>&1; then
        log_error "No network. Check WiFi/Ethernet."
        exit 1
    fi
    log_ok "Network reachable"

    # GPUs
    log_info "GPUs detected:"
    lspci | grep -i vga || true

    # OS
    if [[ -f /etc/os-release ]]; then
        . /etc/os-release
        log_info "OS: ${NAME:-Unknown} ${VERSION:-}"
    fi

    log_ok "Pre-flight checks passed"
}

# =============================================================================
# Stage 1: System update
# =============================================================================
stage1_system_update() {
    log_info "=== Stage 1: System Update ==="
    if is_stage_complete "stage1"; then
        log_warn "Stage 1 already completed. Skipping."
        return 0
    fi

    # Initialize pacman keyring if needed
    if [[ ! -d /etc/pacman.d/gnupg ]]; then
        log_info "Initializing pacman keyring..."
        pacman-key --init
        pacman-key --populate cachyos
    fi

    # Full system update
    pacman -Syu --noconfirm

    mark_stage_complete "stage1"
    log_ok "Stage 1 complete. REBOOT REQUIRED."
    return 0
}

# =============================================================================
# Stage 2: NVIDIA drivers + hybrid graphics
# =============================================================================
stage2_nvidia() {
    log_info "=== Stage 2: NVIDIA Drivers + Hybrid Graphics ==="
    if is_stage_complete "stage2"; then
        log_warn "Stage 2 already completed. Skipping."
        return 0
    fi

    # Install drivers
    pacman -S --noconfirm --needed \
        nvidia-dkms \
        linux-cachyos-headers \
        nvidia-utils \
        lib32-nvidia-utils

    # Configure mkinitcpio for Intel + NVIDIA hybrid
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

    mark_stage_complete "stage2"
    log_ok "Stage 2 complete. REBOOT REQUIRED."
    return 0
}

# =============================================================================
# Stage 3: dots-hyprland
# =============================================================================
stage3_dots_hyprland() {
    log_info "=== Stage 3: dots-hyprland ==="
    if is_stage_complete "stage3"; then
        log_warn "Stage 3 already completed. Skipping."
        return 0
    fi

    # Install Hyprland dependencies that might be missing
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
    if run_user "dots-hyprland installer" "${LOCAL_USER}" "bash <(curl -s https://ii.clsty.link/get)"; then
        log_ok "dots-hyprland installer completed"
    else
        log_warn "dots-hyprland installer had issues. Check output above."
    fi

    mark_stage_complete "stage3"
    log_ok "Stage 3 complete. REBOOT REQUIRED."
    return 0
}

# =============================================================================
# Stage 4: Custom MUX switcher
# =============================================================================
stage4_mux_switcher() {
    log_info "=== Stage 4: Custom MUX Switcher ==="
    if is_stage_complete "stage4"; then
        log_warn "Stage 4 already completed. Skipping."
        return 0
    fi

    if [[ ! -f "${TOOLS_DIR}/msi-mux-switcher.py" ]]; then
        log_error "MUX switcher not found at ${TOOLS_DIR}/msi-mux-switcher.py"
        log_error "Ensure nexus-kernel repo is cloned to ${REPO_ROOT}"
        exit 1
    fi

    # Install MUX switcher
    install -m 755 "${TOOLS_DIR}/msi-mux-switcher.py" /usr/local/bin/msi-mux-switcher

    # Ensure ec_sys is loaded with write support
    if ! grep -q 'options ec_sys write_support=1' /etc/modprobe.d/ec_sys.conf 2>/dev/null; then
        echo "options ec_sys write_support=1" > /etc/modprobe.d/ec_sys.conf
    fi

    # Install Go toolchain (needed for future enhancements)
    pacman -S --noconfirm --needed go base-devel git

    mark_stage_complete "stage4"
    log_ok "Stage 4 complete."
    return 0
}

# =============================================================================
# Stage 5: Hyprland MUX configuration
# =============================================================================
stage5_hyprland_mux() {
    log_info "=== Stage 5: Hyprland MUX Configuration ==="
    if is_stage_complete "stage5"; then
        log_warn "Stage 5 already completed. Skipping."
        return 0
    fi

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"

    # Create mode-specific configs
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

    # Create udev rules for stable GPU paths
    cat > /etc/udev/rules.d/igpu-device-path.rules <<'EOF'
KERNEL=="card*", KERNELS=="0000:00:02.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/igpu"
EOF

    cat > /etc/udev/rules.d/dgpu-device-path.rules <<'EOF'
KERNEL=="card*", KERNELS=="0000:01:00.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/dgpu"
EOF

    udevadm control --reload-rules
    udevadm trigger

    # Ensure ownership
    chown -R "${LOCAL_USER}:${LOCAL_USER}" "${user_home}/.config"

    mark_stage_complete "stage5"
    log_ok "Stage 5 complete."
    return 0
}

# =============================================================================
# Stage 6: AI/ML stack
# =============================================================================
stage6_ai_stack() {
    log_info "=== Stage 6: AI/ML Stack ==="
    if is_stage_complete "stage6"; then
        log_warn "Stage 6 already completed. Skipping."
        return 0
    fi

    # CUDA toolkit
    pacman -S --noconfirm --needed cuda cudnn

    # Ollama + CUDA
    pacman -S --noconfirm --needed ollama ollama-cuda
    systemctl enable --now ollama.service
    usermod -aG video,render "${LOCAL_USER}"

    # Python + pip
    pacman -S --noconfirm --needed \
        python \
        python-pip \
        python-virtualenv \
        python-wheel \
        jupyterlab

    # Install PyTorch with CUDA support
    log_info "Installing PyTorch with CUDA support..."
    run_user "PyTorch install" "${LOCAL_USER}" \
        "pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128"

    # Pull default Ollama model
    log_info "Pulling default Ollama model (qwen2.5:7b)..."
    run_user "Ollama model pull" "${LOCAL_USER}" "ollama pull qwen2.5:7b"

    mark_stage_complete "stage6"
    log_ok "Stage 6 complete."
    return 0
}

# =============================================================================
# Stage 7: Directory organization
# =============================================================================
stage7_directories() {
    log_info "=== Stage 7: Directory Organization ==="
    if is_stage_complete "stage7"; then
        log_warn "Stage 7 already completed. Skipping."
        return 0
    fi

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"

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
        chown "${LOCAL_USER}:${LOCAL_USER}" "${d}"
    done

    # Symlink Ollama models if default location exists
    if [[ -d "${user_home}/.ollama" && ! -e "${user_home}/Models/ollama" ]]; then
        ln -s "${user_home}/.ollama" "${user_home}/Models/ollama"
        chown -h "${LOCAL_USER}:${LOCAL_USER}" "${user_home}/Models/ollama"
    fi

    mark_stage_complete "stage7"
    log_ok "Stage 7 complete."
    return 0
}

# =============================================================================
# Stage 8: SSH restore
# =============================================================================
stage8_ssh() {
    log_info "=== Stage 8: SSH Keys Restore ==="
    if is_stage_complete "stage8"; then
        log_warn "Stage 8 already completed. Skipping."
        return 0
    fi

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    local backup="${user_home}/Downloads/ssh-backup.tar.gz"

    if [[ -f "${backup}" ]]; then
        log_info "Found SSH backup at ${backup}"
        tar xzf "${backup}" -C "${user_home}"
        chown -R "${LOCAL_USER}:${LOCAL_USER}" "${user_home}/.ssh"
        chmod 700 "${user_home}/.ssh"
        chmod 600 "${user_home}/.ssh/id_ed25519" 2>/dev/null || true
        chmod 644 "${user_home}/.ssh/id_ed25519.pub" 2>/dev/null || true
        log_ok "SSH keys restored"
    else
        log_warn "No SSH backup found at ${backup}. Skipping."
    fi

    mark_stage_complete "stage8"
    log_ok "Stage 8 complete."
    return 0
}

# =============================================================================
# Stage 9: Clone GitHub repos
# =============================================================================
stage9_repos() {
    log_info "=== Stage 9: Clone GitHub Repos ==="
    if is_stage_complete "stage9"; then
        log_warn "Stage 9 already completed. Skipping."
        return 0
    fi

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    local workspace="${user_home}/Workspace"

    mkdir -p "${workspace}"
    chown "${LOCAL_USER}:${LOCAL_USER}" "${workspace}"

    # Configure git if not already set
    if ! run_user "Check git config" "${LOCAL_USER}" "git config --global user.name" 2>/dev/null; then
        log_info "Setting up git global config..."
        run_user "Git user.name" "${LOCAL_USER}" "git config --global user.name 'gaganjainse'"
        run_user "Git user.email" "${LOCAL_USER}" "git config --global user.email 'gaganjainse@users.noreply.github.com'"
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
            log_info "Repo ${name} exists, pulling latest..."
            run_user "Pull ${name}" "${LOCAL_USER}" "cd ${dest} && git pull --ff-only"
        else
            log_info "Cloning ${name}..."
            run_user "Clone ${name}" "${LOCAL_USER}" "git clone ${url} ${dest}"
        fi
    done

    mark_stage_complete "stage9"
    log_ok "Stage 9 complete."
    return 0
}

# =============================================================================
# Stage 10: Power management + utilities
# =============================================================================
stage10_power_utils() {
    log_info "=== Stage 10: Power Management + Utilities ==="
    if is_stage_complete "stage10"; then
        log_warn "Stage 10 already completed. Skipping."
        return 0
    fi

    # Power management
    pacman -S --noconfirm --needed power-profiles-daemon
    systemctl enable --now power-profiles-daemon.service
    powerprofilesctl set balanced || true

    # Development utilities
    pacman -S --noconfirm --needed \
        git \
        curl \
        wget \
        htop \
        btop \
        ranger \
        ripgrep \
        fd \
        fzf \
        bat \
        eza \
        zoxide \
        starship \
        tmux \
        unzip \
        zip \
        p7zip \
        jq \
        yq \
        ttf-font-nerd-fonts \
        noto-fonts \
        noto-fonts-emoji \
        ttf-jetbrains-mono-nerd

    # Configure zoxide
    if ! run_user "Check zoxide init" "${LOCAL_USER}" "grep -q 'zoxide init' ~/.bashrc" 2>/dev/null; then
        echo 'eval "$(zoxide init bash)"' >> "${HOME}/.bashrc"
    fi

    # Configure starship
    if [[ ! -f "${HOME}/.config/starship.toml" ]]; then
        mkdir -p "${HOME}/.config"
        cat > "${HOME}/.config/starship.toml" <<'EOF'
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
        chown "${LOCAL_USER}:${LOCAL_USER}" "${HOME}/.config/starship.toml"
    fi

    # Ensure starship in shell
    if ! grep -q 'starship init' "${HOME}/.bashrc" 2>/dev/null; then
        echo 'eval "$(starship init bash)"' >> "${HOME}/.bashrc"
    fi

    mark_stage_complete "stage10"
    log_ok "Stage 10 complete."
    return 0
}

# =============================================================================
# Stage 11: NvChad
# =============================================================================
stage11_nvchad() {
    log_info "=== Stage 11: NvChad (Neovim) ==="
    if is_stage_complete "stage11"; then
        log_warn "Stage 11 already completed. Skipping."
        return 0
    fi

    # Install neovim if not present
    pacman -S --noconfirm --needed neovim git

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    local nvim_dir="${user_home}/.config/nvim"

    if [[ ! -d "${nvim_dir}" ]]; then
        log_info "Cloning NvChad..."
        run_user "Clone NvChad" "${LOCAL_USER}" \
            "git clone https://github.com/NvChad/NvChad.git ${nvim_dir}"

        # Run NvChad install
        log_info "Running NvChad install..."
        run_user "NvChad install" "${LOCAL_USER}" "nvim '+MasonInstallAll' '+qall' 2>/dev/null" || true
    else
        log_info "NvChad already installed, updating..."
        run_user "Update NvChad" "${LOCAL_USER}" "cd ${nvim_dir} && git pull --ff-only"
    fi

    mark_stage_complete "stage11"
    log_ok "Stage 11 complete."
    return 0
}

# =============================================================================
# Stage 12: Final verification
# =============================================================================
stage12_verify() {
    log_info "=== Stage 12: Final Verification ==="
    if is_stage_complete "stage12"; then
        log_warn "Stage 12 already completed. Skipping."
        return 0
    fi

    echo ""
    log_info "Checking key components..."

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
    run_user "PyTorch CUDA check" "${LOCAL_USER}" \
        "python -c 'import torch; print(\"PyTorch CUDA:\", torch.cuda.is_available(), torch.cuda.get_device_name(0) if torch.cuda.is_available() else \"N/A\")'" || true

    # Git
    run_user "Git config check" "${LOCAL_USER}" "git config --global user.name" || true

    # Repos
    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    if [[ -d "${user_home}/Workspace/nexus-kernel" ]]; then
        log_ok "nexus-kernel repo present"
    fi
    if [[ -d "${user_home}/Workspace/NexusAOS" ]]; then
        log_ok "NexusAOS repo present"
    fi
    if [[ -d "${user_home}/Workspace/SeshaOS" ]]; then
        log_ok "SeshaOS repo present"
    fi

    mark_stage_complete "stage12"
    log_ok "Stage 12 complete."
    return 0
}

# =============================================================================
# Main
# =============================================================================
main() {
    local skip_ai=false
    local dry_run=false
    local start_stage=1
    local max_stage=12

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --skip-ai)
                skip_ai=true
                shift
                ;;
            --dry-run)
                dry_run=true
                shift
                ;;
            --stage)
                start_stage="$2"
                shift 2
                ;;
            *)
                log_error "Unknown option: $1"
                echo "Usage: $0 [--skip-ai] [--dry-run] [--stage N]"
                exit 1
                ;;
        esac
    done

    echo ""
    log_info "========================================"
    log_info " CachyOS + dots-hyprland Installer"
    log_info " MSI Sword 16 HX B14VEKG"
    log_info "========================================"
    echo ""

    if [[ "${dry_run}" == true ]]; then
        log_warn "DRY RUN MODE — no changes will be made"
        echo ""
        preflight_checks
        for stage in $(seq "${start_stage}" "${max_stage}"); do
            if [[ "${stage}" -eq 6 && "${skip_ai}" == true ]]; then
                log_warn "Stage 6: SKIPPED (--skip-ai)"
                continue
            fi
            log_info "Would run: stage${stage}"
        done
        log_ok "Dry run complete"
        exit 0
    fi

    # Actual execution
    preflight_checks

    # Stage 1: System update
    if [[ "${start_stage}" -le 1 ]]; then
        stage1_system_update
        if ! is_stage_complete "stage1"; then
            log_error "Stage 1 failed. Fix and re-run."
            exit 1
        fi
        log_warn "REBOOT REQUIRED. After reboot, run: sudo bash ${SCRIPT_DIR}/install.sh --stage 2"
        return 0
    fi

    # Stage 2: NVIDIA
    if [[ "${start_stage}" -le 2 ]]; then
        stage2_nvidia
        if ! is_stage_complete "stage2"; then
            log_error "Stage 2 failed. Fix and re-run."
            exit 1
        fi
        log_warn "REBOOT REQUIRED. After reboot, run: sudo bash ${SCRIPT_DIR}/install.sh --stage 3"
        return 0
    fi

    # Stage 3: dots-hyprland
    if [[ "${start_stage}" -le 3 ]]; then
        stage3_dots_hyprland
        if ! is_stage_complete "stage3"; then
            log_error "Stage 3 failed. Fix and re-run."
            exit 1
        fi
        log_warn "REBOOT REQUIRED. After reboot, run: sudo bash ${SCRIPT_DIR}/install.sh --stage 4"
        return 0
    fi

    # Stage 4: MUX switcher
    if [[ "${start_stage}" -le 4 ]]; then
        stage4_mux_switcher
        if ! is_stage_complete "stage4"; then
            log_error "Stage 4 failed. Fix and re-run."
            exit 1
        fi
    fi

    # Stage 5: Hyprland MUX config
    if [[ "${start_stage}" -le 5 ]]; then
        stage5_hyprland_mux
        if ! is_stage_complete "stage5"; then
            log_error "Stage 5 failed. Fix and re-run."
            exit 1
        fi
    fi

    # Stage 6: AI stack
    if [[ "${start_stage}" -le 6 && "${skip_ai}" == false ]]; then
        stage6_ai_stack
        if ! is_stage_complete "stage6"; then
            log_error "Stage 6 failed. Fix and re-run."
            exit 1
        fi
    elif [[ "${skip_ai}" == true ]]; then
        log_warn "Skipping AI stack (--skip-ai)"
    fi

    # Stage 7: Directories
    if [[ "${start_stage}" -le 7 ]]; then
        stage7_directories
        if ! is_stage_complete "stage7"; then
            log_error "Stage 7 failed. Fix and re-run."
            exit 1
        fi
    fi

    # Stage 8: SSH restore
    if [[ "${start_stage}" -le 8 ]]; then
        stage8_ssh
        if ! is_stage_complete "stage8"; then
            log_error "Stage 8 failed. Fix and re-run."
            exit 1
        fi
    fi

    # Stage 9: Repos
    if [[ "${start_stage}" -le 9 ]]; then
        stage9_repos
        if ! is_stage_complete "stage9"; then
            log_error "Stage 9 failed. Fix and re-run."
            exit 1
        fi
    fi

    # Stage 10: Power + utilities
    if [[ "${start_stage}" -le 10 ]]; then
        stage10_power_utils
        if ! is_stage_complete "stage10"; then
            log_error "Stage 10 failed. Fix and re-run."
            exit 1
        fi
    fi

    # Stage 11: NvChad
    if [[ "${start_stage}" -le 11 ]]; then
        stage11_nvchad
        if ! is_stage_complete "stage11"; then
            log_error "Stage 11 failed. Fix and re-run."
            exit 1
        fi
    fi

    # Stage 12: Verification
    if [[ "${start_stage}" -le 12 ]]; then
        stage12_verify
        if ! is_stage_complete "stage12"; then
            log_error "Stage 12 failed. Fix and re-run."
            exit 1
        fi
    fi

    echo ""
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
}

main "$@"
