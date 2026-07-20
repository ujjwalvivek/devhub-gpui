use devhub_core::{AppearanceMode, ThemeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandId {
    ToggleProjectCatalog,
    ShowOverview,
    ShowFiles,
    ShowSearch,
    ShowGit,
    ShowHistory,
    RefreshGit,
    StageSelectedChange,
    StageAllChanges,
    UnstageSelectedChange,
    UnstageAllChanges,
    DiscardSelectedChange,
    FocusGitCommit,
    FetchGitRemotes,
    PushGitBranch,
    OpenInZed,
    OpenInEditor,
    ToggleProjectPin,
    HideProject,
    CopyProjectPath,
    RefreshProjects,
    ToggleContextPane,
    ToggleReadmePreview,
    ToggleFileWrap,
    SelectTheme,
    ShowSettings,
    ToggleTerminal,
    ToggleAskProject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: CommandId,
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
}

pub const COMMANDS: [CommandSpec; 28] = [
    CommandSpec {
        id: CommandId::ToggleProjectCatalog,
        title: "Toggle Project Catalog",
        category: "Navigation",
        shortcut: Some("Ctrl+1"),
    },
    CommandSpec {
        id: CommandId::ShowOverview,
        title: "Go to Overview",
        category: "Navigation",
        shortcut: Some("Ctrl+2"),
    },
    CommandSpec {
        id: CommandId::ShowFiles,
        title: "Go to Files",
        category: "Navigation",
        shortcut: Some("Ctrl+3"),
    },
    CommandSpec {
        id: CommandId::ShowSearch,
        title: "Search Project Contents",
        category: "Navigation",
        shortcut: Some("Ctrl+4"),
    },
    CommandSpec {
        id: CommandId::ShowGit,
        title: "Go to Git Changes",
        category: "Navigation",
        shortcut: Some("Ctrl+5"),
    },
    CommandSpec {
        id: CommandId::ShowHistory,
        title: "Go to Commit History",
        category: "Navigation",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::RefreshGit,
        title: "Refresh Git Status",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::StageSelectedChange,
        title: "Stage Selected Change",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::StageAllChanges,
        title: "Stage All Changes",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::UnstageSelectedChange,
        title: "Unstage Selected Change",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::UnstageAllChanges,
        title: "Unstage All Changes",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::DiscardSelectedChange,
        title: "Discard Selected Change",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::FocusGitCommit,
        title: "Write Commit Message",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::FetchGitRemotes,
        title: "Fetch Git Remotes",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::PushGitBranch,
        title: "Push Current Git Branch",
        category: "Git",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::OpenInZed,
        title: "Open Project in Zed",
        category: "Project",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::OpenInEditor,
        title: "Open Project In...",
        category: "Project",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::ToggleProjectPin,
        title: "Toggle Project Pin",
        category: "Project",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::HideProject,
        title: "Hide Project",
        category: "Project",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::CopyProjectPath,
        title: "Copy Project Path",
        category: "Project",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::RefreshProjects,
        title: "Refresh Projects",
        category: "Project",
        shortcut: Some("Ctrl+R"),
    },
    CommandSpec {
        id: CommandId::ToggleContextPane,
        title: "Toggle Context Pane",
        category: "View",
        shortcut: Some("Ctrl+B"),
    },
    CommandSpec {
        id: CommandId::ToggleReadmePreview,
        title: "Toggle README Preview",
        category: "View",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::ToggleFileWrap,
        title: "Toggle File Wrap",
        category: "View",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::SelectTheme,
        title: "Select Theme",
        category: "Appearance",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::ShowSettings,
        title: "Open Settings",
        category: "Application",
        shortcut: None,
    },
    CommandSpec {
        id: CommandId::ToggleTerminal,
        title: "Toggle Terminal",
        category: "View",
        shortcut: Some("Ctrl+`"),
    },
    CommandSpec {
        id: CommandId::ToggleAskProject,
        title: "Toggle Ask Project",
        category: "View",
        shortcut: Some("Ctrl+Shift+A"),
    },
];

pub fn filtered_commands(query: &str) -> Vec<CommandSpec> {
    let terms = query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return COMMANDS.to_vec();
    }

    let mut matches = COMMANDS
        .into_iter()
        .enumerate()
        .filter_map(|(registry_index, command)| {
            let title = command.title.to_ascii_lowercase();
            let category = command.category.to_ascii_lowercase();
            let score = terms.iter().try_fold(0usize, |score, term| {
                let title_score = subsequence_score(&title, term);
                let category_score = subsequence_score(&category, term).map(|score| score + 100);
                title_score
                    .into_iter()
                    .chain(category_score)
                    .min()
                    .map(|term_score| score + term_score)
            })?;
            Some((score, registry_index, command))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|(score, registry_index, _)| (*score, *registry_index));
    matches.into_iter().map(|(_, _, command)| command).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeSelection {
    FollowSystem,
    Fixed {
        theme: ThemeId,
        appearance: AppearanceMode,
    },
}

impl ThemeSelection {
    pub const ALL: [Self; 11] = [
        Self::FollowSystem,
        Self::fixed(ThemeId::CatppuccinMocha, AppearanceMode::Dark),
        Self::fixed(ThemeId::RosePineMoon, AppearanceMode::Dark),
        Self::fixed(ThemeId::TokyoNightStorm, AppearanceMode::Dark),
        Self::fixed(ThemeId::HorizonBold, AppearanceMode::Dark),
        Self::fixed(ThemeId::MonochromeZero, AppearanceMode::Dark),
        Self::fixed(ThemeId::CatppuccinMocha, AppearanceMode::Light),
        Self::fixed(ThemeId::RosePineMoon, AppearanceMode::Light),
        Self::fixed(ThemeId::TokyoNightStorm, AppearanceMode::Light),
        Self::fixed(ThemeId::HorizonBold, AppearanceMode::Light),
        Self::fixed(ThemeId::MonochromeZero, AppearanceMode::Light),
    ];

    const fn fixed(theme: ThemeId, appearance: AppearanceMode) -> Self {
        Self::Fixed { theme, appearance }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FollowSystem => "Follow System Appearance",
            Self::Fixed {
                theme: ThemeId::CatppuccinMocha,
                appearance: AppearanceMode::Dark,
            } => "Catppuccin Dark",
            Self::Fixed {
                theme: ThemeId::RosePineMoon,
                appearance: AppearanceMode::Dark,
            } => "Rose Pine Dark",
            Self::Fixed {
                theme: ThemeId::TokyoNightStorm,
                appearance: AppearanceMode::Dark,
            } => "Tokyo Night Dark",
            Self::Fixed {
                theme: ThemeId::HorizonBold,
                appearance: AppearanceMode::Dark,
            } => "Horizon Dark",
            Self::Fixed {
                theme: ThemeId::MonochromeZero,
                appearance: AppearanceMode::Dark,
            } => "Monochrome Dark",
            Self::Fixed {
                theme: ThemeId::CatppuccinMocha,
                appearance: AppearanceMode::Light,
            } => "Catppuccin Light",
            Self::Fixed {
                theme: ThemeId::RosePineMoon,
                appearance: AppearanceMode::Light,
            } => "Rose Pine Light",
            Self::Fixed {
                theme: ThemeId::TokyoNightStorm,
                appearance: AppearanceMode::Light,
            } => "Tokyo Night Light",
            Self::Fixed {
                theme: ThemeId::HorizonBold,
                appearance: AppearanceMode::Light,
            } => "Horizon Light",
            Self::Fixed {
                theme: ThemeId::MonochromeZero,
                appearance: AppearanceMode::Light,
            } => "Monochrome Light",
            Self::Fixed {
                appearance: AppearanceMode::System,
                ..
            } => unreachable!("system appearance is represented by FollowSystem"),
        }
    }

    pub fn preferences(self, current_theme: ThemeId) -> (ThemeId, AppearanceMode) {
        match self {
            Self::FollowSystem => (current_theme, AppearanceMode::System),
            Self::Fixed { theme, appearance } => (theme, appearance),
        }
    }

    pub fn is_active(self, theme: ThemeId, appearance: AppearanceMode) -> bool {
        match self {
            Self::FollowSystem => appearance == AppearanceMode::System,
            Self::Fixed {
                theme: candidate_theme,
                appearance: candidate_appearance,
            } => candidate_theme == theme && candidate_appearance == appearance,
        }
    }
}

pub fn filtered_themes(query: &str) -> Vec<ThemeSelection> {
    let query = query.trim().to_ascii_lowercase();
    let mut themes = ThemeSelection::ALL
        .into_iter()
        .filter_map(|theme| {
            if query.is_empty() {
                return Some((0, theme));
            }
            subsequence_score(&theme.label().to_ascii_lowercase(), &query)
                .map(|score| (score, theme))
        })
        .collect::<Vec<_>>();
    themes.sort_by_key(|(score, _)| *score);
    themes.into_iter().map(|(_, theme)| theme).collect()
}

fn subsequence_score(haystack: &str, needle: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut previous = None;
    let mut score = 0;

    for character in needle.chars() {
        let offset = haystack[cursor..].find(character)?;
        let position = cursor + offset;
        score += previous.map_or(position, |previous| position.saturating_sub(previous + 1));
        previous = Some(position);
        cursor = position + character.len_utf8();
    }

    Some(score)
}

#[cfg(test)]
mod tests {
    use super::{filtered_commands, filtered_themes, CommandId, ThemeSelection, COMMANDS};

    #[test]
    fn empty_query_keeps_registry_order() {
        assert_eq!(filtered_commands(""), COMMANDS);
    }

    #[test]
    fn command_search_matches_title_and_category_terms() {
        assert_eq!(
            filtered_commands("project zed")
                .into_iter()
                .map(|command| command.id)
                .collect::<Vec<_>>(),
            [CommandId::OpenInZed]
        );
        assert_eq!(
            filtered_commands("navigation files")
                .into_iter()
                .map(|command| command.id)
                .collect::<Vec<_>>(),
            [CommandId::ShowFiles]
        );
    }

    #[test]
    fn command_search_accepts_memorable_subsequences() {
        assert_eq!(
            filtered_commands("project catalog")[0].id,
            CommandId::ToggleProjectCatalog
        );
        assert_eq!(filtered_commands("prj zed")[0].id, CommandId::OpenInZed);
        assert_eq!(
            filtered_commands("git stage")[0].id,
            CommandId::StageSelectedChange
        );
    }

    #[test]
    fn theme_search_keeps_declared_order_and_supports_subsequences() {
        assert_eq!(filtered_themes(""), ThemeSelection::ALL);
        assert_eq!(filtered_themes("").len(), 11);
        assert_eq!(
            filtered_themes("").last().map(|theme| theme.label()),
            Some("Monochrome Light")
        );
        assert_eq!(filtered_themes("rose").len(), 2);
        assert_eq!(filtered_themes("tkn").len(), 2);
        assert_eq!(filtered_themes("system"), [ThemeSelection::FollowSystem]);
    }
}
