# CachyOS + dots-hyprland Complete Installation Guide
**Exhaustive Documentation — MSI Sword 16 HX B14VEKG**
**August 2026**

---

## Your Exact Hardware

| Component | Specification |
|-----------|---------------|
| **Model** | MSI Sword 16 HX B14VEKG |
| **CPU** | Intel Core i7-14700HX (14th Gen Raptor Lake-HX) |
| **Cores/Threads** | 20 cores (8P+12E), 28 threads |
| **CPU Cache** | 33 MB |
| **CPU Turbo** | Up to 5.5 GHz |
| **GPU** | NVIDIA GeForce RTX 4050 Laptop GPU |
| **GPU VRAM** | 6 GB GDDR6 |
| **GPU TDP** | 115 W (with Dynamic Boost) |
| **Display** | 16" FHD+ (1920x1200) 144Hz IPS **or** QHD+ (2560x1600) 240Hz IPS |
| **RAM** | 16 GB DDR5-5600 (2 slots, upgradeable to 96 GB) |
| **Storage** | 1 TB NVMe PCIe Gen4x4 (1 slot used, 1 slot free for Gen5) |
| **Network** | Intel Wi-Fi 6E AX211 + Bluetooth 5.3 |
| **Battery** | 65 Wh (4-cell) |
| **MUX Switch** | ✅ **YES — Hardware MUX Switch** |
| **Boot** | UEFI |
| **Current OS** | Ubuntu 26.04 |

### Critical: MUX Switch
Your laptop has a **hardware multiplexer (MUX)**. This is NOT a standard hybrid graphics laptop.

**What MUX means:**
- The display can be physically connected to **either** Intel UHD iGPU **or** NVIDIA RTX 4050 dGPU
- No PRIME Offload overhead when in dGPU mode (display directly wired to NVIDIA)
- Three modes available:
  1. **Hybrid** (MSHybrid) — Display → Intel iGPU, NVIDIA activates per-app via PRIME Offload
  2. **dGPU Only** (Discrete) — Display → NVIDIA directly via MUX, Intel iGPU idle
  3. **iGPU Only** (Integrated) — Display → Intel iGPU, NVIDIA completely powered off

**Linux MUX control:**
- **BIOS/UEFI:** Can switch before boot (may be greyed out on some UEFI versions)
- **`msi-mux-switcher`** — Linux tool that writes UEFI/EC to switch MUX (requires build from source)
- **MSI Center** — Windows only

---

## Table of Contents
1. [Pre-Installation BIOS Settings](#pre-installation-bios-settings)
2. [Boot Media](#boot-media)
3. [CachyOS Installation](#cachyos-installation)
4. [Post-Installation: First Boot](#post-installation-first-boot)
5. [NVIDIA Drivers + MUX Setup](#nvidia-drivers--mux-setup)
6. [msi-mux-switcher Installation](#msi-mux-switcher-installation)
7. [Hyprland + dots-hyprland](#hyprland--dots-hyprland)
8. [AI/ML Stack](#aiml-stack)
9. [Graphics Modes: When to Use Which](#graphics-modes)
10. [Directory Organization](#directory-organization)
11. [Container Strategy](#container-strategy)
12. [Power Management](#power-management)
13. [Backup & Recovery](#backup--recovery)
14. [Troubleshooting](#troubleshooting)

---

## Pre-Installation BIOS Settings

**Before booting from USB, enter BIOS/UEFI:**
- Press **Del** or **F2** during boot (MSI Sword 16 uses Del for BIOS)
- Advanced users: Press **Alt+RCtrl+RShift** then **F2** for unlocked advanced settings

**Critical settings:**

| Setting | Value | Reason |
|---------|-------|--------|
| **Secure Boot** | **Disabled** | Required for NVIDIA DKMS + msi-mux-switcher |
| **VT-d** | Enabled | Needed for IOMMU, GPU switching |
| **GPU Mode** | **MSHybrid (Hybrid)** | Default, most flexible for Linux |
| **Fast Boot** | Disabled | Prevents boot issues |
| **CSM** | Disabled | UEFI only |
| **TPM** | Enabled (fTPM) | Required for some security features |

**⚠️ WARNING:** Do NOT set GPU Mode to "Discrete" before installing. The installer needs the iGPU for display output during install. Set to Hybrid.

**Save and exit BIOS.**

---

## Boot Media

**Your USB:** 8 GB Cruzer Blade, flashed with CachyOS 260628 ISO via `dd`

**Boot from USB:**
1. Reboot, press **F11** repeatedly for boot menu (MSI boot menu key)
2. Select USB drive
3. At CachyOS boot menu, select:
   ```
   CachyOS with NVIDIA closed-source Driver (latest cards only 900+)
   ```

**Do NOT select:**
- ❌ Default "CachyOS" — uses open-source `nouveau` driver, no CUDA
- ❌ Legacy Hardware — no NVIDIA driver
- ❌ Memtest86+ — RAM test only

---

## CachyOS Installation

### Calamares Settings

**1. Desktop Environment:**
- Select **"No Desktop"**
- Reason: dots-hyprland handles its own dependencies. KDE adds ~1 GB bloat.

**2. Additional Packages:**
Check **ONLY:**
- ✅ **Base-devel + Common packages**

**Uncheck EVERYTHING else:**
- ❌ CachyOS Packages
- ❌ KDE-Desktop
- ❌ All other desktop environments

**3. Partitioning (Manual — Recommended for Your Hardware):**

**Option A: Automatic (Simpler)**
- Select "Erase disk"
- Filesystem: **BTRFS**
- Enable **Snapper** snapshots
- Enable **Encryption** (LUKS2) — optional but recommended for laptops

**Resulting layout:**
```
/dev/nvme0n1p1: 4 GB FAT32 → /boot (EFI System Partition)
/dev/nvme0n1p2: Remaining → LUKS2 encrypted → BTRFS
    ├─ @ → /
    ├─ @home → /home
    ├─ @cache → /var/cache
    ├─ @tmp → /var/tmp
    ├─ @log → /var/log
    └─ @snapshots → /.snapshots
```

**Option B: Manual (Advanced — Recommended for AI Work)**

Create these partitions:

| Partition | Size | FS | Mount |
|-----------|------|----|-------|
| `/dev/nvme0n1p1` | 4 GB | FAT32 | /boot (ESP) |
| `/dev/nvme0n1p2` | 100 GB | BTRFS | @ (root) |
| `/dev/nvme0n1p3` | 32 GB | BTRFS | @models (AI models) |
| `/dev/nvme0n1p4` | Remaining (~813 GB) | BTRFS | @home |

**Why separate @models:**
- AI models are large (10–50 GB)
- Keeps `/home` clean
- Can snapshot `/` independently of models

**4. Bootloader:**
- Select **Limine**
- Better snapshot integration than GRUB
- Shows `[+] Snapshots` in boot menu

**5. User setup:**
- Create username: `gagan`
- Set password
- **Enable automatic login** if desired (skip for security)

**6. Review → Install Now**
- Install time: ~10–15 minutes
- No internet required

### After First Boot

1. You'll see TTY login prompt
2. Log in with your username/password
3. **Verify basic functionality:**
   ```bash
   # Check GPUs detected
   lspci | grep -i vga
   # Should show: Intel UHD Graphics + NVIDIA RTX 4050
   
   # Check network
   ping -c 3 archlinux.org
   ```
4. **STOP HERE.** Do not install anything else tonight.

---

## Post-Installation: First Boot

### Verify Installation
```bash
# Check kernel
uname -r
# Should show something like: 7.0.x-cachyos

# Check BTRFS subvolumes
sudo btrfs subvolume list /

# Check Snapper
sudo snapper list

# Check GPUs
lspci -k | grep -A 3 -i vga
```

---

## NVIDIA Drivers + MUX Setup

### Step 1: Install NVIDIA Drivers
```bash
sudo pacman -Syu
# Reboot if kernel updated
```

```bash
# Install NVIDIA DKMS drivers
sudo pacman -S nvidia-dkms linux-cachyos-headers nvidia-utils lib32-nvidia-utils

# Verify installation
nvidia-smi
# Should show RTX 4050 with driver version
```

### Step 2: Configure mkinitcpio for Hybrid Graphics

**CRITICAL for Intel + NVIDIA hybrid:**
```bash
sudo nano /etc/mkinitcpio.conf
```

Find the `MODULES` line. Change to:
```bash
MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)
```

**Why:** Loads Intel i915 before NVIDIA modules. Fixes Electron/Chromium app stalls on hybrid graphics.

Rebuild initramfs:
```bash
sudo mkinitcpio -P
```

### Step 3: Kernel Parameters for NVIDIA

**For Limine bootloader:**
```bash
sudo nano /boot/loader/entries/*.conf
```

Add to the `options` line:
```bash
nvidia-drm.modeset=1 nvidia.NVreg_PreserveVideoMemoryAllocations=1
```

**Parameters explained:**
- `nvidia-drm.modeset=1` — Enables DRM kernel mode setting for Wayland
- `nvidia.NVreg_PreserveVideoMemoryAllocations=1` — Fixes suspend/resume on laptops

Reboot:
```bash
sudo reboot
```

### Step 4: Verify NVIDIA Works
```bash
# Check driver loaded
nvidia-smi

# Check modules
lsmod | grep nvidia

# Verify DRM modeset
cat /sys/module/nvidia_drm/parameters/modeset
# Should return: Y
```

---

## Custom MUX Switcher Installation

This is the **critical tool** for controlling your MUX switch on Linux.

### What It Does
- Switches between Hybrid, dGPU-only, and iGPU-only modes
- Writes to UEFI variables and Embedded Controller (EC)
- Requires reboot after switching
- Built specifically for MSI Sword 16 HX B14VEKG

### Installation

**Prerequisites:**
```bash
# Enable ec_sys kernel module for EC writes
echo "options ec_sys write_support=1" | sudo tee /etc/modprobe.d/ec_sys.conf
sudo mkinitcpio -P

# Verify debugfs mounted
sudo mount -t debugfs none /sys/kernel/debug

# Verify efivarfs mounted
sudo mount -t efivarfs efivarfs /sys/firmware/efi/efivars
```

**Install the custom tool:**
```bash
# Copy from nexus-kernel repo to PATH
sudo cp /home/gagan/Workspace/nexus-kernel/tools/msi-mux-switcher/msi-mux-switcher.py /usr/local/bin/msi-mux-switcher
sudo chmod +x /usr/local/bin/msi-mux-switcher
```

**Note:** This is a custom-built tool for your exact laptop model. If it fails:
1. Check firmware version: `sudo dmidecode -t bios | grep "Version"`
2. Use BIOS to switch modes temporarily
3. Open issue on GitHub with EC dump

### Usage

```bash
# Check current status (no root needed)
msi-mux-switcher status

# Switch to dGPU mode (performance)
sudo msi-mux-switcher dgpu

# Switch to Hybrid mode (balanced)
sudo msi-mux-switcher hybrid

# Dry run (test without changes)
sudo msi-mux-switcher --dry-run dgpu

# Reboot required after switching
sudo reboot
```

**⚠️ WARNING:** This tool writes directly to UEFI/EC. Use at your own risk. Incorrect use can brick your laptop. Always have BIOS fallback available.

---

## Hyprland + dots-hyprland

### Installation
```bash
bash <(curl -s https://ii.clsty.link/get)
```

**During install:**
- Accept defaults
- It installs: Hyprland, Quickshell, Waybar, Rofi, Kitty, Firefox
- Reboot when done

### First Boot into Hyprland
1. At login screen (SDDM/greetd), select **"Hyprland"** session
2. You should see the illogical-impulse desktop

### Hyprland Configuration for MUX Laptop

**Location:** `~/.config/hypr/config/`

**Create/update environment.lua:**
```lua
-- GPU selection for MUX laptop
-- For Hybrid mode (default): Intel iGPU primary, NVIDIA offload
env = AQ_DRM_DEVICES, /dev/dri/card0:/dev/dri/card1

-- NVIDIA environment variables
env = LIBVA_DRIVER_NAME, nvidia
env = __GLX_VENDOR_LIBRARY_NAME, nvidia

-- Wayland compatibility
env = MOZ_ENABLE_WAYLAND, 1
env = GDK_BACKEND, wayland,x11,*
env = QT_QPA_PLATFORM, wayland;xcb

-- NVIDIA Wayland fixes
env = NVD_BACKEND, direct
env = AQ_FORCE_LINEAR_BLIT, 0
env = WLR_NO_HARDWARE_CURSORS, 1
```

**Create udev rules for stable GPU device paths:**
```bash
# Find your GPU PCI IDs
lspci -d ::03xx
# Example output:
# 0000:00:02.0 Intel UHD Graphics
# 0000:01:00.0 NVIDIA RTX 4050

# Create udev rules
sudo tee /etc/udev/rules.d/igpu-device-path.rules << 'EOF'
KERNEL=="card*", KERNELS=="0000:00:02.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/igpu"
EOF

sudo tee /etc/udev/rules.d/dgpu-device-path.rules << 'EOF'
KERNEL=="card*", KERNELS=="0000:01:00.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/dgpu"
EOF

# Reload udev
sudo udevadm control --reload-rules
sudo udevadm trigger
```

**Update Hyprland config to use stable paths:**
```lua
env = AQ_DRM_DEVICES, /dev/dri/igpu:/dev/dri/dgpu
```

### Running Apps on NVIDIA GPU

**Method 1: prime-run (simpler)**
```bash
sudo pacman -S nvidia-prime
prime-run ollama run qwen2.5:7b
prime-run python train.py
```

**Method 2: Environment variables (for scripts)**
```bash
__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia <app>
```

**Method 3: Custom wrapper script**
```bash
sudo tee /usr/local/bin/nvidia-run << 'EOF'
#!/bin/bash
export __NV_PRIME_RENDER_OFFLOAD=1
export __GLX_VENDOR_LIBRARY_NAME=nvidia
export __VK_LAYER_NV_optimus=NVIDIA_only
export GBM_BACKEND=nvidia-drm
export LIBVA_DRIVER_NAME=nvidia
export WLR_NO_HARDWARE_CURSORS=1
exec "$@"
EOF
sudo chmod +x /usr/local/bin/nvidia-run

# Usage:
nvidia-run ollama run qwen2.5:7b
```

---

## AI/ML Stack

### PyTorch with CUDA
```bash
# Install CUDA toolkit
sudo pacman -S cuda cudnn

# Install PyTorch (CUDA 12.8)
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128

# Verify CUDA
python -c "import torch; print(torch.cuda.is_available()); print(torch.cuda.get_device_name(0))"
```

### Ollama (Local LLMs)
```bash
# Install with CUDA support
sudo pacman -S ollama ollama-cuda

# Enable service
sudo systemctl enable --now ollama.service

# Add user to video/render groups
sudo usermod -aG video,render $USER

# Pull and run model
ollama pull qwen2.5:7b  # 7B model, ~4 GB
ollama run qwen2.5:7b
```

**Models for your hardware (RTX 4050 6GB + 16GB RAM):**
| Model | Size | VRAM | Use Case |
|-------|------|------|----------|
| `qwen2.5:2b` | 1.5 GB | 2 GB | Ultra-fast |
| `qwen2.5:7b` | 4 GB | 6 GB | **Recommended** |
| `qwen2.5:14b` | 9 GB | 10 GB | CPU+GPU hybrid |
| `deepseek-coder-v2:16b` | 9 GB | 12 GB | Code generation |

### Jupyter + ML Tools
```bash
sudo pacman -S jupyterlab
pip install transformers datasets accelerate huggingface-hub chromadb langchain
```

---

## Graphics Modes

Your MUX laptop has three modes. Use them strategically:

### 1. Hybrid Mode (Default)
**When to use:** Daily driving, web browsing, coding, light AI

**How to switch:**
```bash
sudo msi-mux-switcher hybrid  # Switches to Hybrid
sudo reboot
```

**Behavior:**
- Display → Intel UHD iGPU
- NVIDIA activates per-app via PRIME Offload
- Best battery life
- Hyprland config: `AQ_DRM_DEVICES, /dev/dri/igpu:/dev/dri/dgpu`

**Verify:**
```bash
glxinfo | grep "OpenGL renderer string"
# Should show: Intel UHD Graphics
prime-run glxinfo | grep "OpenGL renderer string"
# Should show: NVIDIA RTX 4050
```

### 2. dGPU Mode (Performance)
**When to use:** Heavy AI training, gaming, CUDA workloads, external monitor

**How to switch:**
```bash
sudo msi-mux-switcher dgpu
sudo reboot
```

**Behavior:**
- Display → NVIDIA directly via MUX (no PRIME overhead)
- Intel iGPU idle
- Max performance, higher power draw
- Hyprland config: `AQ_DRM_DEVICES, /dev/dri/dgpu`

**Verify:**
```bash
glxinfo | grep "OpenGL renderer string"
# Should show: NVIDIA RTX 4050
```

**⚠️ WARNING:** dGPU mode may cause black screen on some laptops with external monitors connected during boot. Disconnect external monitors before booting if issues occur.

### 3. iGPU Mode (Power Saving)
**When to use:** Battery only, no CUDA needed

**How to switch:**
```bash
# This requires additional EC tooling or BIOS setting
# Not fully supported by msi-mux-switcher yet
# Use BIOS to switch to "Integrated" mode temporarily
```

**Behavior:**
- Display → Intel UHD iGPU only
- NVIDIA completely powered off
- Max battery life
- No CUDA access

### Mode-Specific Hyprland Config

**Create separate configs for each mode:**

```bash
mkdir -p ~/.config/hypr/config/modes
```

**`~/.config/hypr/config/modes/hybrid.lua`:**
```lua
env = AQ_DRM_DEVICES, /dev/dri/igpu:/dev/dri/dgpu
env = LIBVA_DRIVER_NAME, nvidia
env = __GLX_VENDOR_LIBRARY_NAME, nvidia
```

**`~/.config/hypr/config/modes/dgpu.lua`:**
```lua
env = AQ_DRM_DEVICES, /dev/dri/dgpu
env = LIBVA_DRIVER_NAME, nvidia
env = __GLX_VENDOR_LIBRARY_NAME, nvidia
env = GBM_BACKEND, nvidia-drm
```

**`~/.config/hypr/config/modes/igpu.lua`:**
```lua
env = AQ_DRM_DEVICES, /dev/dri/igpu
env = LIBVA_DRIVER_NAME, iHD
env = __GLX_VENDOR_LIBRARY_NAME, mesa
```

**Auto-detect mode on boot (advanced):**
```bash
# Add to hyprland.conf
exec = ~/.config/hypr/scripts/detect-gpu-mode.sh
```

Create `~/.config/hypr/scripts/detect-gpu-mode.sh`:
```bash
#!/bin/bash
# Detect current MUX mode and load appropriate config
if lspci -k | grep -A 2 "VGA" | grep "nvidia" > /dev/null; then
    # NVIDIA is active, likely dGPU or hybrid
    if glxinfo | grep -q "NVIDIA"; then
        echo "dGPU mode detected"
        # Load dGPU config
    else
        echo "Hybrid mode detected"
        # Load hybrid config
    fi
fi
```

---

## Directory Organization

### Recommended Structure
```
/home/gagan/
├── Workspace/              # Git repos (active development)
│   ├── nexus-kernel/
│   ├── NexusAOS/
│   └── SeshaOS/
├── Projects/               # Future non-Git projects
├── Models/                 # AI models (keep large files here)
│   ├── ollama/             # Ollama model cache (symlink)
│   ├── huggingface/        # HF cache
│   └── checkpoints/        # Training checkpoints
├── Datasets/               # Training data
│   ├── raw/
│   ├── processed/
│   └── experiments/
├── Documents/              # Personal docs
├── Downloads/              # Temporary downloads
├── Pictures/               # Images
├── Videos/                 # Videos
├── Music/                  # Audio
├── .ssh/                   # SSH keys (restore from phone)
└── .config/                # App configs (managed by dots-hyprland)
```

### BTRFS Subvolumes (Created by Calamares)
Calamares creates these automatically:
- `@` → `/`
- `@home` → `/home`
- `@cache` → `/var/cache`
- `@tmp` → `/var/tmp`
- `@log` → `/var/log`
- `@snapshots` → `/.snapshots`

**Optional: Add @models subvolume for AI models:**
```bash
sudo btrfs subvolume create /@models

# Add to /etc/fstab
sudo nano /etc/fstab
# Add (replace UUID with yours):
UUID=<your-btrfs-uuid> /models btrfs noatime,compress=zstd:1,subvol=@models 0 0

# Mount
sudo mount -a
```

### NVMe Optimization
Your Samsung PM991 (or similar) is PCIe 4.0. Add to `/etc/fstab`:
```bash
UUID=<your-btrfs-uuid> / btrfs noatime,compress=zstd:1,space_cache=v2,autodefrag,discard=async,subvol=@ 0 0
UUID=<your-btrfs-uuid> /home btrfs noatime,compress=zstd:1,space_cache=v2,autodefrag,discard=async,subvol=@home 0 0
UUID=<your-btrfs-uuid> /models btrfs noatime,compress=zstd:1,space_cache=v2,autodefrag,discard=async,subvol=@models 0 0
```

---

## Container Strategy

### Recommendation: Native Installation First

**For your single-machine AI workflow:**
- Native installation is simpler and faster
- You have one GPU, one primary AI stack
- Distrobox NVIDIA has known CUDA symlink issues

**Install natively:**
```bash
sudo pacman -S nvidia-dkms python-pip ollama ollama-cuda cuda
```

### When to Add Containers Later
- You need multiple CUDA versions (12.4 + 12.8)
- You need isolated Python environments
- You want to test bleeding-edge PyTorch without risking host

### Distrobox Setup (If Needed Later)
```bash
# Install Podman + Distrobox
sudo pacman -S podman distrobox

# Create AI container with NVIDIA
distrobox create --nvidia --name ai-workstation --image ubuntu:22.04

# Known issue: CUDA symlinks may break
# Fix inside container if needed:
sudo rm -f /usr/lib/x86_64-linux-gnu/libcuda.so
sudo ln -s /usr/lib/x86_64-linux-gnu/libcuda.so.1 /usr/lib/x86_64-linux-gnu/libcuda.so
sudo ldconfig
```

---

## Power Management

### Battery Life Optimization

**Your 65Wh battery will drain quickly with dGPU active. Use these tips:**

```bash
# Install power-profiles-daemon
sudo pacman -S power-profiles-daemon
sudo systemctl enable --now power-profiles-daemon.service

# Switch to balanced/power-saving mode
powerprofilesctl set balanced
# or
powerprofilesctl set power-saver
```

### NVIDIA Power Management

**For Hybrid mode (recommended for battery):**
```bash
# NVIDIA dGPU should suspend when not in use
cat /sys/class/drm/card*/device/power_state
# Should show D3cold (suspended) when idle

# If dGPU stays active:
lsof +c0 /dev/nvidia*
# Shows what's keeping it awake
```

### CPU Governor
CachyOS uses `schedutil` by default — optimal for your use case. No changes needed.

### ZRAM
CachyOS uses ZRAM by default (compressed swap in RAM). For 16GB RAM with AI workloads:

**⚠️ ZRAM WARNING for Large Models:**
ZRAM is bad for workloads using 80-90% of RAM. Compressed pages still consume RAM + CPU.

**If you run out of RAM with large models:**
```bash
# Disable ZRAM
sudo systemctl disable --now systemd-zram-setup@zram0.service

# Create real swap file
sudo fallocate -l 8G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Add to /etc/fstab
echo '/swapfile none swap defaults 0 0' | sudo tee -a /etc/fstab
```

---

## Backup & Recovery

### Snapper Snapshots (Automatic)
CachyOS + BTRFS + Limine creates automatic snapshots before/after pacman transactions.

**Manual snapshot before risky changes:**
```bash
sudo snapper -c root create -d "before NVIDIA install"
sudo snapper -c root create -d "before dots-hyprland"
```

**List snapshots:**
```bash
sudo snapper -c root list
```

**Rollback via boot menu:**
1. Reboot
2. Select `[+] Snapshots` in Limine menu
3. Choose snapshot to rollback to

### BTRFS Send/Receive (External Backup)
```bash
# Mount external drive
sudo mount /dev/sdb1 /mnt/backup

# Send snapshot to backup
sudo btrfs send /@snapshots/1/snapshot | sudo btrfs receive /mnt/backup
```

### Critical Files to Backup
| Path | Method | Frequency |
|------|--------|-----------|
| `/home/gagan/Workspace` | Git remote | After every session |
| `/home/gagan/.ssh` | Phone/encrypted USB | After changes |
| `/home/gagan/Models` | External drive | Monthly |
| `/home/gagan/Datasets` | External drive | Monthly |

---

## Troubleshooting

### MUX Switch Issues

**msi-mux-switcher fails:**
```bash
# Check EC module
lsmod | grep ec_sys
# If not loaded:
sudo modprobe ec_sys write_support=1

# Check firmware version
sudo dmidecode -t bios | grep "Version"
# Open issue on msi-mux-switcher if unsupported
```

**Black screen after switching to dGPU:**
- Disconnect all external monitors
- Boot with external monitors disconnected
- Reconnect after boot

### NVIDIA Issues

**Driver not loading:**
```bash
# Rebuild DKMS
sudo dkms autoinstall

# Check status
sudo dkms status | grep nvidia

# Reboot
```

**Black screen after boot:**
```bash
# Check if i915 loads before nvidia
grep "MODULES" /etc/mkinitcpio.conf
# Should show: MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)

# Rebuild initramfs
sudo mkinitcpio -P
sudo reboot
```

**Suspend/resume broken:**
```bash
# Verify kernel parameter
cat /proc/cmdline | grep "NVreg_PreserveVideoMemoryAllocations"
# Should show: nvidia.NVreg_PreserveVideoMemoryAllocations=1
```

### Hyprland Issues

**Black screen:**
```bash
# Check logs
cat ~/.cache/hypr/hyprland.log

# Try disabling AQ_DRM_DEVICES
# Remove from config and reboot
```

**NVIDIA apps not launching:**
```bash
# Verify prime-run works
prime-run glxinfo | grep "OpenGL renderer string"

# Check environment variables
env | grep -i nvidia
```

**Electron/Chromium apps stall:**
- Already fixed by `MODULES=(i915 nvidia ...)` in mkinitcpio.conf
- Rebuild initramfs if still happening

### Ollama GPU Not Detected
```bash
# Verify NVIDIA works
nvidia-smi

# Check Ollama service
sudo systemctl status ollama

# Ensure user in groups
sudo usermod -aG video,render $USER
# Logout and back in

# Check logs
journalctl -u ollama -f
```

---

## Complete Installation Timeline

### Tonight (Limited Data — ~0 MB)
1. Boot from USB → Select **"CachyOS with NVIDIA closed-source Driver"**
2. Install CachyOS:
   - **"No Desktop"**
   - **Only "Base-devel + Common packages"**
   - **BTRFS + Snapper + LUKS2**
   - **Limine bootloader**
3. Reboot, verify TTY login
4. **STOP**

### After Midnight (Unlimited Data — ~5–7 GB)
```bash
# 1. Full system update (~1 GB)
sudo pacman -Syu
sudo reboot

# 2. NVIDIA drivers (~300 MB)
sudo pacman -S nvidia-dkms linux-cachyos-headers nvidia-utils lib32-nvidia-utils
# Edit mkinitcpio.conf: MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)
sudo mkinitcpio -P
# Add kernel parameter: nvidia-drm.modeset=1 nvidia.NVreg_PreserveVideoMemoryAllocations=1
sudo reboot

# 3. dots-hyprland (~1 GB)
bash <(curl -s https://ii.clsty.link/get)
# Reboot when done

# 4. msi-mux-switcher (custom Python tool)
sudo cp /home/gagan/Workspace/nexus-kernel/tools/msi-mux-switcher/msi-mux-switcher.py /usr/local/bin/msi-mux-switcher
sudo chmod +x /usr/local/bin/msi-mux-switcher

# 5. AI stack (~3–5 GB)
sudo pacman -S ollama ollama-cuda python python-pip cuda cudnn
sudo systemctl enable --now ollama.service
ollama pull qwen2.5:7b

# 6. Verify everything
nvidia-smi
prime-run glxinfo | grep "OpenGL renderer string"
ollama list
```

### Post-Install Checklist
- [ ] Restore SSH keys from phone backup
- [ ] Re-clone GitHub repos
- [ ] Configure Hyprland monitors/keybinds
- [ ] Test MUX switching with msi-mux-switcher
- [ ] Test PRIME Offload with prime-run
- [ ] Install dev tools (neovim, git, etc.)
- [ ] Set up backup strategy
- [ ] Test Ollama with CUDA
- [ ] Configure power profiles

---

## Quick Reference

### Essential Commands
```bash
# MUX switching
sudo msi-mux-switcher status
sudo msi-mux-switcher dgpu    # Performance mode
sudo msi-mux-switcher hybrid    # Hybrid mode

# GPU verification
nvidia-smi
prime-run glxinfo | grep "OpenGL renderer string"
glxinfo | grep "OpenGL renderer string"

# NVIDIA apps
nvidia-run <app>              # Custom wrapper
prime-run <app>               # Built-in wrapper

# System update
sudo pacman -Syu

# Snapshots
sudo snapper -c root create -d "description"
sudo snapper -c root list

# Power management
powerprofilesctl list
powerprofilesctl set balanced
```

### File Locations
```
Hyprland config: ~/.config/hypr/config/
Hyprland logs: ~/.cache/hypr/hyprland.log
Snapshots: /.snapshots/
Models: ~/Models/ or ~/.ollama/
Workspace: ~/Workspace/
msi-mux-switcher: /usr/local/bin/msi-mux-switcher
```

---

## Sources

1. CachyOS Wiki — Installation Guide
2. CachyOS Wiki — Post Install Setup
3. CachyOS Wiki — Dual GPU Setup
4. CachyOS Wiki — BTRFS Snapshots
5. CachyOS GitHub — linux-cachyos README
6. CachyOS Forum — Kernel Differences
7. CachyOS Forum — Hybrid Graphics
8. CachyOS Forum — PRIME Offload
9. CachyOS Forum — MUX Switch Discussion
10. MSI Sword 16 HX B14VEKG Specs (MSI Store Malaysia)
11. MSI Sword 16 HX B14VEKG Specs (MSI Store Singapore)
12. MSI Sword 16 HX Review (NotebookCheck.net)
13. MSI Sword 16 HX B14V Review (LaptopMedia)
14. MSI Sword 16 HX Power Profiles (Pokde.Net)
15. msi-mux-switcher GitHub (ElXreno)
16. MSI GPU Switcher NixOS Module
17. ArchWiki — MSI GS66 11UX (MUX switch)
18. ArchWiki — Hybrid Graphics
19. ArchWiki — NVIDIA PRIME
20. ArchWiki — Supergfxctl
21. NVIDIA — Optimus Laptops Guide
22. Hyprland Wiki — NVIDIA Configuration
23. Hyprland Wiki — Multi-GPU Setup
24. Hyprland NVIDIA Environment Variables (GitHub Gist)
25. CachyOS Forum — ASUS BIOS/NVIDIA Issues
26. CachyOS Forum — Ollama Performance
27. bisko.be — Ollama on CachyOS
28. Next Red Hat — PyTorch Containers Guide
29. Distrobox GitHub — NVIDIA Integration
30. NVIDIA Container Toolkit Documentation

---

**Document version:** 3.0 (MSI Sword 16 HX B14VEKG specific)  
**Last updated:** 2026-08-06  
**Your system:** MSI Sword 16 HX B14VEKG — CachyOS + dots-hyprland + AI/ML  
**CPU:** Intel Core i7-14700HX (20C/28T, x86-64-v4)  
**GPU:** NVIDIA RTX 4050 Laptop (6GB, 115W) + Intel UHD iGPU  
**MUX:** Hardware MUX switch (msi-mux-switcher)  
**Boot media:** 8 GB USB (CachyOS 260628, dd-flashed)
