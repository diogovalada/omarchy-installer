//! Read-only network smoke: discovery, or verification of an existing ISO.
use omarchy_release_client::{resolve_current, verify_existing};
use std::sync::atomic::AtomicBool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let release = resolve_current()?;
    println!("{}", serde_json::to_string_pretty(&release)?);
    if let Some(path) = std::env::args_os().nth(1) {
        let started = std::time::Instant::now();
        let image = verify_existing(&release, path, &AtomicBool::new(false), |p| {
            if p.received_bytes == 0 || p.received_bytes == p.total_bytes {
                eprintln!("{:?}: {}/{}", p.phase, p.received_bytes, p.total_bytes);
            }
        })?;
        println!("{}", serde_json::to_string_pretty(&image)?);
        println!(
            "Verification seconds: {:.3}",
            started.elapsed().as_secs_f64()
        );
    }
    Ok(())
}
