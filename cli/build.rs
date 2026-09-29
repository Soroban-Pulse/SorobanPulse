// Embeds the git commit into `spulse --version` (Issue #1135).
//
// `SPULSE_GIT_SHA` wins when set (release builds from a source tarball, CI);
// otherwise the short HEAD hash is read from git, falling back to "unknown"
// when git or the repository is unavailable.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn main() {
    println!("cargo:rerun-if-env-changed=SPULSE_GIT_SHA");

    let sha = std::env::var("SPULSE_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| git(&["rev-parse", "--short=10", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=SPULSE_GIT_SHA={sha}");

    // Rebuild when HEAD moves (checkout) or the current branch gets a commit.
    for path in ["HEAD", "packed-refs"] {
        if let Some(p) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(p) = git(&["rev-parse", "--git-path", &branch]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
}
