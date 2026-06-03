use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tracing::warn;

use crate::cpu_topology::CpuTopology;
use crate::gui_events::{EventSender, TestEvent};
use crate::thermal_monitor::{self, ThermalMonitor};

/// Spawn a background thread that polls k10temp and cpufreq every 2 seconds
/// and sends `ThermalSnapshot` events through `tx`.
///
/// The thread exits cleanly when the channel is closed (i.e. when all receivers
/// are dropped).
pub fn spawn_poller(
    tx: EventSender,
    topology: Arc<CpuTopology>,
    monitor: ThermalMonitor,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let interval = Duration::from_secs(2);
        loop {
            thread::sleep(interval);

            if tx.send(build_snapshot(&topology, &monitor)).is_err() {
                // Receiver dropped — exit cleanly.
                break;
            }
        }
    })
}

fn build_snapshot(topology: &CpuTopology, monitor: &ThermalMonitor) -> TestEvent {
    let tctl;
    let mut core_temps = BTreeMap::new();
    let mut core_freqs = BTreeMap::new();

    match monitor.read_temps() {
        Ok(snapshot) => {
            tctl = snapshot.tctl;

            for (&physical_core_id, logical_cpus) in &topology.core_map {
                let ccd = thermal_monitor::ccd_for_core(physical_core_id, &topology.core_map);
                let ccd_temp = match ccd {
                    Some(0) => snapshot.tccd1,
                    Some(1) => snapshot.tccd2,
                    _ => snapshot.tctl,
                };
                core_temps.insert(physical_core_id, ccd_temp);

                if let Some(&first_logical) = logical_cpus.first() {
                    match thermal_monitor::read_freq(first_logical) {
                        Ok(freq_khz) => {
                            core_freqs.insert(physical_core_id, freq_khz);
                        }
                        Err(e) => {
                            warn!(%e, physical_core_id, "failed to read CPU frequency");
                        }
                    }
                }
            }
        }
        Err(e) => {
            warn!(%e, "failed to read k10temp; sending zeroed snapshot");
            tctl = 0.0;
        }
    }

    TestEvent::ThermalSnapshot {
        tctl,
        core_temps,
        core_freqs,
    }
}
