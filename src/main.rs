pub mod cli;
pub mod co_decoder;
pub mod co_heuristic;
pub mod co_offsets;
pub mod co_reader;
pub mod co_tier;
pub mod config_persistence;
pub mod coordinator;
pub mod cpu_load_poller;
pub mod cpu_topology;
pub mod embedded;
pub mod error_parser;
pub mod gui;
pub mod gui_events;
pub mod gui_modal;
pub mod gui_qr;
pub mod gui_theme;
pub mod gui_update_modal;
pub mod gui_widgets;
pub mod hii_extractor;
pub mod hii_question;
pub mod ifr_parser;
pub mod mce_monitor;
pub mod mprime_config;
pub mod mprime_runner;
pub mod signal_handler;
pub mod thermal_monitor;
pub mod thermal_poller;
pub mod uefi_reader;
pub mod updater;

use cpu_topology::detect_cpu_topology;

fn main() {
    // Internal IPC: pkexec re-exec for UEFI reads. The GUI spawns
    // `pkexec core-probe --uefi-only` to read UEFI settings as root,
    // receives JSON on stdout, and exits. No tracing or arg parsing needed.
    if std::env::args().any(|a| a == "--uefi-only") {
        let physical_core_count = match detect_cpu_topology() {
            Ok(topo) => topo.physical_core_count,
            Err(e) => {
                eprintln!(
                    "topology detection failed before UEFI read, defaulting to 16 cores: {e}"
                );
                16
            }
        };
        let settings = uefi_reader::attempt_uefi_read_with_escalation(physical_core_count);
        let json = serde_json::to_string(&settings).unwrap_or_else(|_| {
            r#"{"available":false,"unavailable_reason":"JSON serialization failed"}"#.to_string()
        });
        println!("{json}");
        return;
    }

    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .with_writer(std::io::stderr)
        .init();

    let args: cli::CliArgs = argh::from_env();
    if args.cli {
        let code = match cli::run(args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("Error: {e:#}");
                2
            }
        };
        std::process::exit(code);
    }

    if let Err(e) = gui::run_gui() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
