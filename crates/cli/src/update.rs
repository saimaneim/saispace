//! `sai update`: installs the latest GitHub release (sai, saispace-mcp and
//! the desktop app) next to the running `sai`, through the same checksummed
//! `install.sh` as a first install. Uses `curl`, so no HTTP stack is linked.

use crate::{CliError, CliResult};
use clap::Args;
use std::process::{Command, ExitCode};

const REPO: &str = "saimaneim/saispace";

#[derive(Args)]
pub struct UpdateArgs {
    #[arg(long, help = "Solo comprueba si hay una versión nueva")]
    check: bool,
}

fn latest_tag() -> CliResult<String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            &format!("https://api.github.com/repos/{REPO}/releases/latest"),
        ])
        .output()
        .map_err(|e| CliError::Usage(format!("no puedo ejecutar curl: {e}")))?;
    if !output.status.success() {
        return Err(CliError::Usage(
            "no encuentro la última versión en GitHub".into(),
        ));
    }
    let release: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    release["tag_name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| CliError::Usage("la respuesta de GitHub no trae versión".into()))
}

/// `v0.1.7` against `0.1.7 (abc12345)`.
fn is_current(tag: &str, version: &str) -> bool {
    let installed = version.split_whitespace().next().unwrap_or_default();
    tag.trim_start_matches('v') == installed
}

/// # Errors
///
/// Returns an error if the release cannot be looked up or installed.
pub fn run(args: &UpdateArgs) -> CliResult<ExitCode> {
    let tag = latest_tag()?;
    if is_current(&tag, crate::VERSION) {
        println!("Ya tienes la última versión ({tag})");
        return Ok(ExitCode::SUCCESS);
    }
    if args.check {
        println!("Hay una versión nueva: {tag} (tienes {})", crate::VERSION);
        return Ok(ExitCode::SUCCESS);
    }
    if cfg!(windows) {
        return Err(CliError::Usage(format!(
            "en Windows descarga {tag} desde https://github.com/{REPO}/releases/latest"
        )));
    }
    let bin_dir = std::env::current_exe()?
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| CliError::Usage("no sé dónde está instalado sai".into()))?;
    let script = format!("https://github.com/{REPO}/releases/download/{tag}/install.sh");
    let status = Command::new("sh")
        .args(["-c", "curl -fsSL \"$0\" | sh", &script])
        .env("SAISPACE_VERSION", &tag)
        .env("SAISPACE_BIN_DIR", &bin_dir)
        .status()?;
    Ok(if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::is_current;

    #[test]
    fn compares_tag_with_installed_version() {
        assert!(is_current("v0.1.7", "0.1.7 (abc12345)"));
        assert!(!is_current("v0.1.8", "0.1.7 (abc12345)"));
    }
}
