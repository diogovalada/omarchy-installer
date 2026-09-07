#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--setup-helper") {
        let result = if args.len() == 4 {
            args[3]
                .parse::<u32>()
                .map_err(|e| e.to_string())
                .and_then(|parent| omarchy_setup_desktop_lib::run_helper(&args[2], parent))
        } else {
            Err("Invalid helper arguments".into())
        };
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    omarchy_setup_desktop_lib::run();
}
