# CachyOS + dots-hyprland — Instruction Sets
**For: MSI Sword 16 HX B14VEKG**
**Generated: 2026-08-06**

---

## Pre-Requisites Checklist

Before starting, confirm:
- [ ] Bootable USB created with CachyOS 260628 ISO ( flashed to `/dev/sda`)
- [ ] SSH keys backed up to phone as `ssh-backup.tar.gz`
- [ ] Critical data backed up (NexusAOS, SeshaOS, Downloads/docs already on GitHub)
- [ ] BIOS settings configured (Secure Boot OFF, VT-d ON, GPU Mode = Hybrid)
- [ ] 2 GB data limit understood — tonight: install only. After midnight: full setup.

---

## Instruction Set 1: BIOS Configuration

**Action:** Configure BIOS before first boot from USB.

**Steps:**
1. Power off laptop completely.
2. Press and hold **Del** or **F2** repeatedly during power-on.
3. BIOS opens. Navigate with arrow keys.

**Settings to change:**

| Setting | Location | Value | Notes |
|---------|----------|-------|-------|
| Secure Boot | Security tab | Disabled | Required for NVIDIA DKMS |
| VT-d | Advanced → CPU Configuration | Enabled | IOMMU for GPU switching |
| GPU Mode | Advanced → System Agent | MSHybrid (Hybrid) | Default, flexible |
| Fast Boot | Boot tab | Disabled | Prevents boot issues |
| CSM | Boot tab | Disabled | UEFI only |
| TPM | Security tab | Enabled (fTPM) | Keep enabled |

**Exit:**
- Press **F10** → "Save Changes and Reset"
- System reboots.

**Verify:** BIOS posts, then proceed to boot menu.

---

## Instruction Set 2: Boot from USB

**Action:** Boot into CachyOS live environment from USB.

**Steps:**
1. After reboot, press **F11** repeatedly (MSI boot menu key).
2. Boot menu appears. Select **USB drive** (e.g., "UEFI: Cruzer Blade").
3. GRUB/Limine menu from CachyOS ISO appears.
4. Select:
   ```
   CachyOS with NVIDIA closed-source Driver (latest cards only 900+)
   ```
5. CachyOS live environment loads (takes ~1-2 minutes).

**Verify:**
```bash
# In live environment, open terminal and run:
lspci | grep -i vga
# Should show: Intel UHD Graphics + NVIDIA RTX 4050

# Check NVIDIA driver loaded
nvidia-smi
# Should show RTX 4050 with driver version
```

**If `nvidia-smi` fails:** Reboot and select correct boot option. The default "CachyOS" uses nouveau, not nvidia.

---

## Instruction Set 3: CachyOS Installation

**Action:** Install CachyOS with minimal packages.

**Steps:**

1. **Launch installer:**
   - Double-click "Install CachyOS" icon on desktop
   - Or run `calamares` in terminal

2. **Language/Region/Keyboard:**
   - Select your language
   - Timezone: Select your region
   - Keyboard: English (US) or your layout

3. **Desktop Environment:**
   - Select **"No Desktop"**
   - Reason: dots-hyprland installs its own stack

4. **Additional Packages:**
   - Check ONLY: **✅ Base-devel + Common packages**
   - Uncheck: ❌ CachyOS Packages, ❌ KDE-Desktop, ❌ Everything else

5. **Partitioning:**
   - Select **"Erase disk"**
   - Filesystem: **BTRFS**
   - Enable **Snapper**: ON
   - Enable **Encryption**: ON (LUKS2) — recommended for laptops
   - Bootloader: **Limine**

   **⚠️ WARNING:** This erases everything on the NVMe. Your Ubuntu data will be gone.

6. **User setup:**
   - Username: `gagan`
   - Password: (your choice)
   - Computer name: (your choice)

7. **Review summary:**
   - Verify: No Desktop, Base-devel only, BTRFS+Snapper+Encryption, Limine
   - Click **"Install Now"**

8. **Wait for install:**
   - Takes ~10-15 minutes
   - No internet required

9. **Reboot when done:**
   - Remove USB when prompted
   - System reboots into new CachyOS installation

**After reboot:**
- You see TTY login prompt: `gagan login:`
- Enter password
- **STOP HERE. Do not install anything else tonight.**

**Verify installation:**
```bash
# Check kernel version
uname -r
# Expected: 7.0.x-cachyos or similar

# Check BTRFS subvolumes
sudo btrfs subvolume list /
# Expected: @, @home, @cache, @tmp, @log, @snapshots

# Check Snapper
sudo snapper list
# Expected: root configuration with snapshots

# Check GPUs
lspci | grep -i vga
# Expected: Intel UHD Graphics + NVIDIA RTX 4050

# Check network
ping -c 3 archlinux.org
# Expected: Replies from archlinux.org
```

---

## Instruction Set 4: System Update (After Midnight)

**Prerequisite:** Wait until after midnight for unlimited data.

**Action:** Update system to latest packages.

**Steps:**
```bash
# 1. Full system update
sudo pacman -Syu

# Expected output: Downloads ~1 GB of updates
# Expected time: 5-10 minutes

# 2. Reboot if kernel was updated
sudo reboot
```

**Verify:**
```bash
# After reboot, check kernel updated
uname -r
# Should show newer version than before

# Check for pending updates
sudo pacman -Qu
# Should show nothing (system is up to date)
```

---

## Instruction Set 5: NVIDIA Drivers + Hybrid Graphics

**Action:** Install proprietary NVIDIA drivers and configure hybrid graphics.

**Steps:**

1. **Install NVIDIA DKMS drivers:**
   ```bash
   sudo pacman -S nvidia-dkms linux-cachyos-headers nvidia-utils lib32-nvidia-utils
   ```
   - Downloads ~300 MB
   - DKMS auto-recompiles modules on kernel updates

2. **Configure mkinitcpio for Intel + NVIDIA hybrid:**
   ```bash
   # Edit mkinitcpio.conf
   sudo nano /etc/mkinitcpio.conf
   ```
   
   Find line:
   ```bash
   MODULES=(nvidia nvidia_modeset nvidia_uvm nvidia_drm)
   ```
   
   Change to:
   ```bash
   MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)
   ```
   
   **Why:** Loads Intel i915 before NVIDIA modules. Fixes Electron/Chromium stalls on hybrid graphics.

3. **Rebuild initramfs:**
   ```bash
   sudo mkinitcpio -P
   ```
   - Takes ~1-2 minutes

4. **Add kernel parameters for NVIDIA:**
   ```bash
   # For Limine bootloader
   sudo nano /boot/loader/entries/*.conf
   ```
   
   Find `options` line, add:
   ```bash
   nvidia-drm.modeset=1 nvidia.NVreg_PreserveVideoMemoryAllocations=1
   ```
   
   **Parameters explained:**
   - `nvidia-drm.modeset=1` — Enables DRM kernel mode setting for Wayland
   - `nvidia.NVreg_PreserveVideoMemoryAllocations=1` — Fixes suspend/resume

5. **Reboot:**
   ```bash
   sudo reboot
   ```

**Verify:**
```bash
# Check NVIDIA driver loaded
nvidia-smi
# Expected: Shows RTX 4050, driver version, CUDA version

# Check modules loaded
lsmod | grep nvidia
# Expected: nvidia, nvidia_modeset, nvidia_uvm, nvidia_drm

# Verify DRM modeset
cat /sys/module/nvidia_drm/parameters/modeset
# Expected: Y

# Verify i915 loaded before nvidia
grep "MODULES" /etc/mkinitcpio.conf
# Expected: MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)
```

---

## Instruction Set 6: Custom MUX Switcher Installation

**Action:** Install tool to control MUX switch.

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

**Steps:**
```bash
# 1. Copy custom MUX switcher to PATH
sudo cp /home/gagan/Workspace/nexus-kernel/tools/msi-mux-switcher/msi-mux-switcher.py /usr/local/bin/msi-mux-switcher
sudo chmod +x /usr/local/bin/msi-mux-switcher

# 2. Verify installation
msi-mux-switcher status
# Expected: Shows prerequisite status and detected mode
```

**Usage:**
```bash
# Check current status (no root needed)
msi-mux-switcher status

# Switch to dGPU mode (performance, display wired to NVIDIA)
sudo msi-mux-switcher dgpu

# Switch to Hybrid mode (balanced, display on Intel, NVIDIA offload)
sudo msi-mux-switcher hybrid

# Dry run (test without changes)
sudo msi-mux-switcher --dry-run dgpu

# Reboot required after switching
sudo reboot
```

**Verify MUX mode:**
```bash
# After reboot, check which GPU is primary
# In Hybrid mode:
glxinfo | grep "OpenGL renderer string"
# Expected: Intel UHD Graphics

# In dGPU mode:
glxinfo | grep "OpenGL renderer string"
# Expected: NVIDIA RTX 4050
```

**⚠️ WARNING:** This tool writes to UEFI/EC. Use at your own risk. If your model is unsupported, check GitHub issues.

**If unsupported:** Use BIOS to switch modes temporarily. Boot into BIOS → Advanced → GPU Mode → Select mode → Save and Exit.

---

## Instruction Set 7: dots-hyprland Installation

**Action:** Install illogical-impulse dotfiles.

**Steps:**
```bash
# Run installer
bash <(curl -s https://ii.clsty.link/get)
```

**During install:**
- Press Enter to accept defaults
- It will ask about packages — accept recommended selections
- Downloads ~1 GB
- Takes ~5-10 minutes

**After install:**
```bash
# Reboot
sudo reboot
```

**First boot into Hyprland:**
1. At login screen (SDDM), click username
2. Select session: **"Hyprland"**
3. Enter password
4. You should see illogical-impulse desktop

**If Hyprland fails to start:**
```bash
# Switch to TTY (Ctrl+Alt+F2)
# Check logs
cat ~/.cache/hypr/hyprland.log

# Reinstall dots-hyprland
bash <(curl -s https://ii.clsty.link/get)
```

---

## Instruction Set 8: Hyprland Configuration for MUX Laptop

**Action:** Configure Hyprland for hybrid graphics with MUX.

**Steps:**

1. **Create stable GPU device paths with udev:**
   ```bash
   # Find your GPU PCI IDs
   lspci -d ::03xx
   # Expected output:
   # 0000:00:02.0 Intel UHD Graphics
   # 0000:01:00.0 NVIDIA RTX 4050
   
   # Create udev rule for iGPU
   sudo tee /etc/udev/rules.d/igpu-device-path.rules << 'EOF'
   KERNEL=="card*", KERNELS=="0000:00:02.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/igpu"
   EOF
   
   # Create udev rule for dGPU
   sudo tee /etc/udev/rules.d/dgpu-device-path.rules << 'EOF'
   KERNEL=="card*", KERNELS=="0000:01:00.0", SUBSYSTEM=="drm", SUBSYSTEMS=="pci", SYMLINK+="dri/dgpu"
   EOF
   
   # Reload udev
   sudo udevadm control --reload-rules
   sudo udevadm trigger
   ```

2. **Verify symlinks created:**
   ```bash
   ls -la /dev/dri/igpu /dev/dri/dgpu
   # Expected: Symlinks to card0 and card1
   ```

3. **Configure Hyprland environment:**
   ```bash
   # Edit Hyprland config
   nano ~/.config/hypr/config/environment.lua
   ```
   
   Add:
   ```lua
   -- GPU selection for MUX laptop
   env = AQ_DRM_DEVICES, /dev/dri/igpu:/dev/dri/dgpu
   
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

4. **Create mode-specific configs (optional but recommended):**
   ```bash
   mkdir -p ~/.config/hypr/config/modes
   
   # Hybrid mode config (default)
   cat > ~/.config/hypr/config/modes/hybrid.lua << 'EOF'
   env = AQ_DRM_DEVICES, /dev/dri/igpu:/dev/dri/dgpu
   env = LIBVA_DRIVER_NAME, nvidia
   env = __GLX_VENDOR_LIBRARY_NAME, nvidia
   EOF
   
   # dGPU mode config
   cat > ~/.config/hypr/config/modes/dgpu.lua << 'EOF'
   env = AQ_DRM_DEVICES, /dev/dri/dgpu
   env = LIBVA_DRIVER_NAME, nvidia
   env = __GLX_VENDOR_LIBRARY_NAME, nvidia
   env = GBM_BACKEND, nvidia-drm
   EOF
   ```

5. **Install nvidia-prime for prime-run:**
   ```bash
   sudo pacman -S nvidia-prime
   ```

6. **Test PRIME Offload:**
   ```bash
   # Should show Intel renderer
   glxinfo | grep "OpenGL renderer string"
   
   # Should show NVIDIA renderer
   prime-run glxinfo | grep "OpenGL renderer string"
   ```

---

## Instruction Set 9: AI/ML Stack Installation

**Action:** Install CUDA, PyTorch, Ollama for AI work.

**Prerequisite:** After midnight, unlimited data.

**Steps:**

1. **Install CUDA toolkit:**
   ```bash
   sudo pacman -S cuda cudnn
   ```
   - Downloads ~3 GB
   - Takes ~5 minutes

2. **Install PyTorch with CUDA:**
   ```bash
   pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu128
   ```
   - Downloads ~2 GB
   - Takes ~5 minutes

3. **Verify PyTorch CUDA:**
   ```bash
   python -c "import torch; print(torch.cuda.is_available()); print(torch.cuda.get_device_name(0))"
   # Expected: True, "NVIDIA GeForce RTX 4050"
   ```

4. **Install Ollama with CUDA:**
   ```bash
   sudo pacman -S ollama ollama-cuda
   sudo systemctl enable --now ollama.service
   sudo usermod -aG video,render $USER
   ```
   - Downloads ~500 MB

5. **Pull AI model:**
   ```bash
   # Recommended for your hardware (6GB VRAM + 16GB RAM)
   ollama pull qwen2.5:7b
   # Downloads ~4 GB
   
   # Test model
   ollama run qwen2.5:7b
   ```

6. **Install additional ML tools:**
   ```bash
   sudo pacman -S jupyterlab
   pip install transformers datasets accelerate huggingface-hub chromadb langchain
   ```

**Verify AI stack:**
```bash
# Check NVIDIA CUDA
nvidia-smi
# Expected: Shows RTX 4050, CUDA version

# Check PyTorch
python -c "import torch; print(torch.cuda.is_available())"
# Expected: True

# Check Ollama
ollama list
# Expected: Shows qwen2.5:7b

# Test Ollama with GPU
ollama run qwen2.5:7b "Hello, test GPU acceleration"
# Expected: Responds quickly (GPU-accelerated)
```

---

## Instruction Set 10: Directory Organization

**Action:** Create organized directory structure for AI work.

**Steps:**
```bash
# Create directories
mkdir -p ~/Workspace
mkdir -p ~/Projects
mkdir -p ~/Models/ollama
mkdir -p ~/Models/huggingface
mkdir -p ~/Models/checkpoints
mkdir -p ~/Datasets/raw
mkdir -p ~/Datasets/processed
mkdir -p ~/Datasets/experiments
mkdir -p ~/Documents
mkdir -p ~/Downloads
mkdir -p ~/Pictures
mkdir -p ~/Videos
mkdir -p ~/Music

# Set up Ollama model directory
# Option 1: Use default (~/.ollama)
# Option 2: Symlink to ~/Models/ollama
ln -s ~/.ollama ~/Models/ollama

# Verify structure
ls -la ~/
```

**Result:**
```
/home/gagan/
├── Workspace/          # Git repos
├── Projects/           # Non-Git projects
├── Models/             # AI models
├── Datasets/           # Training data
├── Documents/
├── Downloads/
├── Pictures/
├── Videos/
├── Music/
├── .ssh/               # Restore from phone
└── .config/            # Managed by dots-hyprland
```

---

## Instruction Set 11: Restore SSH Keys

**Action:** Restore SSH keys from phone backup.

**Prerequisite:** `ssh-backup.tar.gz` on phone.

**Steps:**
```bash
# 1. Transfer ssh-backup.tar.gz from phone to laptop
# Use USB cable, Telegram, or any file transfer method
# Save to ~/Downloads/

# 2. Extract
cd ~
tar xzf ~/Downloads/ssh-backup.tar.gz

# 3. Verify
ls -la ~/.ssh/
# Expected: id_ed25519, id_ed25519.pub, known_hosts, config

# 4. Set permissions
chmod 700 ~/.ssh
chmod 600 ~/.ssh/id_ed25519
chmod 644 ~/.ssh/id_ed25519.pub

# 5. Add to ssh-agent
eval "$(ssh-agent -s)"
ssh-add ~/.ssh/id_ed25519

# 6. Verify GitHub access
ssh -T git@github.com
# Expected: "Hi gaganjainse! You've successfully authenticated..."
```

---

## Instruction Set 12: Re-Clone GitHub Repos

**Action:** Clone all projects from GitHub.

**Prerequisite:** SSH keys restored and working.

**Steps:**
```bash
# 1. Create Workspace directory
mkdir -p ~/Workspace
cd ~/Workspace

# 2. Clone repos
git clone git@github.com:gaganjainse/nexus-kernel.git
git clone git@github.com:gaganjainse/NexusAOS.git
git clone git@github.com:gaganjainse/SeshaOS.git

# 3. Verify
ls -la ~/Workspace/
# Expected: nexus-kernel/, NexusAOS/, SeshaOS/

# 4. Check git status in each
cd ~/Workspace/nexus-kernel && git status
cd ~/Workspace/NexusAOS && git status
cd ~/Workspace/SeshaOS && git status
# Expected: All clean, synced with origin
```

---

## Instruction Set 13: Power Management Configuration

**Action:** Configure power profiles for optimal battery/performance.

**Steps:**
```bash
# 1. Install power-profiles-daemon
sudo pacman -S power-profiles-daemon
sudo systemctl enable --now power-profiles-daemon.service

# 2. Check available profiles
powerprofilesctl list
# Expected: performance, balanced, power-saver

# 3. Set default profile (balanced for daily use)
powerprofilesctl set balanced

# 4. For gaming/AI workloads, switch to performance
powerprofilesctl set performance

# 5. For battery saving, switch to power-saver
powerprofilesctl set power-saver
```

**Configure ZRAM for 16GB RAM:**
```bash
# Check current ZRAM usage
zramctl

# If running out of RAM with large models, add real swap:
sudo fallocate -l 8G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Add to fstab for persistence
echo '/swapfile none swap defaults 0 0' | sudo tee -a /etc/fstab
```

**Verify power management:**
```bash
# Check current profile
powerprofilesctl get
# Expected: balanced

# Check ZRAM
zramctl
# Expected: Shows swap device

# Check swap
swapon --show
# Expected: Shows swapfile if created
```

---

## Instruction Set 14: Backup Strategy

**Action:** Set up automated backups.

**Steps:**

1. **Install restic:**
   ```bash
   sudo pacman -S restic
   ```

2. **Initialize backup repository (on external drive):**
   ```bash
   # Mount external drive
   sudo mount /dev/sdb1 /mnt/backup
   
   # Initialize restic repo
   restic init --repo /mnt/backup/restic-repo
   ```

3. **Create backup script:**
   ```bash
   tee ~/backup.sh << 'EOF'
   #!/bin/bash
   # Backup script for CachyOS
   
   # Backup home directory (excluding cache)
   restic -r /mnt/backup/restic-repo backup /home/gagan \
     --exclude=".cache" \
     --exclude=".npm" \
     --exclude=".cargo" \
     --exclude="snap"
   
   # Backup workspace
   restic -r /mnt/backup/restic-repo backup /home/gagan/Workspace
   
   echo "Backup completed: $(date)"
   EOF
   
   chmod +x ~/backup.sh
   ```

4. **Schedule automated backups (cron):**
   ```bash
   # Edit crontab
   crontab -e
   
   # Add line (backup daily at 2 AM)
   0 2 * * * /home/gagan/backup.sh
   ```

5. **Test backup:**
   ```bash
   ~/backup.sh
   # Expected: "Backup completed: ..."
   ```

---

## Instruction Set 15: Troubleshooting Commands

**Action:** Common troubleshooting procedures.

**MUX Switch Issues:**
```bash
# Check EC module
lsmod | grep ec_sys
# If not loaded:
sudo modprobe ec_sys write_support=1

# Check firmware version
sudo dmidecode -t bios | grep "Version"

# Test msi-mux-switcher
sudo msi-mux-switcher status
```

**NVIDIA Issues:**
```bash
# Rebuild DKMS modules
sudo dkms autoinstall

# Check DKMS status
sudo dkms status | grep nvidia

# Rebuild initramfs
sudo mkinitcpio -P

# Check NVIDIA logs
journalctl -b | grep -i nvidia
```

**Hyprland Issues:**
```bash
# Check Hyprland logs
cat ~/.cache/hypr/hyprland.log

# Check GPU detection
lspci -k | grep -A 3 -i vga

# Test PRIME Offload
prime-run glxinfo | grep "OpenGL renderer string"
```

**Ollama Issues:**
```bash
# Check Ollama service
sudo systemctl status ollama

# Check Ollama logs
journalctl -u ollama -f

# Verify NVIDIA detected by Ollama
ollama list
# Expected: Shows models

# Check GPU usage
nvidia-smi
# Expected: Shows Ollama process using GPU
```

**Network Issues:**
```bash
# Check network status
ip link show

# Connect to WiFi
nmcli dev wifi list
nmcli dev wifi connect "SSID" password "PASSWORD"

# Test connection
ping -c 3 archlinux.org
```

---

## Quick Reference: Complete Command Sequence

**Tonight (Install CachyOS):**
```bash
# From BIOS:
# 1. Disable Secure Boot
# 2. Enable VT-d
# 3. Set GPU Mode = Hybrid
# 4. Boot from USB → Select "CachyOS with NVIDIA closed-source Driver"
# 5. Install: No Desktop + Base-devel only + BTRFS + Snapper + Limine
# 6. Reboot, verify TTY login
# 7. STOP
```

**After Midnight (Full Setup):**
```bash
# 1. System update
sudo pacman -Syu && sudo reboot

# 2. NVIDIA drivers
sudo pacman -S nvidia-dkms linux-cachyos-headers nvidia-utils lib32-nvidia-utils
# Edit /etc/mkinitcpio.conf: MODULES=(i915 nvidia nvidia_modeset nvidia_uvm nvidia_drm)
sudo mkinitcpio -P
# Edit /boot/loader/entries/*.conf: add nvidia-drm.modeset=1 nvidia.NVreg_PreserveVideoMemoryAllocations=1
sudo reboot

# 3. dots-hyprland
bash <(curl -s https://ii.clsty.link/get)
sudo reboot

# 4. msi-mux-switcher (custom Python tool)
sudo cp /home/gagan/Workspace/nexus-kernel/tools/msi-mux-switcher/msi-mux-switcher.py /usr/local/bin/msi-mux-switcher
sudo chmod +x /usr/local/bin/msi-mux-switcher

# 5. Hyprland config for MUX
# Create udev rules, edit ~/.config/hypr/config/environment.lua

# 6. AI stack
sudo pacman -S ollama ollama-cuda python python-pip cuda cudnn
sudo systemctl enable --now ollama.service
ollama pull qwen2.5:7b

# 7. Restore SSH, clone repos, organize directories
```

---

## Notes

- **Data limit:** Tonight uses ~0 MB. After midnight uses ~5-7 GB.
- **Reboots required:** After NVIDIA install, after dots-hyprland, after MUX switch.
- **MUX switching:** Requires reboot. Use `msi-mux-switcher dgpu` for AI/gaming, `msi-mux-switcher hybrid` for battery saving (Hybrid mode). Note: true iGPU-only mode not yet available in this tool.
- **SSH backup:** Already on phone as `ssh-backup.tar.gz`. Restore after midnight.
- **GitHub repos:** All already pushed. Just clone after midnight.

---

**End of Instruction Sets**
