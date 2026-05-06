use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sysinfo::System;

use crate::cpu_topology::CpuTopology;
use crate::gui_events::{CpuLoadSnapshot, EventSender, TestEvent};

/// Spawn a background thread that polls per-CPU load via sysinfo every second
/// and sends `CpuLoadSnapshot` events through `tx`.
///
/// The thread exits cleanly when the channel is closed (i.e. when all receivers
/// are dropped).
pub fn spawn_poller(tx: EventSender, topology: Arc<CpuTopology>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut system = System::new_all();

        loop {
            thread::sleep(Duration::from_secs(1));

            if let Some(snapshot) = poll_once(&mut system, &topology) {
                if tx.send(TestEvent::CpuLoadSnapshot(snapshot)).is_err() {
                    break;
                }
            }
        }
    })
}

fn poll_once(system: &mut System, topology: &CpuTopology) -> Option<CpuLoadSnapshot> {
    system.refresh_cpu_all();

    let logical_count = system.cpus().len();
    if logical_count == 0 {
        return None;
    }

    let logical_usages: Vec<(u32, f32)> = (0..logical_count)
        .map(|i| (i as u32, system.cpus()[i].cpu_usage()))
        .collect();

    build_snapshot(&logical_usages, &topology.core_map)
}

fn build_snapshot(
    logical_usages: &[(u32, f32)],
    core_map: &BTreeMap<u32, Vec<u32>>,
) -> Option<CpuLoadSnapshot> {
    if logical_usages.iter().any(|(_, u)| u.is_nan()) {
        return None;
    }

    let loads = aggregate_load(logical_usages, core_map);
    Some(CpuLoadSnapshot { loads })
}

fn aggregate_load(
    logical_usages: &[(u32, f32)],
    core_map: &BTreeMap<u32, Vec<u32>>,
) -> BTreeMap<u32, f32> {
    let mut phys_load: BTreeMap<u32, f32> = BTreeMap::new();

    for &(logical_id, usage) in logical_usages {
        let phys_id = core_map
            .iter()
            .find(|(_, logicals)| logicals.contains(&logical_id))
            .map(|(&phys, _)| phys);

        if let Some(phys_id) = phys_id {
            let entry = phys_load.entry(phys_id).or_insert(0.0f32);
            if usage > *entry {
                *entry = usage;
            }
        }
    }

    phys_load
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn make_topology() -> CpuTopology {
        let mut core_map = BTreeMap::new();
        core_map.insert(0, vec![0, 1]);
        core_map.insert(1, vec![2, 3]);
        CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "Test CPU".to_string(),
            physical_core_count: 2,
            logical_cpu_count: 4,
            core_map,
            bios_map: BTreeMap::from([(0, 0), (1, 1)]),
            physical_map: BTreeMap::from([(0, 0), (1, 1)]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        }
    }

    /// BDD: Given a running poller, when waiting for an event, then a
    ///      CpuLoadSnapshot is received.
    #[test]
    fn poller_sends_events() {
        let topology = Arc::new(make_topology());
        let (tx, rx) = mpsc::channel();
        let _handle = spawn_poller(tx, topology);

        let event = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("should receive an event within 5 seconds");

        assert!(
            matches!(event, TestEvent::CpuLoadSnapshot(_)),
            "expected CpuLoadSnapshot event, got {:?}",
            event
        );
    }

    /// BDD: Given a running poller, when the receiver is dropped, then the
    ///      poller thread exits cleanly.
    #[test]
    fn clean_exit_on_channel_drop() {
        let topology = Arc::new(make_topology());
        let (tx, rx) = mpsc::channel();
        let handle = spawn_poller(tx, topology);

        drop(rx);
        thread::sleep(Duration::from_secs(2));

        assert!(
            handle.is_finished(),
            "poller thread should exit after the channel is closed"
        );
    }

    /// BDD: Given a fresh System instance, when polling once, then the
    ///      iteration is skipped because cpu_usage returns NaN on the first
    ///      refresh.
    #[test]
    fn nan_handling() {
        let core_map = BTreeMap::from([(0, vec![0, 1])]);
        let usages = [(0, f32::NAN), (1, 50.0f32)];

        let snapshot = build_snapshot(&usages, &core_map);

        assert!(
            snapshot.is_none(),
            "should skip iteration when any cpu_usage is NaN"
        );
    }

    /// BDD: Given logical CPUs belonging to the same physical core with
    ///      different loads, when aggregating, then the physical core load is
    ///      the maximum of its logical CPUs.
    #[test]
    fn aggregate_load_uses_max_per_physical_core() {
        let core_map = BTreeMap::from([(0, vec![0, 1]), (1, vec![2, 3])]);
        let usages = [(0, 10.0f32), (1, 80.0f32), (2, 30.0f32), (3, 40.0f32)];

        let result = aggregate_load(&usages, &core_map);

        assert_eq!(result.len(), 2);
        assert_eq!(*result.get(&0).unwrap(), 80.0);
        assert_eq!(*result.get(&1).unwrap(), 40.0);
    }

    /// BDD: Given a System warmed up with one refresh, when polling a second
    ///      time, then a valid snapshot is returned.
    #[test]
    fn poll_once_returns_valid_snapshot_after_warmup() {
        let mut system = System::new_all();
        let topology = make_topology();

        let _ = poll_once(&mut system, &topology);
        thread::sleep(Duration::from_millis(500));

        let snapshot = poll_once(&mut system, &topology);

        assert!(
            snapshot.is_some(),
            "should return a valid snapshot after warmup"
        );
        let snapshot = snapshot.unwrap();
        assert!(!snapshot.loads.is_empty());
        assert!(
            !snapshot.loads.values().any(|v| v.is_nan()),
            "no load value should be NaN"
        );
    }
}
