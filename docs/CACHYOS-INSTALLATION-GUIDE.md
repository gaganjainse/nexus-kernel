# CachyOS + dots-hyprland — Complete Setup
**For: MSI Sword 16 HX B14VEKG**

---

## Pre-Installation

### Backup (do this tonight)
```bash
# Backup SSH keys to phone
tar czf /tmp/ssh-backup.tar.gz -C /home/gagan .ssh
# Transfer ssh-backup.tar.gz to your phone
```

### Create bootable USB
```bash
# Flash CachyOS ISO to USB
sudo dd if=cachyos-desktop-linux-260628.iso of=/dev/sda bs=4M status=progress && sync
```

---

## Installation (tonight)

1. Boot from USB → select **"CachyOS with NVIDIA closed-source Driver"**
2. Install with:
   - **Desktop:** No Desktop
   - **Packages:** Base-devel + Common packages only
   - **Filesystem:** BTRFS + Snapper + LUKS2 encryption
   - **Bootloader:** Limine
3. Reboot into CachyOS

---

## Post-Installation (after midnight)

```bash
# Run the installer
sudo bash /home/gagan/Workspace/nexus-kernel/tools/install.sh
```

This installs everything:
- System update
- NVIDIA drivers + hybrid graphics
- dots-hyprland (illogical-impulse)
- MUX switcher
- Hyprland MUX configuration
- AI/ML stack (CUDA, PyTorch, Ollama)
- Directory organization
- SSH restore
- GitHub repos
- Power management + utilities
- NvChad

**Reboots are handled automatically.**

---

## After Install

1. Reboot → select **Hyprland** at login
2. Test MUX: `sudo msi-mux-switcher status`
3. Test AI: `ollama run qwen2.5:7b`
4. Test GPU: `prime-run glxinfo | grep NVIDIA`
5. Test Neovim: `nvim`

---

## GPU Modes

| Mode | Command | Use |
|------|---------|-----|
| Hybrid | `sudo msi-mux-switcher hybrid` | Daily use, battery |
| dGPU | `sudo msi-mux-switcher dgpu` | AI training, gaming |

Requires reboot after switching.

---

## Directory Structure

```
/home/gagan/
├── Workspace/          # Git repos
├── Projects/           # Future projects
├── Models/             # AI models
├── Datasets/           # Training data
├── Documents/
├── Downloads/
├── Pictures/
├── Videos/
├── Music/
└── .ssh/               # Restored from backup
```

---

## Troubleshooting

**NVIDIA not working:**
```bash
sudo dkms autoinstall
sudo mkinitcpio -P
sudo reboot
```

**Hyprland black screen:**
```bash
cat ~/.cache/hypr/hyprland.log
```

**Ollama GPU not detected:**
```bash
nvidia-smi
sudo systemctl status ollama
sudo usermod -aG video,render $USER
```

---

## Sources
1. CachyOS Wiki — Installation Guide
2. CachyOS Wiki — Post Install Setup
3. CachyOS Wiki — Dual GPU Setup
4. CachyOS GitHub — linux-cachyos README
5. CachyOS Forum — Hybrid Graphics
6. msi-gpu-switcher GitHub (ElXreno)
7. msi-ec kernel driver (BeardOverflow)
8. MControlCenter GitHub (dmitry-s93)
9. ArchWiki — Hybrid Graphics
10. ArchWiki — NVIDIA PRIME
11. NVIDIA — Optimus Laptops Guide
12. Hyprland Wiki — NVIDIA Configuration
13. end-4/dots-hyprland
14. NvChad GitHub
15. topgrade-rs/topgrade
16. MSI Sword 16 HX B14VEKG Specs
17. NotebookCheck Review
18. LaptopMedia Review
19. Distrobox GitHub
20. CachyOS Forum — MUX Switch Discussion

---

**Version:** 1.0  
**Date:** 2026-08-06  
**Hardware:** MSI Sword 16 HX B14VEKG — i7-14700HX + RTX 4050 + MUX switch
