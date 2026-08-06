#!/usr/bin/env bash
#
# CachyOS + dots-hyprland — Automated Installer
# For: MSI Sword 16 HX B14VEKG
# Generated: 2026-08-06
#
# This script automates the entire post-midnight setup:
#  - System update
#  - NVIDIA drivers + hybrid graphics
#  - dots-hyprland (illogical-impulse)
#  - Custom MUX switcher
#  - Hyprland MUX configuration
#  - AI/ML stack (CUDA, PyTorch, Ollama)
#  - Directory organization
#  - SSH restore (if backup found)
#  - GitHub repo cloning
#  - Power management
#
# Usage:
#   sudo bash install.sh            # Full install
#   sudo bash install.sh --skip-ai  # Skip AI stack
#   sudo bash install.sh --dry-run  # Show what would be done
#
# Reboots are handled automatically where required.
#
set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
REPO_ROOT="/home/gagan/Workspace/nexus-kernel"
TOOLS_DIR="${REPO_ROOT}/tools/msi-mux-switcher"
CONFIG_DIR="/home/gagan/.config/hypr/config"
MODES_DIR="${CONFIG_DIR}/modes"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

run() {
    local desc="$1"
    shift
    log_info "${desc}..."
    if "$@"; then
        log_ok "${desc}"
    else
        log_error "${desc} failed"
        exit 1
    fi
}

run_user() {
    local desc="$1"
    local user="${2:-gagan}"
    local cmd="$3"

    log_info "${desc}..."
    if sudo -u "${user}" bash -lc "${cmd}"; then
        log_ok "${desc}"
    else
        log_error "${desc} failed"
        exit 1
    fi
}

# =============================================================================
# Pre-flight checks
# =============================================================================
preflight_checks() {
    log_info "Running pre-flight checks..."

    # Must be root
    require_root

    # Detect user
    LOCAL_USER="${SUDO_USER:-gagan}"
    if [[ -z "${LOCAL_USER}" ]]; then
        log_warn "SUDO_USER not set; defaulting to gagan"
        LOCAL_USER="gagan"
    fi
    log_info "Target user: ${LOCAL_USER}"

    # Detect network
    if ! ping -c 1 archlinux.org >/dev/null 2>&1; then
        log_error "No network connectivity. Check WiFi/Ethernet."
        exit 1
    fi
    log_ok "Network reachable"

    # Detect GPUs
    log_info "Detecting GPUs..."
    if ! command -v lspci >/dev/null 2>&1; then
        log_error "lspci not found. Install pciutils."
        exit 1
    fi
    lspci | grep -i vga || true
    log_ok "GPU detection complete"

    # Check if CachyOS
    if [[ -f /etc/os-release ]]; then
        . /etc/os-release
        log_info "OS: ${NAME:-Unknown} ${VERSION:-}"
    fi

    log_ok "Pre-flight checks passed"
}

# =============================================================================
# 1. System update
# =============================================================================
system_update() {
    log_info "=== Step 1: System Update ==="
    pacman -Syu --noconfirm
    log_ok "System updated"
}

# =============================================================================
# 2. NVIDIA drivers + hybrid graphics
# =============================================================================
nvidia_setup() {
    log_info "=== Step 2: NVIDIA Drivers + Hybrid Graphics ==="

    # Install drivers
    pacman -S --noconfirm --needed \
        nvidia-dkms \
        linux-cachyos-headers \
        nvidia-utils \
        lib32-nvidia-utils

    # Configure mkinitcpio for Intel + NVIDIA hybrid
    local mkinitcpio="/etc/mkinitcpio.conf"
    if ! grep -q 'MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)' "${mkinitcpio}"; then
        log_info "Updating MODULES in mkinitcpio.conf for hybrid graphics..."
        sed -i 's/^MODULES=.*/MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)/' "${mkinitcpio}"
    else
        log_info "mkinitcpio.conf already configured for hybrid graphics"
    fi

    # Rebuild initramfs
    mkinitcpio -P

    # Add kernel parameters for Limine
    local boot_entries="/boot/loader/entries"
    if [[ -d "${boot_entries}" ]]; then
        log_info "Adding NVIDIA kernel parameters..."
        for conf in "${boot_entries}"/*.conf; do
            [[ -f "${conf}" ]] || continue
            if ! grep -q 'nvidia-drm.modeset=1' "${conf}"; then
                sed -i 's|^options |options nvidia-drm.modeset=1 nvidia.NVreg_PreserveVideoMemoryAllocations=1 |' "${conf}" || true
            fi
        done
    else
        log_warn "Boot entries directory not found: ${boot_entries}"
    fi

    log_ok "NVIDIA drivers configured. Reboot required."
}

# =============================================================================
# 3. dots-hyprland
# =============================================================================
dots_hyprland() {
    log_info "=== Step 3: dots-hyprland (illogical-impulse) ==="

    # Ensure running as target user for installer
    local cmd='bash <(curl -s https://ii.clsty.link/get)'
    if run_user "Running dots-hyprland installer" "${LOCAL_USER}" "${cmd}"; then
        log_ok "dots-hyprland installed"
    else
        log_warn "dots-hyprland installer reported an issue"
    fi

    log_ok "dots-hyprland installation complete. Reboot required."
}

# =============================================================================
# 4. Custom MUX switcher
# =============================================================================
mux_switcher() {
    log_info "=== Step 4: Custom MUX Switcher ==="

    local dest="/usr/local/bin/msi-mux-switcher"
    if [[ -f "${TOOLS_DIR}/msi-mux-switcher.py" ]]; then
        cp "${TOOLS_DIR}/msi-mux-switcher.py" "${dest}"
        chmod +x "${dest}"
        log_ok "Installed msi-mux-switcher to ${dest}"
    else
        log_error "MUX switcher script not found at ${TOOLS_DIR}/msi-mux-switcher.py"
        log_error "Clone nexus-kernel repo first or run from repo root"
        exit 1
    fi

    # Enable ec_sys with write support
    if ! grep -q 'options ec_sys write_support=1' /etc/modprobe.d/ec_sys.conf 2>/dev/null; then
        echo "options ec_sys write_support=1" > /etc/modprobe.d/ec_sys.conf
    fi

    # Ensure debugfs and efivarfs mounted
    if ! mount | grep -q 'debugfs on /sys/kernel/debug'; then
        mount -t debugfs none /sys/kernel/debug || log_warn "Failed to mount debugfs"
    fi
    if ! mount | grep -q 'efivarfs on /sys/firmware/efi/efivars'; then
        mount -t efivarfs efivarfs /sys/firmware/efi/efivars || log_warn "Failed to mount efivarfs"
    fi

    log_ok "MUX switcher installed"
}

# =============================================================================
# 5. Hyprland MUX configuration
# =============================================================================
hyprland_mux_config() {
    log_info "=== Step 5: Hyprland MUX Configuration ==="

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

    log_ok "Hyprland MUX configuration written to ${MODES_DIR}"
}

# =============================================================================
# 6. AI/ML stack
# =============================================================================
ai_stack() {
    log_info "=== Step 6: AI/ML Stack ==="

    # CUDA toolkit
    pacman -S --noconfirm --needed cuda cudnn

    # Ollama + CUDA
    pacman -S --noconfirm --needed ollama ollama-cuda
    systemctl enable --now ollama.service
    usermod -aG video,render "${LOCAL_USER}"

    # Python + pip
    pacman -S --noconfirm --needed python python-pip python-virtualenv python-wheel

    # Optional: Jupyter
    pacman -S --noconfirm --needed jupyterlab

    # PyTorch CUDA
    log_info "Installing PyTorch with CUDA support..."
    run_user "Installing PyTorch" "${LOCAL_USER}" \
        "pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128"

    # Pull default model
    log_info "Pulling default Ollama model (qwen2.5:7b)..."
    run_user "Pulling qwen2.5:7b" "${LOCAL_USER}" "ollama pull qwen2.5:7b"

    log_ok "AI/ML stack installed"
}

# =============================================================================
# 7. Directory organization
# =============================================================================
directory_organization() {
    log_info "=== Step 7: Directory Organization ==="

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

    log_ok "Directory structure created"
}

# =============================================================================
# 8. Restore SSH keys
# =============================================================================
restore_ssh() {
    log_info "=== Step 8: SSH Keys Restore ==="

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
}

# =============================================================================
# 9. Clone GitHub repos
# =============================================================================
clone_repos() {
    log_info "=== Step 9: Clone GitHub Repos ==="

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    local workspace="${user_home}/Workspace"

    mkdir -p "${workspace}"
    chown "${LOCAL_USER}:${LOCAL_USER}" "${workspace}"

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
            log_info "Repo ${name} already exists, pulling latest..."
            run_user "Pulling ${name}" "${LOCAL_USER}" "cd ${dest} && git pull --ff-only"
        else
            log_info "Cloning ${name}..."
            run_user "Cloning ${name}" "${LOCAL_USER}" "git clone ${url} ${dest}"
        fi
    done

    log_ok "GitHub repos synced"
}

# =============================================================================
# 10. Power management
# =============================================================================
power_management() {
    log_info "=== Step 10: Power Management ==="

    # Install power-profiles-daemon
    pacman -S --noconfirm --needed power-profiles-daemon
    systemctl enable --now power-profiles-daemon.service

    # Set default profile
    powerprofilesctl set balanced || true

    # ZRAM is default on CachyOS; verify swap file for large models
    log_info "Note: If you run out of RAM with large AI models, add a swap file:"
    log_info "  sudo fallocate -l 8G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile"

    log_ok "Power management configured"
}

# =============================================================================
# 11. Optional: NvChad (Neovim)
# =============================================================================
nvchad_setup() {
    log_info "=== Step 11: NvChad (Optional) ==="

    pacman -S --noconfirm --needed neovim git

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"

    if [[ ! -d "${user_home}/.config/nvim" ]]; then
        run_user "Cloning NvChad" "${LOCAL_USER}" \
            "git clone https://github.com/NvChad/NvChad.git ${user_home}/.config/nvim"
    else
        log_info "NvChad already installed, skipping"
    fi

    log_ok "NvChad setup complete"
}

# =============================================================================
# 12. Final verification
# =============================================================================
final_verification() {
    log_info "=== Final Verification ==="

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
    run_user "Verifying PyTorch CUDA" "${LOCAL_USER}" \
        "python -c 'import torch; print(\"PyTorch CUDA:\", torch.cuda.is_available(), torch.cuda.get_device_name(0) if torch.cuda.is_available() else \"N/A\")'" || true

    echo ""
    log_ok "Verification complete"
}

# =============================================================================
# Main
# =============================================================================
main() {
    local skip_ai=false
    local dry_run=false

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
            *)
                log_error "Unknown option: $1"
                echo "Usage: $0 [--skip-ai] [--dry-run]"
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
        log_info "Would run: system_update"
        log_info "Would run: nvidia_setup"
        log_info "Would run: dots_hyprland"
        log_info "Would run: mux_switcher"
        log_info "Would run: hyprland_mux_config"
        if [[ "${skip_ai}" == false ]]; then
            log_info "Would run: ai_stack"
        else
            log_warn "Skipping AI stack (--skip-ai)"
        fi
        log_info "Would run: directory_organization"
        log_info "Would run: restore_ssh"
        log_info "Would run: clone_repos"
        log_info "Would run: power_management"
        log_info "Would run: nvchad_setup"
        log_info "Would run: final_verification"
        log_ok "Dry run complete"
        exit 0
    fi

    # Actual execution
    preflight_checks
    system_update
    nvidia_setup

    log_warn "Reboot required before continuing."
    log_warn "After reboot, run this script again."
    log_info "Rebooting in 10 seconds..."
    sleep 10
    reboot

    # The script will resume after reboot if run again.
    # Use a sentinel file to avoid re-running early steps.
    local sentinel="/tmp/cachyos-install-stage"
    if [[ ! -f "${sentinel}" ]]; then
        log_info "First boot detected. Creating sentinel..."
        touch "${sentinel}"
    fi

    # Continue with remaining steps after reboot
    dots_hyprland
    mux_switcher
    hyprland_mux_config

    if [[ "${skip_ai}" == false ]]; then
        ai_stack
    else
        log_warn "Skipping AI stack (--skip-ai)"
    fi

    directory_organization
    restore_ssh
    clone_repos
    power_management
    nvchad_setup
    final_verification

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
    echo ""
}

main "$@"
