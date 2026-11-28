fn main() {
    saispace_build_info();
}

/// Exposes the short commit as `SAI_COMMIT` ("unknown" outside a git checkout).
fn saispace_build_info() {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string());
    println!("cargo:rustc-env=SAI_COMMIT={commit}");
    // The release workflow stamps each automatic release (`0.1.7`); local
    // builds keep the crate version.
    let version = std::env::var("SAI_RELEASE_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=SAI_VERSION={version}");
    println!("cargo:rerun-if-env-changed=SAI_RELEASE_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
}
