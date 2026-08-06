#!/usr/bin/env bash
#
# Midnight Install — Heavy Downloads for CachyOS + dots-hyprland
# For: MSI Sword 16 HX B14VEKG
# Run this during unlimited data window (12 AM – 6 AM)
#
# This script handles the data-heavy steps:
#  - Full system update
#  - NVIDIA drivers
#  - dots-hyprland
#  - AI/ML stack (CUDA, PyTorch, Ollama)
#  - NvChad
#  - Model downloads
#
# Usage:
#   sudo bash midnight-install.sh            # Full midnight install
#   sudo bash midnight-install.sh --skip-ai  # Skip AI stack
#   sudo bash midnight-install.sh --dry-run  # Show what would be done
#
set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(dirname "$(dirname "$SCRIPT_DIR")")"
readonly TOOLS_DIR="${REPO_ROOT}/tools"
readonly CONFIG_DIR="/home/gagan/.config/hypr/config"
readonly MODES_DIR="${CONFIG_DIR}/modes"
readonly SENTINEL_DIR="/var/cache/cachyos-midnight"
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
    if [[ -n "${SUDO_USER:-}" ]]; then
        INSTALL_USER="${SUDO_USER}"
    elif [[ -d /home/gagan ]]; then
        INSTALL_USER="gagan"
    else
        log_error "Cannot detect user. Run with sudo or set SUDO_USER."
        exit 1
    fi
    export HOME="/home/${INSTALL_USER}"
    log_info "Target user: ${INSTALL_USER} (HOME=${HOME})"
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
# Pre-flight
# =============================================================================
preflight_checks() {
    log_info "=== Midnight Install Pre-flight ==="
    require_root
    detect_user

    # Network
    if ! ping -c 1 archlinux.org >/dev/null 2>&1; then
        log_error "No network. Check WiFi/Ethernet."
        exit 1
    fi
    log_ok "Network reachable"

    # Time window check
    local hour
    hour=$(date +%H)
    if [[ $hour -ge 0 && $hour -lt 6 ]]; then
        log_ok "Running in unlimited data window (midnight–6 AM)"
    else
        log_warn "NOT in unlimited data window (current hour: ${hour}:00)"
        log_warn "This script downloads ~5–7 GB. Continue anyway?"
        read -p "Continue? [y/N] " -n 1 -r
        echo
        [[ ! $REPLY =~ ^[Yy]$ ]] && exit 0
    fi

    # GPUs
    log_info "GPUs detected:"
    lspci | grep -i vga || true

    # Disk space
    local available
    available=$(df -h /home | awk 'NR==2 {print $4}')
    log_info "Home partition available space: ${available}"

    mkdir -p "${SENTINEL_DIR}"
    log_ok "Pre-flight checks passed"
}

# =============================================================================
# Stage 1: System update
# =============================================================================
stage_system_update() {
    log_info "=== Stage 1: System Update ==="
    if is_stage_complete "system_update"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

    if [[ ! -d /etc/pacman.d/gnupg ]]; then
        log_info "Initializing pacman keyring..."
        pacman-key --init
        pacman-key --populate cachyos
    fi

    pacman -Syu --noconfirm
    mark_stage_complete "system_update"
    log_ok "Stage 1 complete. REBOOT REQUIRED."
    return 0
}

# =============================================================================
# Stage 2: NVIDIA drivers + hybrid graphics
# =============================================================================
stage_nvidia_drivers() {
    log_info "=== Stage 2: NVIDIA Drivers + Hybrid Graphics ==="
    if is_stage_complete "nvidia_drivers"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

    pacman -S --noconfirm --needed \
        nvidia-dkms \
        linux-cachyos-headers \
        nvidia-utils \
        lib32-nvidia-utils

    local mkinitcpio="/etc/mkinitcpio.conf"
    if ! grep -q 'MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)' "${mkinitcpio}"; then
        log_info "Configuring mkinitcpio for hybrid graphics..."
        sed -i 's/^MODULES=.*/MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)/' "${mkinitcpio}"
    fi

    mkinitcpio -P

    local boot_entries="/boot/loader/entries"
    if [[ -d "${boot_entries}" ]]; then
        for conf in "${boot_entries}"/*.conf; do
            [[ -f "${conf}" ]] || continue
            if ! grep -q 'nvidia-drm.modeset=1' "${conf}"; then
                sed -i 's|^options |options nvidia-drm.modeset=1 nvidia.NVreg_PreserveVideoMemoryAllocations=1 |' "${conf}" || true
            fi
        done
    fi

    if ! grep -q 'options ec_sys write_support=1' /etc/modprobe.d/ec_sys.conf 2>/dev/null; then
        echo "options ec_sys write_support=1" > /etc/modprobe.d/ec_sys.conf
    fi

    if ! mount | grep -q 'debugfs on /sys/kernel/debug'; then
        mount -t debugfs none /sys/kernel/debug || true
    fi
    if ! mount | grep -q 'efivarfs on /sys/firmware/efi/efivars'; then
        mount -t efivarfs efivarfs /sys/firmware/efi/efivars || true
    fi

    mark_stage_complete "nvidia_drivers"
    log_ok "Stage 2 complete. REBOOT REQUIRED."
    return 0
}

# =============================================================================
# Stage 3: dots-hyprland
# =============================================================================
stage_dots_hyprland() {
    log_info "=== Stage 3: dots-hyprland ==="
    if is_stage_complete "dots_hyprland"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

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

    log_info "Running dots-hyprland installer..."
    if sudo -u "${INSTALL_USER}" bash -lc "bash <(curl -s https://ii.clsty.link/get)"; then
        mark_stage_complete "dots_hyprland"
        log_ok "Stage 3 complete. REBOOT REQUIRED."
    else
        log_warn "dots-hyprland installer had issues"
        return 1
    fi
    return 0
}

# =============================================================================
# Stage 4: MUX switcher
# =============================================================================
stage_mux_switcher() {
    log_info "=== Stage 4: MUX Switcher ==="
    if is_stage_complete "mux_switcher"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

    local src="${TOOLS_DIR}/midnight-install/msi-mux-switcher.py"
    if [[ ! -f "${src}" ]]; then
        log_error "MUX switcher not found at ${src}"
        return 1
    fi

    install -m 755 "${src}" /usr/local/bin/msi-mux-switcher
    mark_stage_complete "mux_switcher"
    log_ok "Stage 4 complete."
    return 0
}

# =============================================================================
# Stage 5: Hyprland MUX configuration
# =============================================================================
stage_hyprland_mux_config() {
    log_info "=== Stage 5: Hyprland MUX Configuration ==="
    if is_stage_complete "hyprland_mux_config"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

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
    mark_stage_complete "hyprland_mux_config"
    log_ok "Stage 5 complete."
    return 0
}

# =============================================================================
# Stage 6: AI/ML stack
# =============================================================================
stage_ai_stack() {
    log_info "=== Stage 6: AI/ML Stack ==="
    if is_stage_complete "ai_stack"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

    if [[ "${SKIP_AI:-false}" == "true" ]]; then
        log_warn "Skipping AI stack (SKIP_AI=true)"
        return 0
    fi

    log_info "Installing CUDA toolkit (~3 GB)..."
    pacman -S --noconfirm --needed cuda cudnn

    log_info "Installing Ollama with CUDA..."
    pacman -S --noconfirm --needed ollama ollama-cuda
    systemctl enable --now ollama.service
    usermod -aG video,render "${INSTALL_USER}"

    log_info "Installing Python ecosystem..."
    pacman -S --noconfirm --needed \
        python \
        python-pip \
        python-virtualenv \
        python-wheel \
        jupyterlab

    log_info "Installing PyTorch with CUDA (~2 GB)..."
    sudo -u "${INSTALL_USER}" bash -lc "pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128"

    log_info "Pulling default Ollama model qwen2.5:7b (~4 GB)..."
    sudo -u "${INSTALL_USER}" bash -lc "ollama pull qwen2.5:7b"

    mark_stage_complete "ai_stack"
    log_ok "Stage 6 complete."
    return 0
}

# =============================================================================
# Stage 7: NvChad
# =============================================================================
stage_nvchad() {
    log_info "=== Stage 7: NvChad ==="
    if is_stage_complete "nvchad"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

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

    mark_stage_complete "nvchad"
    log_ok "Stage 7 complete."
    return 0
}

# =============================================================================
# Stage 8: Final verification
# =============================================================================
stage_verify() {
    log_info "=== Stage 8: Final Verification ==="
    if is_stage_complete "verify"; then
        log_warn "Already completed. Skipping."
        return 0
    fi

    echo ""
    log_info "Checking key components..."

    if command -v nvidia-smi >/dev/null 2>&1; then
        nvidia-smi || log_warn "nvidia-smi failed"
    else
        log_warn "nvidia-smi not found"
    fi

    if [[ -d "${CONFIG_DIR}" ]]; then
        log_ok "Hyprland config directory exists"
    else
        log_warn "Hyprland config directory not found"
    fi

    if command -v msi-mux-switcher >/dev/null 2>&1; then
        msi-mux-switcher status || true
    else
        log_warn "msi-mux-switcher not installed"
    fi

    if systemctl is-active --quiet ollama.service; then
        log_ok "Ollama service running"
    else
        log_warn "Ollama service not running"
    fi

    sudo -u "${INSTALL_USER}" bash -lc \
        "python -c 'import torch; print(\"PyTorch CUDA:\", torch.cuda.is_available(), torch.cuda.get_device_name(0) if torch.cuda.is_available() else \"N/A\")'" 2>/dev/null || true

    local user_home
    user_home="$(eval echo ~${INSTALL_USER})"
    [[ -d "${user_home}/Workspace/nexus-kernel" ]] && log_ok "nexus-kernel repo present"
    [[ -d "${user_home}/Workspace/NexusAOS" ]] && log_ok "NexusAOS repo present"
    [[ -d "${user_home}/Workspace/SeshaOS" ]] && log_ok "SeshaOS repo present"

    mark_stage_complete "verify"
    log_ok "Stage 8 complete."
    return 0
}

# =============================================================================
# Main
# =============================================================================
main() {
    local skip_ai=false
    local dry_run=false
    local assume_yes=false

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --skip-ai) skip_ai=true; shift ;;
            --dry-run) dry_run=true; shift ;;
            --assume-yes) assume_yes=true; shift ;;
            --help|-h)
                echo "Usage: $0 [--skip-ai] [--dry-run] [--assume-yes]"
                echo "  --skip-ai      Skip AI stack"
                echo "  --dry-run      Show what would be done"
                echo "  --assume-yes   Don't ask for confirmation"
                exit 0
                ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
    done

    SKIP_AI="${skip_ai}"
    DRY_RUN="${dry_run}"

    echo ""
    log_info "========================================"
    log_info " Midnight Install — Heavy Downloads"
    log_info " MSI Sword 16 HX B14VEKG"
    log_info "========================================"
    echo ""

    if [[ "${DRY_RUN}" == "true" ]]; then
        log_warn "DRY RUN MODE — no changes will be made"
        echo ""
    fi

    preflight_checks

    if [[ "${assume_yes}" != "true" && "${DRY_RUN}" != "true" ]]; then
        log_warn "This will download ~5–7 GB during unlimited data window."
        read -p "Continue? [Y/n] " -n 1 -r
        echo
        [[ ! $REPLY =~ ^[Yy]$ ]] && exit 0
    fi

    # Run stages in order
    stage_system_update
    stage_nvidia_drivers
    stage_dots_hyprland
    stage_mux_switcher
    stage_hyprland_mux_config
    stage_ai_stack
    stage_nvchad
    stage_verify

    echo ""
    log_ok "========================================"
    log_ok " Midnight Install Complete"
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
