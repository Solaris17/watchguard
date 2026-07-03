use std::{
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn cmd_output(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");

    let git_hash = cmd_output("git", &["rev-parse", "--short", "HEAD"]);
    let rustc_version = cmd_output("rustc", &["--version"]);
    let build_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=WATCHGUARD_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=WATCHGUARD_RUSTC_VERSION={rustc_version}");
    println!("cargo:rustc-env=WATCHGUARD_BUILD_UNIX={build_unix}");
}
