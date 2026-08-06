# MSI Sword 16 HX B14VEKG — MUX Switch Tool
**Custom Linux MUX Controller**
**August 2026**

---

## The Problem

No actively maintained Linux tool exists for MSI MUX switching:

| Tool | Status | MUX Support |
|------|--------|-------------|
| `msi-gpu-switcher` | ❌ Stalled (Feb 2026, AI-generated Go code, 5 stars) | Only 2 confusing modes (`igpu` is actually Hybrid) |
| `msi-ec` | ✅ Active kernel driver | ❌ No MUX switching (EC access only) |
| `MControlCenter` | ✅ Active GUI | ⚠️ Workaround via ASUS-only `supergfxctl` |
| `supergfxctl` | ❌ Being phased out | ASUS MUX only |
| `envycontrol` | ❌ Abandoned | Optimus only, no MUX hardware control |
| `GhostDeck` | ❌ Windows only | N/A |

**`msi-gpu-switcher` issues:**
- Written by AI, not maintained
- Only 2 modes with confusing naming (`igpu` = Hybrid, not true iGPU-only)
- No error handling or safety checks
- No model detection
- No fallback methods

---

## The Solution: Custom MUX Switcher

Built specifically for **MSI Sword 16 HX B14VEKG** with:
- **Python** — readable, maintainable, no Go toolchain needed
- **Dual-method approach** — UEFI variable + EC register writes
- **Safety checks** — prerequisite validation, dry-run mode, error handling
- **Model detection** — checks firmware, EC driver availability
- **Fallback support** — uses `msi-ec` kernel driver if available, falls back to raw `ec_sys`

---

## How It Works

### Method 1: UEFI Variable Write (Primary)
MSI stores MUX mode in UEFI variable `MsiDCVarData`:
- **GUID:** `DD96BAAF-145E-4F56-B1CF-193256298E99`
- **Path:** `/sys/firmware/efi/efivars/MsiDCVarData-DD96BAAF-145E-4F56-B1CF-193256298E99`
- **Size:** 4 bytes + 4 byte attributes header

### Method 2: EC Register Write (Trigger)
After UEFI write, trigger EC to apply the change:
- **EC switch register:** `0xD1` — write `0xD1` to trigger
- **EC MUX bit:** `0x2E` — toggle bit `0x40`

### Combined Sequence
```
1. Write UEFI variable MsiDCVarData with mode value
2. Write 0xD1 to EC register 0xD1 (trigger switch)
3. Read EC register 0x2E
4. Toggle bit 0x40 in EC register 0x2E
5. Reboot
```

---

## Installation

### Prerequisites

**1. Enable ec_sys with write support:**
```bash
# Check if ec_sys is builtin or module
zgrep EC_SYS /proc/config.gz 2>/dev/null || grep EC_SYS /boot/config-$(uname -r)

# If builtin (CONFIG_EC_SYS=y):
echo "ec_sys.write_support=1" | sudo tee -a /etc/default/grub
sudo grub-mkconfig -o /boot/grub/grub.cfg

# If module (CONFIG_EC_SYS=m):
echo "options ec_sys write_support=1" | sudo tee /etc/modprobe.d/ec_sys.conf
sudo mkinitcpio -P

# Reboot
sudo reboot
```

**2. Verify ec_sys write support:**
```bash
cat /sys/module/ec_sys/parameters/write_support
# Expected: Y
```

**3. Verify debugfs mounted:**
```bash
mount | grep debugfs
# Expected: debugfs on /sys/kernel/debug type debugfs

# If not mounted:
sudo mount -t debugfs none /sys/kernel/debug
```

**4. Verify efivarfs mounted:**
```bash
mount | grep efivarfs
# Expected: efivarfs on /sys/firmware/efi/efivars type efivarfs

# If not mounted:
sudo mount -t efivarfs efivarfs /sys/firmware/efi/efivars
```

### Install the Tool

```bash
# Copy to PATH
sudo cp msi-mux-switcher.py /usr/local/bin/msi-mux-switcher
sudo chmod +x /usr/local/bin/msi-mux-switcher
```

### Verify Installation

```bash
# Check status (no root needed for status)
msi-mux-switcher status

# Expected output:
# === MSI Sword 16 HX B14VEKG MUX Status ===
# efivarfs: ✓
# msi-ec driver: ✓ or ✗
# ec_sys debugfs: ✓
# ...
```

---

## Usage

### Check Current Status
```bash
msi-mux-switcher status
```

Shows:
- Prerequisites (efivarfs, msi-ec, ec_sys)
- UEFI variable value and detected mode
- EC register values
- Active GPU (OpenGL renderer)
- NVIDIA module status

### Switch to Hybrid Mode (Default)
```bash
sudo msi-mux-switcher hybrid
```

**Use for:** Daily driving, web browsing, coding, light AI inference
**Behavior:**
- Display → Intel UHD iGPU
- NVIDIA available via PRIME Offload (`prime-run`)
- Best battery life

**After switching:**
```bash
sudo reboot
```

### Switch to dGPU Mode (Performance)
```bash
sudo msi-mux-switcher dgpu
```

**Use for:** Heavy AI training, gaming, CUDA workloads, external monitors
**Behavior:**
- Display → NVIDIA RTX 4050 directly via MUX
- Intel iGPU idle
- Max performance, no PRIME overhead

**After switching:**
```bash
sudo reboot
```

### Dry Run (Test Without Changes)
```bash
sudo msi-mux-switcher --dry-run dgpu
```

Shows what would be done without making actual changes.

### Debug Mode
```bash
sudo msi-mux-switcher --debug hybrid
```

Shows detailed debug output including:
- UEFI variable paths
- EC register reads/writes
- Exact byte values

---

## Modes Explained

| Mode | Command | Display | NVIDIA | Power | Use Case |
|------|---------|---------|--------|-------|----------|
| **Hybrid** | `sudo msi-mux-switcher hybrid` | Intel UHD | PRIME Offload | Medium | Daily use, light AI |
| **dGPU** | `sudo msi-mux-switcher dgpu` | NVIDIA RTX 4050 | Active direct | High | AI training, gaming |
| **iGPU** | `sudo msi-mux-switcher igpu` | Intel UHD | Off | Lowest | Battery only |

**Switching requires reboot** — the MUX is latched and only applies at boot.

---

## Integration with Hyprland

### Mode-Specific Configs

Create separate Hyprland configs for each mode:

```bash
mkdir -p ~/.config/hypr/config/modes
```

**`~/.config/hypr/config/modes/hybrid.lua`:**
```lua
-- Hybrid: Intel iGPU primary, NVIDIA offload
env = AQ_DRM_DEVICES, /dev/dri/igpu:/dev/dri/dgpu
env = LIBVA_DRIVER_NAME, nvidia
env = __GLX_VENDOR_LIBRARY_NAME, nvidia
env = MOZ_ENABLE_WAYLAND, 1
```

**`~/.config/hypr/config/modes/dgpu.lua`:**
```lua
-- dGPU: NVIDIA direct via MUX
env = AQ_DRM_DEVICES, /dev/dri/dgpu
env = LIBVA_DRIVER_NAME, nvidia
env = __GLX_VENDOR_LIBRARY_NAME, nvidia
env = GBM_BACKEND, nvidia-drm
env = MOZ_ENABLE_WAYLAND, 1
```

### Auto-Load Mode Config

Create script to detect mode and load appropriate config:

```bash
tee ~/.config/hypr/scripts/load-mode-config.sh << 'EOF'
#!/bin/bash
# Detect MUX mode and load appropriate Hyprland config

# Check if NVIDIA is primary renderer
if glxinfo 2>/dev/null | grep -q "NVIDIA"; then
    MODE="dgpu"
else
    MODE="hybrid"
fi

echo "Detected mode: $MODE"

# Load mode-specific config
CONFIG="$HOME/.config/hypr/config/modes/${MODE}.lua"
if [ -f "$CONFIG" ]; then
    echo "Loading $CONFIG"
    # Source the config in Hyprland
fi
EOF
chmod +x ~/.config/hypr/scripts/load-mode-config.sh
```

---

## Troubleshooting

### UEFI Variable Not Found
```bash
# Check if variable exists
ls -la /sys/firmware/efi/efivars/ | grep MsiDCVarData

# If not found, the variable may need to be created first
# Use BIOS to switch modes initially, then the tool can read/write it

# Verify GUID format
# Correct: MsiDCVarData-DD96BAAF-145E-4F56-B1CF-193256298E99
# Some systems use: MsiDCVarData-DD96baaf-145e-4f56-b1cf-193256298e99
```

### EC Write Fails
```bash
# Verify ec_sys loaded with write support
lsmod | grep ec_sys
cat /sys/module/ec_sys/parameters/write_support
# Expected: Y

# If not, reload with write support
sudo modprobe -r ec_sys
sudo modprobe ec_sys write_support=1

# Verify debugfs mounted
mount | grep debugfs
# If not:
sudo mount -t debugfs none /sys/kernel/debug
```

### Black Screen After Switching
```bash
# 1. Disconnect all external monitors
# 2. Boot with external monitors disconnected
# 3. Reconnect after boot

# If still black, switch back via BIOS:
# - Reboot, enter BIOS (Del/F2)
# - Advanced → GPU Mode → Select mode
# - Save and Exit
```

### Mode Not Detected Correctly
```bash
# Manual check
glxinfo | grep "OpenGL renderer string"

# Check NVIDIA module
lsmod | grep nvidia

# Check UEFI variable
sudo hexdump -C /sys/firmware/efi/efivars/MsiDCVarData-*

# Check EC registers
sudo xxd /sys/kernel/debug/ec/ec0/io | head
```

### Permission Denied on UEFI Variable
```bash
# Remove immutable flag
sudo chattr -i /sys/firmware/efi/efivars/MsiDCVarData-*

# Try writing again
sudo msi-mux-switcher dgpu
```

---

## Safety & Risks

**This tool writes to:**
1. UEFI variables (firmware settings)
2. EC registers (embedded controller hardware)

**Risks:**
- Incorrect values can cause boot failures
- EC writes can affect power management, fans, battery
- MUX switch is latched — requires reboot to apply

**Safety features:**
- Prerequisite checks before any writes
- Dry-run mode for testing
- Read-only verification of current state
- Clear warnings before destructive operations

**If something goes wrong:**
1. Boot into BIOS (Del/F2)
2. Reset to optimal defaults
3. Or use BIOS to switch MUX mode back
4. If completely stuck, clear CMOS (last resort)

---

## Technical Details

### UEFI Variable Format
```
Offset 0-3:   Attributes (4 bytes, little-endian)
Offset 4-7:   Mode value (4 bytes, little-endian)
              - 0x00000000 = Hybrid
              - 0x00000001 = dGPU
              - 0x00000002 = iGPU (if supported)
```

### EC Register Map
```
Address 0xD1: Switch trigger
  - Write 0xD1 to trigger MUX switch sequence

Address 0x2E: MUX control
  - Bit 6 (mask 0x40): MUX state
    - 0 = iGPU mode
    - 1 = dGPU mode
```

### Fallback Strategy
```
1. Try msi-ec kernel driver (preferred, safer)
   - /sys/devices/platform/msi-ec/
   - If firmware supported, use driver's interfaces

2. Fall back to raw ec_sys
   - /sys/kernel/debug/ec/ec0/io
   - Direct EC bus access
   - Requires write_support=1

3. If both fail, report error
   - Suggest BIOS switching
   - Open issue on GitHub with firmware version
```

---

## Advanced Usage

### Script Mode Switching

Create convenience script:

```bash
sudo tee /usr/local/bin/gpu-performance << 'EOF'
#!/bin/bash
# Switch to dGPU mode for AI/gaming
sudo msi-mux-switcher dgpu
echo "Reboot required. Run: sudo reboot"
EOF
sudo chmod +x /usr/local/bin/gpu-performance

sudo tee /usr/local/bin/gpu-power-save << 'EOF'
#!/bin/bash
# Switch to hybrid mode for battery saving
sudo msi-mux-switcher hybrid
echo "Reboot required. Run: sudo reboot"
EOF
sudo chmod +x /usr/local/bin/gpu-power-save
```

Usage:
```bash
gpu-performance    # Switch to dGPU
gpu-power-save     # Switch to hybrid
```

### Systemd Service for Mode Switching

Create service to apply mode at boot:

```bash
sudo tee /etc/systemd/system/mux-mode.service << 'EOF'
[Unit]
Description=Apply MUX mode at boot
After=multi-user.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/msi-mux-switcher hybrid
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl enable mux-mode.service
```

---

## Contributing

If this tool doesn't work on your Sword 16 HX B14VEKG:

1. **Check firmware version:**
   ```bash
   sudo dmidecode -t bios | grep "Version"
   ```

2. **Test in BIOS first:**
   - Reboot, enter BIOS
   - Advanced → GPU Mode → Switch modes
   - Save and reboot
   - Verify if mode changes correctly

3. **Collect EC dump:**
   ```bash
   # If msi-ec supports your firmware:
   sudo modprobe msi-ec debug=true
   cat /sys/devices/platform/msi-ec/debug/ec_dump > ec_dump.txt

   # If using ec_sys:
   sudo xxd /sys/kernel/debug/ec/ec0/io > ec_dump.txt
   ```

4. **Open GitHub issue with:**
   - Firmware version
   - EC dump
   - Whether BIOS switching works
   - Error messages from tool

---

## References

1. `msi-gpu-switcher` by ElXreno — original reverse engineering
2. `msi-ec` by BeardOverflow — kernel driver for MSI EC access
3. `MControlCenter` by dmitry-s93 — GUI for MSI laptops
4. MSI Sword 16 HX B14V review (NotebookCheck, LaptopMedia)
5. ArchWiki — Hybrid graphics, NVIDIA PRIME
6. CachyOS Wiki — Dual GPU Setup

---

**File:** `tools/msi-mux-switcher/msi-mux-switcher.py`
**License:** MIT
**Status:** Experimental — test carefully, BIOS fallback available
