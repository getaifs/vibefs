// Build script to capture git hash at compile time

use std::path::Path;
use std::process::Command;

fn main() {
    // Get git hash (works even without .git if `git` can find the repo)
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    // Only set up file watches when .git exists (i.e., building from a real
    // checkout, not a VibeFS mount or tarball). When .git is absent, Cargo's
    // default behavior (re-run if any build input changes) is fine.
    if Path::new(".git/HEAD").exists() {
        println!("cargo:rerun-if-changed=.git/HEAD");

        // .git/HEAD contains "ref: refs/heads/<branch>" — only changes on
        // branch switch. The actual commit hash lives in the ref file.
        if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
            let head = head.trim();
            if let Some(refpath) = head.strip_prefix("ref: ") {
                let ref_file = format!(".git/{}", refpath);
                if Path::new(&ref_file).exists() {
                    println!("cargo:rerun-if-changed={}", ref_file);
                }
            }
        }

        // git gc packs loose refs into this file
        if Path::new(".git/packed-refs").exists() {
            println!("cargo:rerun-if-changed=.git/packed-refs");
        }
    }
}
