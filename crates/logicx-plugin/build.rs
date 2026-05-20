use std::path::PathBuf;
use std::process::Command;

fn main() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));

    let git_sha = Command::new("git")
        .args(["-C", repo_root.to_str().unwrap_or(".")])
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "dev".into());

    let dirty = Command::new("git")
        .args(["-C", repo_root.to_str().unwrap_or(".")])
        .args(["diff", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
        || Command::new("git")
            .args(["-C", repo_root.to_str().unwrap_or(".")])
            .args(["diff", "--cached", "--quiet"])
            .status()
            .map(|s| !s.success())
            .unwrap_or(false);

    let build_id = if dirty {
        format!("{git_sha}-dirty")
    } else {
        git_sha
    };

    println!("cargo:rustc-env=LOGICX_BUILD_ID={build_id}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
