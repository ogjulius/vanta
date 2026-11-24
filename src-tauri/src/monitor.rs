//! Lightweight system metrics. Nothing polls in the background: metrics are
//! sampled only when the Performance page asks for them, at the user's interval.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuMetrics {
    pub name: String,
    pub source: String,
    pub utilization: Option<f32>,
    pub temperature: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub power_watts: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub game_id: String,
    pub game_name: String,
    pub pid: u32,
    pub process_name: String,
    pub process_count: usize,
    /// Share of total CPU capacity (0–100).
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMetrics {
    pub cpu_name: String,
    pub cpu_count: usize,
    pub cpu_percent: f32,
    pub per_core: Vec<f32>,
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub uptime_secs: u64,
    pub load_average: [f64; 3],
    pub gpus: Vec<GpuMetrics>,
    pub gpu_note: Option<String>,
    pub games: Vec<ProcessMetrics>,
}

pub struct Monitor {
    sys: System,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_all();
        Monitor { sys }
    }

    /// Sample CPU/RAM and, if games are running, their process trees.
    pub fn sample(&mut self, running: &[(String, String, u32)]) -> SystemMetrics {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        let mut games = vec![];
        if !running.is_empty() {
            self.sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );
            let ncpu = self.sys.cpus().len().max(1) as f32;
            for (game_id, game_name, root) in running {
                let root_pid = Pid::from_u32(*root);
                let Some(root_proc) = self.sys.process(root_pid) else {
                    continue;
                };
                // Collect the whole process tree under the root (wine/proton spawn many).
                let mut tree: HashSet<Pid> = HashSet::from([root_pid]);
                loop {
                    let before = tree.len();
                    for (pid, p) in self.sys.processes() {
                        if let Some(parent) = p.parent() {
                            if tree.contains(&parent) {
                                tree.insert(*pid);
                            }
                        }
                    }
                    if tree.len() == before {
                        break;
                    }
                }
                let (mut cpu, mut mem) = (0f32, 0u64);
                let mut heaviest = (0u64, root_proc.name().to_string_lossy().into_owned());
                for pid in &tree {
                    if let Some(p) = self.sys.process(*pid) {
                        cpu += p.cpu_usage();
                        mem += p.memory();
                        if p.memory() > heaviest.0 {
                            heaviest = (p.memory(), p.name().to_string_lossy().into_owned());
                        }
                    }
                }
                games.push(ProcessMetrics {
                    game_id: game_id.clone(),
                    game_name: game_name.clone(),
                    pid: *root,
                    process_name: heaviest.1,
                    process_count: tree.len(),
                    cpu_percent: (cpu / ncpu).min(100.0),
                    memory_bytes: mem,
                });
            }
        }
        let la = System::load_average();
        SystemMetrics {
            cpu_name: self
                .sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_default(),
            cpu_count: self.sys.cpus().len(),
            cpu_percent: self.sys.global_cpu_usage(),
            per_core: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            memory_total: self.sys.total_memory(),
            memory_used: self.sys.used_memory(),
            memory_available: self.sys.available_memory(),
            swap_total: self.sys.total_swap(),
            swap_used: self.sys.used_swap(),
            uptime_secs: System::uptime(),
            load_average: [la.one, la.five, la.fifteen],
            gpus: vec![],
            gpu_note: None,
            games,
        }
    }
}

/// Parse `nvidia-smi --query-gpu=name,utilization.gpu,temperature.gpu,memory.used,memory.total,power.draw --format=csv,noheader,nounits`.
pub fn parse_nvidia_smi(out: &str) -> Vec<GpuMetrics> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f32>().ok());
            GpuMetrics {
                name: f.first().unwrap_or(&"NVIDIA GPU").to_string(),
                source: "nvidia-smi".into(),
                utilization: num(1),
                temperature: num(2),
                memory_used: num(3).map(|m| (m as u64) * 1024 * 1024),
                memory_total: num(4).map(|m| (m as u64) * 1024 * 1024),
                power_watts: num(5),
            }
        })
        .collect()
}

async fn nvidia() -> Option<Vec<GpuMetrics>> {
    let bin = crate::platforms::which("nvidia-smi")?;
    let fut = tokio::process::Command::new(bin)
        .args([
            "--query-gpu=name,utilization.gpu,temperature.gpu,memory.used,memory.total,power.draw",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let out = tokio::time::timeout(Duration::from_secs(3), fut)
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let gpus = parse_nvidia_smi(&String::from_utf8_lossy(&out.stdout));
    (!gpus.is_empty()).then_some(gpus)
}

fn read_num(p: &Path) -> Option<f64> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// AMD (amdgpu) metrics from sysfs.
fn amdgpu() -> Vec<GpuMetrics> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dev = e.path().join("device");
        let Some(busy) = read_num(&dev.join("gpu_busy_percent")) else {
            continue;
        };
        let temp = std::fs::read_dir(dev.join("hwmon"))
            .ok()
            .and_then(|mut h| h.next())
            .and_then(|h| h.ok())
            .and_then(|h| read_num(&h.path().join("temp1_input")))
            .map(|t| (t / 1000.0) as f32);
        out.push(GpuMetrics {
            name: format!("AMD GPU ({name})"),
            source: "amdgpu sysfs".into(),
            utilization: Some(busy as f32),
            temperature: temp,
            memory_used: read_num(&dev.join("mem_info_vram_used")).map(|v| v as u64),
            memory_total: read_num(&dev.join("mem_info_vram_total")).map(|v| v as u64),
            power_watts: None,
        });
    }
    out
}

pub async fn gpus() -> (Vec<GpuMetrics>, Option<String>) {
    let mut all = nvidia().await.unwrap_or_default();
    all.extend(amdgpu());
    if all.is_empty() {
        (all, Some("GPU metrics are unavailable: no nvidia-smi and no amdgpu sysfs counters were found.".into()))
    } else {
        (all, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_output() {
        let g = parse_nvidia_smi("NVIDIA GeForce RTX 4070, 37, 54, 1024, 12282, 45.12\n");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].utilization, Some(37.0));
        assert_eq!(g[0].temperature, Some(54.0));
        assert_eq!(g[0].memory_total, Some(12282 * 1024 * 1024));
        let g = parse_nvidia_smi("Some GPU, [N/A], [N/A], 1, 2, [N/A]");
        assert_eq!(
            g[0].utilization, None,
            "unavailable values are not invented"
        );
    }

    #[test]
    fn samples_system() {
        let mut m = Monitor::new();
        let me = std::process::id();
        let s = m.sample(&[("g".into(), "Test".into(), me)]);
        assert!(s.memory_total > 0);
        assert!(s.cpu_count > 0);
        assert_eq!(s.games.len(), 1);
        assert!(s.games[0].memory_bytes > 0);
    }
}
