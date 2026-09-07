fn main() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../.native-providers");
    let lock = root.join("provider-lock.json");
    println!("cargo:rerun-if-changed={}", lock.display());
    let manifest = std::fs::read_to_string(&lock).unwrap_or_else(|_| "{}".to_owned());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out.join("provider-lock.json"), manifest).unwrap();
    println!(
        "cargo:rustc-env=OMARCHY_PROVIDER_ROOT={}",
        root.join("bundle").display()
    );
    tauri_build::build()
}
