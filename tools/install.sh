#!/usr/bin/env bash
#
# CachyOS + dots-hyprland — Single-Shot Installer
# For: MSI Sword 16 HX B14VEKG
#
# Installs everything in one go:
#  - System update
#  - NVIDIA drivers + hybrid graphics
#  - dots-hyprland (illogical-impulse)
#  - MUX switcher
#  - Hyprland MUX configuration
#  - AI/ML stack (CUDA, PyTorch, Ollama)
#  - Directory organization
#  - SSH restore
#  - GitHub repos
#  - Power management + utilities
#  - NvChad
#  - Verification
#
# Usage:
#   sudo bash install.sh
#
set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
REPO_ROOT="/home/gagan/Workspace/nexus-kernel"
TOOLS_DIR="${REPO_ROOT}/tools/msi-mux-switcher"
CONFIG_DIR="/home/gagan/.config/hypr/config"
MODES_DIR="${CONFIG_DIR}/modes"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

# =============================================================================
# Pre-flight
# =============================================================================
preflight() {
    log_info "=== Pre-flight Checks ==="

    if [[ $EUID -ne 0 ]]; then
        log_error "Run as root: sudo bash install.sh"
        exit 1
    fi

    LOCAL_USER="${SUDO_USER:-gagan}"
    if [[ -z "${LOCAL_USER}" ]]; then
        LOCAL_USER="gagan"
    fi
    export HOME="/home/${LOCAL_USER}"
    log_info "Target user: ${LOCAL_USER}"

    if ! ping -c 1 archlinux.org >/dev/null 2>&1; then
        log_error "No network. Check WiFi/Ethernet."
        exit 1
    fi
    log_ok "Network reachable"

    log_info "GPUs detected:"
    lspci | grep -i vga || true

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
    log_info "=== 1. System Update ==="

    if [[ ! -d /etc/pacman.d/gnupg ]]; then
        log_info "Initializing pacman keyring..."
        pacman-key --init
        pacman-key --populate cachyos
    fi

    pacman -Syu --noconfirm
    log_ok "System updated"
}

# =============================================================================
# 2. NVIDIA drivers + hybrid graphics
# =============================================================================
nvidia_setup() {
    log_info "=== 2. NVIDIA Drivers + Hybrid Graphics ==="

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

    log_ok "NVIDIA drivers configured"
}

# =============================================================================
# 3. dots-hyprland
# =============================================================================
dots_hyprland() {
    log_info "=== 3. dots-hyprland ==="

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
    if sudo -u "${LOCAL_USER}" bash -lc "bash <(curl -s https://ii.clsty.link/get)"; then
        log_ok "dots-hyprland installed"
    else
        log_warn "dots-hyprland installer had issues"
    fi
}

# =============================================================================
# 4. MUX switcher
# =============================================================================
mux_switcher() {
    log_info "=== 4. MUX Switcher ==="

    if [[ ! -f "${TOOLS_DIR}/msi-mux-switcher.py" ]]; then
        log_error "MUX switcher not found at ${TOOLS_DIR}/msi-mux-switcher.py"
        return 1
    fi

    install -m 755 "${TOOLS_DIR}/msi-mux-switcher.py" /usr/local/bin/msi-mux-switcher
    log_ok "MUX switcher installed"
}

# =============================================================================
# 5. Hyprland MUX configuration
# =============================================================================
hyprland_mux_config() {
    log_info "=== 5. Hyprland MUX Configuration ==="

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"

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

    chown -R "${LOCAL_USER}:${LOCAL_USER}" "${user_home}/.config"
    log_ok "Hyprland MUX configuration written"
}

# =============================================================================
# 6. AI/ML stack
# =============================================================================
ai_stack() {
    log_info "=== 6. AI/ML Stack ==="

    pacman -S --noconfirm --needed cuda cudnn

    pacman -S --noconfirm --needed ollama ollama-cuda
    systemctl enable --now ollama.service
    usermod -aG video,render "${LOCAL_USER}"

    pacman -S --noconfirm --needed \
        python \
        python-pip \
        python-virtualenv \
        python-wheel \
        jupyterlab

    log_info "Installing PyTorch with CUDA..."
    sudo -u "${LOCAL_USER}" bash -lc "pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128"

    log_info "Pulling default Ollama model (qwen2.5:7b)..."
    sudo -u "${LOCAL_USER}" bash -lc "ollama pull qwen2.5:7b"

    log_ok "AI/ML stack installed"
}

# =============================================================================
# 7. Directory organization
# =============================================================================
directories() {
    log_info "=== 7. Directory Organization ==="

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

    if [[ -d "${user_home}/.ollama" && ! -e "${user_home}/Models/ollama" ]]; then
        ln -s "${user_home}/.ollama" "${user_home}/Models/ollama"
        chown -h "${LOCAL_USER}:${LOCAL_USER}" "${user_home}/Models/ollama"
    fi

    log_ok "Directory structure created"
}

# =============================================================================
# 8. SSH restore
# =============================================================================
ssh_restore() {
    log_info "=== 8. SSH Keys Restore ==="

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
# 9. GitHub repos
# =============================================================================
git_clone() {
    log_info "=== 9. GitHub Repos ==="

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    local workspace="${user_home}/Workspace"

    mkdir -p "${workspace}"
    chown "${LOCAL_USER}:${LOCAL_USER}" "${workspace}"

    if ! sudo -u "${LOCAL_USER}" git config --global user.name >/dev/null 2>&1; then
        sudo -u "${LOCAL_USER}" git config --global user.name "gaganjainse"
        sudo -u "${LOCAL_USER}" git config --global user.email "gaganjainse@users.noreply.github.com"
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
            sudo -u "${LOCAL_USER}" bash -lc "cd ${dest} && git pull --ff-only" || true
        else
            log_info "Cloning ${name}..."
            sudo -u "${LOCAL_USER}" bash -lc "git clone ${url} ${dest}" || true
        fi
    done

    log_ok "GitHub repos synced"
}

# =============================================================================
# 10. Power management + utilities
# =============================================================================
power_management() {
    log_info "=== 10. Power Management + Utilities ==="

    pacman -S --noconfirm --needed power-profiles-daemon
    systemctl enable --now power-profiles-daemon.service
    powerprofilesctl set balanced || true

    pacman -S --noconfirm --needed \
        git curl wget htop btop ranger ripgrep fd fzf bat eza zoxide starship \
        tmux unzip zip p7zip jq yq \
        ttf-font-nerd-fonts noto-fonts noto-fonts-emoji ttf-jetbrains-mono-nerd

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"

    if ! sudo -u "${LOCAL_USER}" grep -q 'zoxide init' "${user_home}/.bashrc" 2>/dev/null; then
        echo 'eval "$(zoxide init bash)"' >> "${user_home}/.bashrc"
    fi

    if ! sudo -u "${LOCAL_USER}" grep -q 'starship init' "${user_home}/.bashrc" 2>/dev/null; then
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
        chown "${LOCAL_USER}:${LOCAL_USER}" "${user_home}/.config/starship.toml"
    fi

    log_ok "Power management + utilities configured"
}

# =============================================================================
# 11. NvChad
# =============================================================================
nvchad() {
    log_info "=== 11. NvChad ==="

    pacman -S --noconfirm --needed neovim git

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    local nvim_dir="${user_home}/.config/nvim"

    if [[ ! -d "${nvim_dir}" ]]; then
        log_info "Cloning NvChad..."
        sudo -u "${LOCAL_USER}" bash -lc "git clone https://github.com/NvChad/NvChad.git ${nvim_dir}"
        log_info "Running NvChad install..."
        sudo -u "${LOCAL_USER}" bash -lc "nvim '+MasonInstallAll' '+qall' 2>/dev/null" || true
    else
        log_info "NvChad already installed, updating..."
        sudo -u "${LOCAL_USER}" bash -lc "cd ${nvim_dir} && git pull --ff-only" || true
    fi

    log_ok "NvChad configured"
}

# =============================================================================
# 12. Verification
# =============================================================================
verify() {
    log_info "=== 12. Final Verification ==="

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

    sudo -u "${LOCAL_USER}" bash -lc \
        "python -c 'import torch; print(\"PyTorch CUDA:\", torch.cuda.is_available(), torch.cuda.get_device_name(0) if torch.cuda.is_available() else \"N/A\")'" 2>/dev/null || true

    local user_home
    user_home="$(eval echo ~${LOCAL_USER})"
    [[ -d "${user_home}/Workspace/nexus-kernel" ]] && log_ok "nexus-kernel repo present"
    [[ -d "${user_home}/Workspace/NexusAOS" ]] && log_ok "NexusAOS repo present"
    [[ -d "${user_home}/Workspace/SeshaOS" ]] && log_ok "SeshaOS repo present"

    log_ok "Verification complete"
}

# =============================================================================
# Main
# =============================================================================
main() {
    echo ""
    log_info "========================================"
    log_info " CachyOS + dots-hyprland Installer"
    log_info " MSI Sword 16 HX B14VEKG"
    log_info "========================================"
    echo ""

    preflight
    system_update
    nvidia_setup
    dots_hyprland
    mux_switcher
    hyprland_mux_config
    ai_stack
    directories
    ssh_restore
    git_clone
    power_management
    nvchad
    verify

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
