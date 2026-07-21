use std::collections::VecDeque;

use devhub_core::{
    PersistenceEvent, PersistenceFailure, PersistenceOperation, PersistenceRecoverySource,
    PersistenceStore,
};

const PERSISTENCE_HISTORY_CAPACITY: usize = 8;

#[derive(Debug, Default)]
pub struct PersistenceHistory {
    events: VecDeque<PersistenceEvent>,
}

impl PersistenceHistory {
    pub fn record_events(&mut self, events: impl IntoIterator<Item = PersistenceEvent>) {
        for event in events {
            if self.events.len() == PERSISTENCE_HISTORY_CAPACITY {
                self.events.pop_front();
            }
            self.events.push_back(event);
        }
    }

    pub fn record_failure(&mut self, failure: &PersistenceFailure) {
        if let Some(event) = failure.event() {
            self.record_events([event.clone()]);
        }
    }

    pub fn latest(&self) -> Option<&PersistenceEvent> {
        self.events.back()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

pub fn persistence_status_text(event: &PersistenceEvent) -> String {
    match event {
        PersistenceEvent::Recovered { store, source } => format!(
            "{} recovered from {}.",
            store_label(*store),
            recovery_source_label(*source)
        ),
        PersistenceEvent::Conflict { store, operation } => match operation {
            PersistenceOperation::Recovery => format!(
                "{} recovery stopped because another instance changed it.",
                store_label(*store)
            ),
            PersistenceOperation::Write => format!(
                "{} save blocked while another instance is writing.",
                store_label(*store)
            ),
        },
    }
}

fn store_label(store: PersistenceStore) -> &'static str {
    match store {
        PersistenceStore::Config => "Config",
        PersistenceStore::ProjectCache => "Project cache",
        PersistenceStore::Todos => "Todos",
    }
}

fn recovery_source_label(source: PersistenceRecoverySource) -> &'static str {
    match source {
        PersistenceRecoverySource::Backup => "backup",
        PersistenceRecoverySource::Temporary => "an interrupted write",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded_and_retains_the_newest_event() {
        let mut history = PersistenceHistory::default();
        for index in 0..12 {
            let store = if index % 2 == 0 {
                PersistenceStore::Config
            } else {
                PersistenceStore::ProjectCache
            };
            history.record_events([PersistenceEvent::Conflict {
                store,
                operation: PersistenceOperation::Write,
            }]);
        }

        assert_eq!(history.len(), PERSISTENCE_HISTORY_CAPACITY);
        assert_eq!(
            history.latest(),
            Some(&PersistenceEvent::Conflict {
                store: PersistenceStore::ProjectCache,
                operation: PersistenceOperation::Write,
            })
        );
    }

    #[test]
    fn status_text_distinguishes_recovery_source_and_conflicts() {
        assert_eq!(
            persistence_status_text(&PersistenceEvent::Recovered {
                store: PersistenceStore::Config,
                source: PersistenceRecoverySource::Backup,
            }),
            "Config recovered from backup."
        );
        assert_eq!(
            persistence_status_text(&PersistenceEvent::Recovered {
                store: PersistenceStore::ProjectCache,
                source: PersistenceRecoverySource::Temporary,
            }),
            "Project cache recovered from an interrupted write."
        );
        assert_eq!(
            persistence_status_text(&PersistenceEvent::Conflict {
                store: PersistenceStore::Config,
                operation: PersistenceOperation::Write,
            }),
            "Config save blocked while another instance is writing."
        );
    }
}
