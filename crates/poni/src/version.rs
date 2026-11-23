//! Which build this is, for the menu's About line and the tray tooltip:
//! the version, plus where the build came from (build.rs fills these in).

const REPO: &str = "https://github.com/Dapirz/oyen-aja";
/// The name people see. Hihu Island is a personal fork of Oyenza Studio.
pub const APP_NAME: &str = "Hihu Island";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// "release" (a release tag's CI build), "test" (any other CI build) or "local".
const KIND: &str = env!("PONI_BUILD_KIND");
/// The commit a CI build was made from, 7 characters; empty for a local build.
const COMMIT: &str = env!("PONI_BUILD_COMMIT");

/// The page the About line opens: this release's notes, a test build's commit.
pub fn notes_url() -> String {
    url(VERSION, KIND, COMMIT)
}

fn url(version: &str, kind: &str, commit: &str) -> String {
    match kind {
        "release" => format!("{REPO}/releases/tag/v{version}"),
        "test" if !commit.is_empty() => format!("{REPO}/commit/{commit}"),
        _ => format!("{REPO}/releases"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_opens_its_notes_and_a_test_build_its_commit() {
        assert_eq!(url("0.2.3", "release", "c7d77c8"), "https://github.com/Dapirz/oyen-aja/releases/tag/v0.2.3");
        assert_eq!(url("0.2.3", "test", "c7d77c8"), "https://github.com/Dapirz/oyen-aja/commit/c7d77c8");
        assert_eq!(url("0.2.3", "local", ""), "https://github.com/Dapirz/oyen-aja/releases");
    }
}
