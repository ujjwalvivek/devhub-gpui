use devhub_core::Project;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanState {
    Idle,
    Scanning,
    Loaded { count: usize },
    Empty,
    Error(String),
}

pub struct ScanModel {
    pub projects: Vec<Project>,
    pub state: ScanState,
    generation: u64,
}

pub fn previous_selection(current: Option<usize>, item_count: usize) -> Option<usize> {
    if item_count == 0 {
        return None;
    }

    Some(match current {
        Some(index) => index.min(item_count - 1).saturating_sub(1),
        None => item_count - 1,
    })
}

pub fn next_selection(current: Option<usize>, item_count: usize) -> Option<usize> {
    if item_count == 0 {
        return None;
    }

    Some(match current {
        Some(index) => (index + 1).min(item_count - 1),
        None => 0,
    })
}

impl ScanModel {
    pub fn new(projects: Vec<Project>) -> Self {
        Self {
            projects,
            state: ScanState::Idle,
            generation: 0,
        }
    }

    pub fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.state = ScanState::Scanning;
        self.generation
    }

    pub fn apply_result(&mut self, generation: u64, result: Result<Vec<Project>, String>) -> bool {
        if generation != self.generation {
            return false;
        }

        match result {
            Ok(projects) if projects.is_empty() => {
                self.projects.clear();
                self.state = ScanState::Empty;
            }
            Ok(projects) => {
                let count = projects.len();
                self.projects = projects;
                self.state = ScanState::Loaded { count };
            }
            Err(error) => {
                self.state = ScanState::Error(error);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devhub_core::{ProjectSource, ProjectType};
    use std::path::PathBuf;

    fn project() -> Project {
        Project {
            name: "fixture".to_string(),
            path: PathBuf::from(r"F:\fixture"),
            source: ProjectSource::Local,
            project_type: ProjectType::Rust,
            has_git: false,
            git_remote: None,
            markers_found: vec!["Cargo.toml".to_string()],
            last_modified: None,
            search_key: String::new(),
        }
    }

    #[test]
    fn stale_result_cannot_replace_newer_scan() {
        let mut scan = ScanModel::new(vec![project()]);
        let stale_generation = scan.begin();
        let current_generation = scan.begin();

        assert!(!scan.apply_result(stale_generation, Ok(vec![project()])));
        assert_eq!(scan.state, ScanState::Scanning);
        assert_eq!(scan.projects.len(), 1);

        assert!(scan.apply_result(current_generation, Ok(vec![project()])));
        assert_eq!(scan.state, ScanState::Loaded { count: 1 });
    }

    #[test]
    fn current_result_models_loaded_empty_and_error_states() {
        let mut scan = ScanModel::new(Vec::new());
        let generation = scan.begin();

        assert!(scan.apply_result(generation, Ok(vec![project()])));
        assert_eq!(scan.state, ScanState::Loaded { count: 1 });
        assert_eq!(scan.projects.len(), 1);

        assert!(scan.apply_result(generation, Ok(Vec::new())));
        assert_eq!(scan.state, ScanState::Empty);
        assert!(scan.projects.is_empty());

        let mut scan = ScanModel::new(vec![project()]);
        let generation = scan.begin();
        assert!(scan.apply_result(generation, Err("fixture failure".to_string())));
        assert_eq!(scan.state, ScanState::Error("fixture failure".to_string()));
        assert_eq!(scan.projects.len(), 1);
    }

    #[test]
    fn keyboard_selection_stops_at_list_boundaries() {
        assert_eq!(previous_selection(None, 3), Some(2));
        assert_eq!(previous_selection(Some(0), 3), Some(0));
        assert_eq!(previous_selection(Some(2), 3), Some(1));

        assert_eq!(next_selection(None, 3), Some(0));
        assert_eq!(next_selection(Some(1), 3), Some(2));
        assert_eq!(next_selection(Some(2), 3), Some(2));
    }

    #[test]
    fn keyboard_selection_handles_empty_and_stale_indices() {
        assert_eq!(previous_selection(Some(8), 0), None);
        assert_eq!(next_selection(Some(8), 0), None);
        assert_eq!(previous_selection(Some(8), 3), Some(1));
        assert_eq!(next_selection(Some(8), 3), Some(2));
    }
}
