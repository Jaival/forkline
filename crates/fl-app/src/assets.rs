//! Icons: Tabler (`assets/icons`, MIT) in place of gpui-kit's Lucide set,
//! saved under the Lucide file names so every `IconName` keeps working. The
//! kit's Lucide icons stay as the fallback for paths Tabler does not cover.
//! To add an icon: add it to `assets/icons/fetch.sh` and to `tabler!` below.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

macro_rules! tabler {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")))),*]
    };
}

/// (asset path, SVG bytes).
static TABLER: &[(&str, &[u8])] = tabler!(
    "arrow-down", "arrow-left", "arrow-right", "arrow-up", "bell", "book-marked", "book-open", "calendar",
    "check", "chevron-down", "chevron-left", "chevron-right", "chevron-up", "chevrons-up-down", "circle-alert",
    "circle-check", "circle-dot", "circle-user", "circle-x", "clock", "close", "columns-2", "compass", "copy",
    "dash", "ellipsis-vertical", "ellipsis", "external-link", "eye-off", "eye", "file-text", "file",
    "folder-closed", "folder-open", "folder", "git-commit-horizontal", "git-fork", "git-merge",
    "git-pull-request-closed", "git-pull-request-draft", "git-pull-request", "globe", "house", "inbox", "info",
    "loader-circle", "loader", "menu", "minus", "moon", "palette", "panel-left-close", "panel-left-open",
    "panel-left", "pin", "plus", "refresh-cw", "search", "settings-2", "settings", "star-fill", "star-off",
    "star", "sun", "triangle-alert", "user", "x",
);

// Lucide icons outside the kit's default bundle. Tabler serves each of them
// first (see the test); they stay so a missing Tabler file still draws.
gpui_kit::assets::icon_assets!(
    pub ExtraIcons,
    [GitPullRequest, CircleDot, Compass, House, BookMarked, Columns2, GitFork, GitCommitHorizontal, Pin, GitMerge, GitPullRequestClosed, Clock, X, PanelLeftClose, PanelLeftOpen]
);

/// The logo as two one-colour layers (`titlebar.rs`), from
/// `docs/design/logo/a-branch-f.svg`.
static LOGO: &[(&str, &[u8])] = &[
    ("logo/stem.svg", include_bytes!("../assets/logo/stem.svg")),
    ("logo/branch.svg", include_bytes!("../assets/logo/branch.svg")),
];

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = TABLER.iter().chain(LOGO).find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.extend(TABLER.iter().map(|(p, _)| *p).filter(|p| p.starts_with(path)).map(SharedString::from));
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::AssetSource as _;

    use super::{AppAssets, ExtraIcons, TABLER};

    #[test]
    fn every_extra_icon_is_drawn_from_tabler() {
        for path in ExtraIcons.list("icons/").unwrap() {
            assert!(TABLER.iter().any(|(p, _)| *p == path.as_ref()), "{path} has no Tabler file");
        }
    }

    #[test]
    fn tabler_wins_over_the_kit_icon() {
        let bytes = AppAssets.load("icons/inbox.svg").unwrap().unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("tabler"));
    }
}
