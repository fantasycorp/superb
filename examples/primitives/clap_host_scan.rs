//! Scans the default CLAP directories, plus `CLAP_PATH` and any directories
//! on the command line. With no audio effect it returns. Otherwise it lists
//! that plugin's parameters and renders one second of a 440 Hz sine through it.
//!
//! ```text
//! cargo run --example clap_host_scan -- [dir ...] [--id com.vendor.plugin]
//! ```

use std::path::PathBuf;

use superb::primitives::graph::nodes::SineNode;
use superb::primitives::graph::AudioGraph;
use superb::primitives::io::{offline, Engine};
use superb::primitives::plugins::clap::ClapBundle;
use superb::primitives::plugins::scan::{
    default_search_paths, scan_default, scan_directory, ScanReport,
};
use superb::primitives::plugins::{PluginFormat, PluginInstance, PluginNode};
use superb::primitives::transport::Transport;
use superb::primitives::{Result, SampleRate};

fn main() -> Result<()> {
    // Extra directories, and an optional `--id` that consumes the next token.
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut wanted_id: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--id" {
            wanted_id = args.next();
        } else {
            dirs.push(PathBuf::from(a));
        }
    }

    // `+` means the directory exists. Defaults that are missing still print.
    println!("default CLAP search paths:");
    for p in default_search_paths(PluginFormat::Clap) {
        println!("  {} {}", if p.is_dir() { "+" } else { "-" }, p.display());
    }

    // OS defaults and `CLAP_PATH`, then each extra directory. A directory
    // that cannot be scanned is skipped.
    let mut report: ScanReport = scan_default(PluginFormat::Clap)?;
    for dir in &dirs {
        match scan_directory(dir, PluginFormat::Clap) {
            Ok(part) => {
                report.plugins.extend(part.plugins);
                report.failures.extend(part.failures);
            }
            Err(e) => println!("skipping {}: {e}", dir.display()),
        }
    }
    println!(
        "\nfound {} plugin(s), {} bundle(s) failed to load",
        report.plugins.len(),
        report.failures.len()
    );
    for d in &report.plugins {
        println!(
            "  {:<40} {:<24} v{:<8} [{}]  {}",
            d.id,
            d.name,
            d.version,
            d.features.join(", "),
            d.path.display()
        );
    }
    for (path, why) in &report.failures {
        println!("  ! {}: {why}", path.display());
    }

    // `--id` if one was given, otherwise the first audio effect. Neither
    // is a failed run: an empty scan returns here.
    let chosen = match &wanted_id {
        Some(id) => report.plugins.iter().find(|d| &d.id == id),
        None => report.plugins.iter().find(|d| d.is_audio_effect()),
    };
    let Some(desc) = chosen else {
        println!("\nnothing to instantiate (no audio effect found); pass a directory or --id to pick one");
        return Ok(());
    };

    println!("\ninstantiating {} ({})", desc.name, desc.id);
    let bundle = ClapBundle::load(&desc.path)?;
    let plugin = bundle.instantiate(&desc.id)?;
    println!("ports: {:?}", plugin.ports());
    println!("{} parameter(s):", plugin.params().len());
    for p in plugin.params() {
        let value = plugin.param_value(p.id).unwrap_or(f64::NAN);
        let flags = [
            (p.automatable, "auto"),
            (p.stepped, "stepped"),
            (p.hidden, "hidden"),
            (p.readonly, "ro"),
            (p.bypass, "bypass"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join(",");
        println!(
            "  #{:<6} {:<32} {:>10.4} in [{}, {}] default {} {}",
            p.id, p.name, value, p.min, p.max, p.default, flags
        );
    }

    // One second at 48 kHz is 48000 frames, in blocks of 256. The sine is
    // 440 Hz at amplitude 0.5, with one channel per plugin input (at least
    // one). A plugin with no input is rendered on its own.
    let rate = SampleRate::HZ_48K;
    assert_eq!(rate.seconds_to_samples(1.0), 48_000);
    let mut graph = AudioGraph::new();
    let sine = SineNode::new(440.0, 0.5, plugin.ports().inputs.max(1));
    assert!((sine.frequency().get() - 440.0).abs() < 1e-6);
    assert!((sine.amplitude().get() - 0.5).abs() < 1e-6);
    let osc = graph.add_node(Box::new(sine));
    let fx = graph.add_node(Box::new(PluginNode::new(Box::new(plugin))));
    if graph.connect(osc, fx).is_err() {
        println!("plugin has no audio input (instrument?) — rendering it unconnected");
    }
    graph.set_output(fx)?;
    let mut engine = Engine::new(graph, Transport::new(rate, 120.0));
    engine.transport_mut().play();
    let frames = rate.hz() as usize;
    let audio = offline::render(&mut engine, rate, 256, frames)?;
    assert_eq!(frames, 48_000);
    assert_eq!(audio.frames(), 48_000);
    let latency = engine.processor().latency_frames();
    println!(
        "\nrendered {} frames x {} ch through the plugin: peak {:.4}, reported latency {} frames",
        audio.frames(),
        audio.channels(),
        audio.peak(),
        latency
    );
    Ok(())
}
