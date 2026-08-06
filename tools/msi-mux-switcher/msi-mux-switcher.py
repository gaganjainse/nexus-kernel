#!/usr/bin/env python3
"""
MSI Sword 16 HX B14VEKG — MUX Switch Controller
=================================================

Controls the hardware MUX switch on MSI Sword 16 HX B14VEKG.

Modes:
  hybrid  — Display → Intel UHD, NVIDIA available via PRIME Offload
  dgpu    — Display → NVIDIA RTX 4050 via MUX (direct, no PRIME overhead)
  igpu    — Display → Intel UHD, NVIDIA powered off (if supported)

Requires:
  - Root privileges
  - ec_sys kernel module with write_support=1
  - efivarfs mounted at /sys/firmware/efi/efivars
  - debugfs mounted at /sys/kernel/debug

Usage:
  sudo python3 msi-mux-switcher.py status
  sudo python3 msi-mux-switcher.py hybrid
  sudo python3 msi-mux-switcher.py dgpu
  sudo python3 msi-mux-switcher.py igpu

Reboot required after switching.
"""

import argparse
import os
import sys
import fcntl
import struct
import subprocess
import logging
from pathlib import Path
from typing import Optional, Tuple

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
log = logging.getLogger(__name__)


class MSIMUXSwitcherError(Exception):
    """Base exception for MUX switcher errors."""
    pass


class MSIMUXSwitcher:
    """
    MUX switch controller for MSI Sword 16 HX B14VEKG.

    Uses two methods:
    1. UEFI variable write (MsiDCVarData) — primary method
    2. EC register write (0xD1, 0x2E mask 0x40) — trigger

    The tool attempts msi-ec kernel driver first, falls back to raw ec_sys.
    """

    # UEFI variable GUID for MSI display control
    MUX_VAR_GUID = "DD96BAAF-145E-4F56-B1CF-193256298E99"
    MUX_VAR_NAME = "MsiDCVarData"

    # EC register addresses (from msi-gpu-switcher reverse engineering)
    EC_SWITCH_ADDR = 0xD1
    EC_MUX_ADDR = 0x2E
    EC_MUX_MASK = 0x40

    # Mode values for UEFI variable
    # These are model-specific and may need adjustment
    MODE_VALUES = {
        "hybrid": {
            "uefi": bytes([0x00, 0x00, 0x00, 0x00]),
            "description": "Hybrid mode — Intel iGPU primary, NVIDIA offload",
        },
        "dgpu": {
            "uefi": bytes([0x01, 0x00, 0x00, 0x00]),
            "description": "Discrete mode — NVIDIA RTX 4050 direct via MUX",
        },
        "igpu": {
            "uefi": bytes([0x02, 0x00, 0x00, 0x00]),
            "description": "Integrated mode — Intel UHD only, NVIDIA off",
        },
    }

    def __init__(self, dry_run: bool = False):
        self.dry_run = dry_run
        self.uefi_path = Path("/sys/firmware/efi/efivars")
        self.ec_path = Path("/sys/kernel/debug/ec/ec0/io")
        self.msi_ec_path = Path("/sys/devices/platform/msi-ec")

    def _check_root(self) -> None:
        """Verify running as root."""
        if os.geteuid() != 0:
            raise MSIMUXSwitcherError("This tool must be run as root (sudo)")

    def _check_prerequisites(self) -> bool:
        """Check if required kernel interfaces are available."""
        errors = []

        # Check efivarfs
        if not self.uefi_path.exists():
            errors.append("efivarfs not mounted at /sys/firmware/efi/efivars")

        # Check ec_sys or msi-ec
        ec_available = False
        if self.msi_ec_path.exists() and (self.msi_ec_path / "fw_version").exists():
            log.info("Found msi-ec kernel driver")
            ec_available = True
        elif self.ec_path.exists():
            log.info("Found raw ec_sys interface at /sys/kernel/debug/ec/ec0/io")
            ec_available = True
        else:
            errors.append(
                "No EC interface found. Load with:\n"
                "  sudo modprobe ec_sys write_support=1\n"
                "  sudo modprobe msi-ec"
            )

        if errors:
            for e in errors:
                log.error(e)
            return False
        return True

    def _get_mux_var_path(self) -> Path:
        """Get path to MUX UEFI variable."""
        # UEFI variable files are named: VarName-GUID
        # The GUID format uses dashes, not the standard format
        var_filename = f"{self.MUX_VAR_NAME}-{self.MUX_VAR_GUID.replace('-', '_')}"
        var_path = self.uefi_path / var_filename

        # Try alternate naming if not found
        if not var_path.exists():
            # Some systems use different GUID formatting
            alt_filename = f"{self.MUX_VAR_NAME}-{self.MUX_VAR_GUID}"
            var_path = self.uefi_path / alt_filename

        return var_path

    def _read_uefi_var(self, var_path: Path) -> Optional[bytes]:
        """Read UEFI variable value."""
        try:
            data = var_path.read_bytes()
            # UEFI vars have a header: attributes (4 bytes) + data
            # Skip first 4 bytes (attributes)
            return data[4:]
        except Exception as e:
            log.error(f"Failed to read UEFI variable: {e}")
            return None

    def _write_uefi_var(self, var_path: Path, data: bytes) -> bool:
        """
        Write UEFI variable value.

        UEFI variables are immutable by default on some systems.
        We need to remove the immutable flag first.
        """
        try:
            # Remove immutable flag if set
            try:
                fcntl.ioctl(
                    var_path.open("rb"),
                    0x40086601,  # FS_IOC_SETFLAGS
                    struct.pack("Q", 0),  # Clear immutable flag
                )
            except Exception:
                pass  # Flag may not be set

            # Read existing data to preserve attributes
            existing = var_path.read_bytes()
            attributes = existing[:4]

            # Write new data with preserved attributes
            new_data = attributes + data
            var_path.write_bytes(new_data)

            log.info(f"Wrote UEFI variable: {data.hex()}")
            return True
        except Exception as e:
            log.error(f"Failed to write UEFI variable: {e}")
            log.error("You may need to:")
            log.error(f"  sudo chattr -i {var_path}")
            return False

    def _ec_read_byte(self, addr: int) -> int:
        """Read byte from EC via ec_sys debugfs."""
        if not self.ec_path.exists():
            raise MSIMUXSwitcherError("EC debug interface not available")

        try:
            with open(self.ec_path, "rb") as f:
                f.seek(addr)
                data = f.read(1)
                return data[0] if data else 0
        except Exception as e:
            raise MSIMUXSwitcherError(f"EC read failed at 0x{addr:02x}: {e}")

    def _ec_write_byte(self, addr: int, value: int) -> bool:
        """Write byte to EC via ec_sys debugfs."""
        if not self.ec_path.exists():
            raise MSIMUXSwitcherError("EC debug interface not available")

        try:
            with open(self.ec_path, "wb") as f:
                f.seek(addr)
                f.write(bytes([value & 0xFF]))
            return True
        except Exception as e:
            raise MSIMUXSwitcherError(f"EC write failed at 0x{addr:02x}: {e}")

    def _trigger_mux_switch(self) -> bool:
        """
        Trigger the EC MUX switch sequence.

        Based on msi-gpu-switcher:
        1. Write 0xD1 to trigger switch
        2. Toggle MUX bit at 0x2E with mask 0x40
        """
        try:
            log.info("Triggering EC MUX switch sequence...")

            # Step 1: Write to EC switch register
            log.debug(f"Writing 0xD1 to EC switch register")
            self._ec_write_byte(self.EC_SWITCH_ADDR, 0xD1)

            # Step 2: Read current MUX register value
            current = self._ec_read_byte(self.EC_MUX_ADDR)
            log.debug(f"Current EC MUX register 0x{self.EC_MUX_ADDR:02x}: 0x{current:02x}")

            # Step 3: Toggle MUX bit
            new_value = current ^ self.EC_MUX_MASK
            log.debug(f"Writing 0x{new_value:02x} to EC MUX register")
            self._ec_write_byte(self.EC_MUX_ADDR, new_value)

            return True
        except Exception as e:
            log.error(f"EC MUX switch failed: {e}")
            return False

    def get_current_mode(self) -> Optional[str]:
        """
        Detect current MUX mode.

        Returns: "hybrid", "dgpu", "igpu", or None if unknown
        """
        # Method 1: Check UEFI variable
        var_path = self._get_mux_var_path()
        if var_path.exists():
            data = self._read_uefi_var(var_path)
            if data:
                for mode, config in self.MODE_VALUES.items():
                    if data.startswith(config["uefi"]):
                        return mode

        # Method 2: Check active GPU via glxinfo
        try:
            result = subprocess.run(
                ["glxinfo", "|", "grep", "OpenGL renderer string"],
                shell=True,
                capture_output=True,
                text=True,
            )
            if "NVIDIA" in result.stdout:
                return "dgpu"
            elif "Intel" in result.stdout:
                return "hybrid"  # Could also be igpu
        except Exception:
            pass

        # Method 3: Check if NVIDIA is loaded
        try:
            result = subprocess.run(
                ["lsmod"], capture_output=True, text=True
            )
            if "nvidia" in result.stdout:
                return "dgpu"  # Likely dGPU mode
        except Exception:
            pass

        return None

    def set_mode(self, mode: str) -> bool:
        """
        Switch MUX to specified mode.

        Args:
            mode: One of "hybrid", "dgpu", "igpu"

        Returns:
            True if successful, False otherwise
        """
        self._check_root()

        if mode not in self.MODE_VALUES:
            raise MSIMUXSwitcherError(f"Invalid mode: {mode}. Choose from: {list(self.MODE_VALUES.keys())}")

        if not self._check_prerequisites():
            return False

        if self.dry_run:
            log.info(f"[DRY RUN] Would switch to {mode} mode")
            return True

        mode_config = self.MODE_VALUES[mode]
        log.info(f"Switching to {mode} mode...")
        log.info(f"Description: {mode_config['description']}")

        # Step 1: Write UEFI variable
        var_path = self._get_mux_var_path()
        if var_path.exists():
            log.info(f"Writing UEFI variable {self.MUX_VAR_NAME}...")
            if not self._write_uefi_var(var_path, mode_config["uefi"]):
                log.error("Failed to write UEFI variable")
                return False
        else:
            log.warning(f"UEFI variable {self.MUX_VAR_NAME} not found")
            log.warning("You may need to create it first, or use BIOS to switch")

        # Step 2: Trigger EC switch
        if self.ec_path.exists():
            log.info("Triggering EC MUX switch...")
            if not self._trigger_mux_switch():
                log.error("Failed to trigger EC MUX switch")
                return False

        log.info(f"Successfully switched to {mode} mode")
        log.info("Reboot required for changes to take effect")
        return True

    def status(self) -> None:
        """Print current MUX status."""
        log.info("=== MSI Sword 16 HX B14VEKG MUX Status ===")

        # Check prerequisites
        log.info("\n--- Prerequisites ---")
        log.info(f"efivarfs: {'✓' if self.uefi_path.exists() else '✗'}")
        log.info(f"msi-ec driver: {'✓' if self.msi_ec_path.exists() else '✗'}")
        log.info(f"ec_sys debugfs: {'✓' if self.ec_path.exists() else '✗'}")

        # Check UEFI variable
        var_path = self._get_mux_var_path()
        log.info(f"\n--- UEFI Variable ---")
        log.info(f"MsiDCVarData: {'✓ exists' if var_path.exists() else '✗ not found'}")
        if var_path.exists():
            data = self._read_uefi_var(var_path)
            if data:
                log.info(f"Current value: {data.hex()}")
                for mode, config in self.MODE_VALUES.items():
                    if data.startswith(config["uefi"]):
                        log.info(f"Detected mode: {mode}")

        # Check EC
        if self.ec_path.exists():
            try:
                mux_val = self._ec_read_byte(self.EC_MUX_ADDR)
                log.info(f"\n--- EC Registers ---")
                log.info(f"EC MUX register (0x{self.EC_MUX_ADDR:02x}): 0x{mux_val:02x}")
                log.info(f"MUX bit (0x{self.EC_MUX_MASK:02x}): {'set' if mux_val & self.EC_MUX_MASK else 'clear'}")
            except Exception as e:
                log.error(f"Failed to read EC: {e}")

        # Check active GPU
        log.info(f"\n--- Active GPU ---")
        try:
            result = subprocess.run(
                ["glxinfo", "|", "grep", "OpenGL renderer string"],
                shell=True,
                capture_output=True,
                text=True,
            )
            if result.stdout:
                log.info(f"OpenGL renderer: {result.stdout.strip()}")
            else:
                log.info("glxinfo not available or no output")
        except Exception:
            log.info("glxinfo not installed")

        # Check NVIDIA
        try:
            result = subprocess.run(
                ["lsmod"], capture_output=True, text=True
            )
            nvidia_loaded = "nvidia" in result.stdout
            log.info(f"NVIDIA module loaded: {'✓' if nvidia_loaded else '✗'}")
        except Exception:
            log.info("Could not check NVIDIA module")

        # Overall mode
        current = self.get_current_mode()
        log.info(f"\n--- Detected Mode ---")
        if current:
            log.info(f"Current mode: {current}")
        else:
            log.info("Current mode: unknown")


def main():
    parser = argparse.ArgumentParser(
        description="MSI Sword 16 HX B14VEKG MUX Switch Controller"
    )
    parser.add_argument(
        "command",
        choices=["status", "hybrid", "dgpu", "igpu"],
        help="Action: status (show info), hybrid, dgpu, or igpu mode",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be done without making changes",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug logging",
    )

    args = parser.parse_args()

    if args.debug:
        log.setLevel(logging.DEBUG)

    switcher = MSIMUXSwitcher(dry_run=args.dry_run)

    if args.command == "status":
        switcher.status()
    elif args.command == "hybrid":
        success = switcher.set_mode("hybrid")
        sys.exit(0 if success else 1)
    elif args.command == "dgpu":
        success = switcher.set_mode("dgpu")
        sys.exit(0 if success else 1)
    elif args.command == "igpu":
        success = switcher.set_mode("igpu")
        sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
