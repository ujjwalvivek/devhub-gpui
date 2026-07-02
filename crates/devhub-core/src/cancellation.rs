use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const OPERATION_CANCELLED: &str = "operation cancelled";

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(OPERATION_CANCELLED.into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_shared_by_clones() {
        let token = CancellationToken::new();
        let clone = token.clone();

        assert!(token.check().is_ok());
        clone.cancel();
        assert!(token.is_cancelled());
        assert_eq!(token.check().unwrap_err(), OPERATION_CANCELLED);
    }
}
