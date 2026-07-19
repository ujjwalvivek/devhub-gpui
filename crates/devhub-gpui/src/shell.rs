#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Overview,
    Files,
    Search,
}

impl Activity {
    pub const ALL: [Self; 3] = [Self::Overview, Self::Files, Self::Search];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Files => "Files",
            Self::Search => "Search",
        }
    }
}

pub fn visible_project_row(filtered: &[usize], selected_project: Option<usize>) -> Option<usize> {
    selected_project.and_then(|selected| {
        filtered
            .iter()
            .position(|project_index| *project_index == selected)
    })
}

#[cfg(test)]
mod tests {
    use super::{visible_project_row, Activity};

    #[test]
    fn activities_are_project_workspace_modes() {
        assert_eq!(
            Activity::ALL.map(Activity::label),
            ["Overview", "Files", "Search"]
        );
    }

    #[test]
    fn project_selection_survives_filter_and_sort_changes() {
        let selected = Some(7);
        assert_eq!(visible_project_row(&[3, 7, 9], selected), Some(1));
        assert_eq!(visible_project_row(&[7, 3, 9], selected), Some(0));
        assert_eq!(visible_project_row(&[3, 9], selected), None);
    }
}
