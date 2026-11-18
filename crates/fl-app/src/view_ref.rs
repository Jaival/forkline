//! What a tab shows. Saved inside the dock layout, so keep it stable:
//! add variants, do not rename them.

use gpui_kit::SharedString;
use gpui_kit::assets::IconName;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewRef {
    Home,
    Inbox,
    Pulls {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repo_id: Option<i64>,
    },
    Issues {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repo_id: Option<i64>,
    },
    Reviews,
    Starred,
    Repos,
    Explore,
    Repo { repo_id: i64 },
    Issue { id: i64 },
    Pr { id: i64 },
    Search { q: String },
    Settings,
}

impl ViewRef {
    /// Tab title until the view knows better (e.g. "owner/name", "#123").
    pub fn title(&self) -> SharedString {
        match self {
            Self::Home => "Home".into(),
            Self::Inbox => "Inbox".into(),
            Self::Pulls { .. } => "Pull requests".into(),
            Self::Issues { .. } => "Issues".into(),
            Self::Reviews => "Review requests".into(),
            Self::Starred => "Starred".into(),
            Self::Repos => "Repositories".into(),
            Self::Explore => "Explore".into(),
            Self::Repo { repo_id } => format!("Repository {repo_id}").into(),
            Self::Issue { id } => format!("Issue {id}").into(),
            Self::Pr { id } => format!("Pull request {id}").into(),
            Self::Settings => "Settings".into(),
            Self::Search { q } if q.is_empty() => "Search".into(),
            Self::Search { q } => format!("Search: {q}").into(),
        }
    }

    /// The repo a repo-scoped tab shows, whose name goes in the title.
    pub fn repo_id(&self) -> Option<i64> {
        match self {
            Self::Repo { repo_id } => Some(*repo_id),
            Self::Pulls { repo_id } | Self::Issues { repo_id } => *repo_id,
            _ => None,
        }
    }

    /// The title with the repo's `owner/name`, once known. An unknown repo
    /// keeps the plain title ("Repository 4").
    pub fn title_with(&self, repo: Option<&str>) -> SharedString {
        match (self, repo) {
            (Self::Repo { .. }, Some(name)) => SharedString::from(name.to_owned()),
            (Self::Pulls { repo_id: Some(_) }, Some(name)) => format!("{name}: Pull requests").into(),
            (Self::Issues { repo_id: Some(_) }, Some(name)) => format!("{name}: Issues").into(),
            _ => self.title(),
        }
    }

    /// Same variant, whatever its fields: a filtered Pulls tab and the
    /// unfiltered one, or two Search tabs.
    pub fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    pub fn icon(&self) -> IconName {
        match self {
            Self::Home => IconName::House,
            Self::Inbox => IconName::Inbox,
            Self::Pulls { .. } | Self::Pr { .. } => IconName::GitPullRequest,
            Self::Issues { .. } | Self::Issue { .. } => IconName::CircleDot,
            Self::Reviews => IconName::Eye,
            Self::Starred => IconName::Star,
            Self::Repos | Self::Repo { .. } => IconName::BookMarked,
            Self::Explore => IconName::Compass,
            Self::Search { .. } => IconName::Search,
            Self::Settings => IconName::Settings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        for v in [
            ViewRef::Home,
            ViewRef::Pulls { repo_id: None },
            ViewRef::Issues { repo_id: Some(4) },
            ViewRef::Pr { id: 9 },
            ViewRef::Search { q: "is:open".into() },
            ViewRef::Settings,
        ] {
            let json = serde_json::to_value(&v).unwrap();
            assert_eq!(serde_json::from_value::<ViewRef>(json).unwrap(), v);
        }
        assert_eq!(serde_json::to_string(&ViewRef::Pulls { repo_id: None }).unwrap(), r#"{"kind":"pulls"}"#);
    }

    #[test]
    fn titles_name_the_repo() {
        let name = Some("acme/rocket");
        assert_eq!(ViewRef::Repo { repo_id: 1 }.title_with(name), "acme/rocket");
        assert_eq!(ViewRef::Repo { repo_id: 1 }.title_with(None), "Repository 1");
        assert_eq!(ViewRef::Pulls { repo_id: Some(1) }.title_with(name), "acme/rocket: Pull requests");
        assert_eq!(ViewRef::Issues { repo_id: Some(1) }.title_with(name), "acme/rocket: Issues");
        assert_eq!(ViewRef::Inbox.title_with(name), "Inbox");
        assert!(ViewRef::Pulls { repo_id: None }.same_kind(&ViewRef::Pulls { repo_id: Some(2) }));
        assert!(!ViewRef::Pulls { repo_id: None }.same_kind(&ViewRef::Issues { repo_id: None }));
    }
}
