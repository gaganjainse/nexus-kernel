//! Resource monitoring for NexusAOS.
//!
//! Reports system pressure (RAM, VRAM, disk, queue depth) so the kernel
//! can make informed decisions about task admission and context budgeting.

use serde::{Deserialize, Serialize};
use sysinfo::System;

/// Hard resource budget ceilings for the target hardware.
///
/// Defaults are calibrated for 16 GB RAM / 6 GB VRAM hardware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBudget {
    /// Maximum RAM usage in MB before refusing new work.
    pub max_ram_mb: u64,

    /// Maximum VRAM usage in MB before refusing model loads.
    pub max_vram_mb: u64,

    /// Maximum context tokens for any single inference request.
    pub max_context_tokens: usize,

    /// Maximum number of tasks in the scheduler queue.
    pub max_queue_depth: usize,

    /// Minimum free disk space in GB before refusing writes.
    pub min_disk_free_gb: u64,

    /// Maximum tool output size in bytes before truncation.
    pub max_tool_output_size: usize,
}

impl Default for ResourceBudget {
    fn default() -> Self {
        Self {
            max_ram_mb: 14_336, // 14 GB ceiling for 16 GB hardware
            max_vram_mb: 5_120, // 5 GB ceiling for 6 GB hardware
            max_context_tokens: 32_768,
            max_queue_depth: 32,
            min_disk_free_gb: 5,
            max_tool_output_size: 1_048_576,
        }
    }
}

impl ResourceBudget {
    /// Check if the system pressure exceeds the RAM budget ceiling.
    pub fn exceeds_ram_budget(pressure: &SystemPressure, budget: &ResourceBudget) -> bool {
        let used_mb = pressure.ram_total_mb.saturating_sub(pressure.ram_available_mb);
        used_mb > budget.max_ram_mb
    }

    /// Check if the system pressure exceeds the VRAM budget ceiling.
    pub fn exceeds_vram_budget(pressure: &SystemPressure, budget: &ResourceBudget) -> bool {
        if pressure.vram_total_mb == 0 {
            return false;
        }
        let used_mb = pressure.vram_total_mb.saturating_sub(pressure.vram_available_mb);
        used_mb > budget.max_vram_mb
    }

    /// Check if the queue depth exceeds the budget ceiling.
    pub fn exceeds_queue_budget(queue_depth: usize, budget: &ResourceBudget) -> bool {
        queue_depth >= budget.max_queue_depth
    }

    /// Check if the disk space is below the budget floor.
    pub fn exceeds_disk_budget(pressure: &SystemPressure, budget: &ResourceBudget) -> bool {
        pressure.disk_available_gb < budget.min_disk_free_gb
    }

    /// Check all budget ceilings and return a list of exceeded resources.
    pub fn check_all(pressure: &SystemPressure, budget: &ResourceBudget) -> Vec<String> {
        let mut exceeded = Vec::new();
        if Self::exceeds_ram_budget(pressure, budget) {
            let used_mb = pressure.ram_total_mb.saturating_sub(pressure.ram_available_mb);
            exceeded.push(format!("RAM: {} MB used, {} MB ceiling", used_mb, budget.max_ram_mb));
        }
        if Self::exceeds_vram_budget(pressure, budget) {
            let used_mb = pressure.vram_total_mb.saturating_sub(pressure.vram_available_mb);
            exceeded.push(format!("VRAM: {} MB used, {} MB ceiling", used_mb, budget.max_vram_mb));
        }
        if Self::exceeds_queue_budget(pressure.queue_depth, budget) {
            exceeded.push(format!(
                "Queue depth: {} >= {} ceiling",
                pressure.queue_depth, budget.max_queue_depth
            ));
        }
        if Self::exceeds_disk_budget(pressure, budget) {
            exceeded.push(format!(
                "Disk: {} GB available, {} GB floor",
                pressure.disk_available_gb, budget.min_disk_free_gb
            ));
        }
        exceeded
    }
}

/// A snapshot of current system resource pressure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPressure {
    /// Available RAM in megabytes.
    pub ram_available_mb: u64,

    /// Total RAM in megabytes.
    pub ram_total_mb: u64,

    /// Available VRAM in megabytes (0 if GPU not detected).
    pub vram_available_mb: u64,

    /// Total VRAM in megabytes (0 if GPU not detected).
    pub vram_total_mb: u64,

    /// Available disk space in gigabytes on the data partition.
    pub disk_available_gb: u64,

    /// Current number of tasks in the scheduler queue.
    pub queue_depth: usize,
}

/// Monitors system resources for admission control and budgeting.
pub struct ResourceMonitor;

impl ResourceMonitor {
    /// Take a snapshot of current system pressure.
    ///
    /// This is a relatively expensive operation (system calls + optional
    /// subprocess for GPU info). Cache the result for a few seconds.
    pub fn snapshot(&self, data_dir: &std::path::Path) -> SystemPressure {
        let mut sys = System::new_all();
        sys.refresh_memory();

        let ram_available_mb = sys.available_memory() / (1024 * 1024);
        let ram_total_mb = sys.total_memory() / (1024 * 1024);

        let (vram_available_mb, vram_total_mb) = Self::query_gpu_vram();

        let disk_available_gb = Self::query_disk_space(data_dir);

        SystemPressure {
            ram_available_mb,
            ram_total_mb,
            vram_available_mb,
            vram_total_mb,
            disk_available_gb,
            queue_depth: 0, // Updated by the scheduler
        }
    }

    /// Query GPU VRAM via multiple vendor tools (nvidia-smi, rocm-smi).
    ///
    /// Returns (available_mb, total_mb). Returns (0, 0) if no GPU tools are available.
    fn query_gpu_vram() -> (u64, u64) {
        for query in [Self::query_nvidia_vram(), Self::query_amd_vram()] {
            if query != (0, 0) {
                return query;
            }
        }
        (0, 0)
    }

    /// Run a GPU VRAM query command and parse the result.
    fn run_vram_query<F>(command: &mut std::process::Command, parse: F) -> (u64, u64)
    where
        F: FnOnce(&str) -> (u64, u64),
    {
        match command.output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                parse(&stdout)
            }
            _ => (0, 0),
        }
    }

    /// Query NVIDIA GPU VRAM via nvidia-smi.
    fn query_nvidia_vram() -> (u64, u64) {
        let mut command = std::process::Command::new("nvidia-smi");
        command.args(["--query-gpu=memory.used,memory.total", "--format=csv,noheader,nounits"]);
        Self::run_vram_query(&mut command, |stdout| {
            if let Some(line) = stdout.lines().next() {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let used: u64 = parts[0].parse().unwrap_or(0);
                    let total: u64 = parts[1].parse().unwrap_or(0);
                    return (total.saturating_sub(used), total);
                }
            }
            (0, 0)
        })
    }

    /// Query AMD GPU VRAM via rocm-smi.
    fn query_amd_vram() -> (u64, u64) {
        let mut command = std::process::Command::new("rocm-smi");
        command.args(["--showmeminfo", "vram"]);
        Self::run_vram_query(&mut command, |stdout| {
            let mut total_mb = 0u64;
            let mut used_mb = 0u64;
            for line in stdout.lines() {
                let line_lower = line.to_lowercase();
                if line_lower.contains("total memory") && line_lower.contains("vram") {
                    if let Some(num) = line.split(':').nth(1) {
                        total_mb = num.trim().parse().unwrap_or(0);
                    }
                } else if line_lower.contains("used memory") && line_lower.contains("vram") {
                    if let Some(num) = line.split(':').nth(1) {
                        used_mb = num.trim().parse().unwrap_or(0);
                    }
                }
            }
            if total_mb > 0 {
                return (total_mb.saturating_sub(used_mb), total_mb);
            }
            (0, 0)
        })
    }

    /// Query available disk space on the root filesystem (cross-platform).
    fn query_disk_space(data_dir: &std::path::Path) -> u64 {
        let data_dir = data_dir.canonicalize().unwrap_or_else(|_| data_dir.to_path_buf());

        let disks = sysinfo::Disks::new_with_refreshed_list();
        if let Some(disk) = disks.iter().find(|d| data_dir.starts_with(d.mount_point())) {
            return disk.available_space() / (1024 * 1024 * 1024);
        }

        let mount_point = data_dir.to_string_lossy();
        #[cfg(target_os = "linux")]
        {
            Self::query_df_space(&["--output=avail", "-BG", &mount_point], 0)
        }

        #[cfg(target_os = "macos")]
        {
            Self::query_df_space(&["-g", &mount_point], 3)
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            0
        }
    }

    /// Run `df` and parse the available space column.
    fn query_df_space(args: &[&str], column: usize) -> u64 {
        let output = std::process::Command::new("df").args(args).output();
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = stdout.lines().nth(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if column < parts.len() {
                        return parts[column].trim_end_matches('G').parse().unwrap_or(0);
                    }
                }
            }
        }
        0
    }

    /// Check if the system is under memory pressure.
    pub fn is_memory_pressure(pressure: &SystemPressure, headroom_mb: u64) -> bool {
        pressure.ram_available_mb < headroom_mb
    }

    /// Check if there is sufficient VRAM for a model load.
    pub fn has_sufficient_vram(pressure: &SystemPressure, needed_mb: u64) -> bool {
        if pressure.vram_total_mb == 0 {
            // No GPU detected — can't check, assume ok for CPU-only inference
            return true;
        }
        pressure.vram_available_mb >= needed_mb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_runs() {
        let monitor = ResourceMonitor;
        let pressure = monitor.snapshot(std::path::Path::new("/"));
        assert!(pressure.ram_total_mb > 0, "should detect RAM");
    }

    #[test]
    fn test_memory_pressure_check() {
        let pressure = SystemPressure {
            ram_available_mb: 1000,
            ram_total_mb: 16000,
            vram_available_mb: 0,
            vram_total_mb: 0,
            disk_available_gb: 100,
            queue_depth: 0,
        };

        assert!(ResourceMonitor::is_memory_pressure(&pressure, 2000));
        assert!(!ResourceMonitor::is_memory_pressure(&pressure, 500));
    }

    #[test]
    fn test_vram_check_no_gpu() {
        let pressure = SystemPressure {
            ram_available_mb: 8000,
            ram_total_mb: 16000,
            vram_available_mb: 0,
            vram_total_mb: 0,
            disk_available_gb: 100,
            queue_depth: 0,
        };

        // No GPU = assume ok
        assert!(ResourceMonitor::has_sufficient_vram(&pressure, 4000));
    }

    #[test]
    fn test_vram_check_with_gpu() {
        let pressure = SystemPressure {
            ram_available_mb: 8000,
            ram_total_mb: 16000,
            vram_available_mb: 3000,
            vram_total_mb: 6000,
            disk_available_gb: 100,
            queue_depth: 0,
        };

        assert!(ResourceMonitor::has_sufficient_vram(&pressure, 2000));
        assert!(!ResourceMonitor::has_sufficient_vram(&pressure, 4000));
    }

    #[test]
    fn test_memory_pressure_exact_boundary() {
        let pressure = SystemPressure {
            ram_available_mb: 2048,
            ram_total_mb: 16000,
            vram_available_mb: 0,
            vram_total_mb: 0,
            disk_available_gb: 100,
            queue_depth: 0,
        };

        // Exact equality: available < headroom? 2048 < 2048 = false
        assert!(!ResourceMonitor::is_memory_pressure(&pressure, 2048));
        // One less: 2048 < 2049 = true
        assert!(ResourceMonitor::is_memory_pressure(&pressure, 2049));
    }

    #[test]
    fn test_vram_exact_boundary() {
        let pressure = SystemPressure {
            ram_available_mb: 8000,
            ram_total_mb: 16000,
            vram_available_mb: 4000,
            vram_total_mb: 8000,
            disk_available_gb: 100,
            queue_depth: 0,
        };

        assert!(ResourceMonitor::has_sufficient_vram(&pressure, 4000));
        assert!(!ResourceMonitor::has_sufficient_vram(&pressure, 4001));
    }

    #[test]
    fn test_system_pressure_default_fields() {
        let pressure = SystemPressure {
            ram_available_mb: 0,
            ram_total_mb: 0,
            vram_available_mb: 0,
            vram_total_mb: 0,
            disk_available_gb: 0,
            queue_depth: 0,
        };

        assert!(ResourceMonitor::is_memory_pressure(&pressure, 1));
        assert!(ResourceMonitor::has_sufficient_vram(&pressure, 1));
    }

    #[test]
    fn test_system_pressure_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let pressure = SystemPressure {
            ram_available_mb: 8000,
            ram_total_mb: 16000,
            vram_available_mb: 3000,
            vram_total_mb: 6000,
            disk_available_gb: 100,
            queue_depth: 5,
        };
        let json = serde_json::to_string(&pressure)?;
        let back: SystemPressure = serde_json::from_str(&json)?;
        assert_eq!(pressure.ram_available_mb, back.ram_available_mb);
        assert_eq!(pressure.vram_available_mb, back.vram_available_mb);
        assert_eq!(pressure.queue_depth, back.queue_depth);
        Ok(())
    }

    #[test]
    fn test_snapshot_returns_reasonable_values() {
        let monitor = ResourceMonitor;
        let pressure = monitor.snapshot(std::path::Path::new("/"));
        assert!(pressure.ram_total_mb > 0, "total RAM should be > 0");
        assert!(pressure.ram_available_mb <= pressure.ram_total_mb, "available <= total");
    }
}
