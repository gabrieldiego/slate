use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BroadwebStatusKind {
    Idle,
    Fetching,
    SwitchingGateway,
    Complete,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BroadwebStatusSnapshot {
    pub kind: BroadwebStatusKind,
    pub message: String,
    pub target: Option<String>,
    pub gateway: Option<String>,
    pub sequence: u64,
}

impl BroadwebStatusSnapshot {
    pub fn idle() -> Self {
        Self {
            kind: BroadwebStatusKind::Idle,
            message: "Ready".to_string(),
            target: None,
            gateway: None,
            sequence: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BroadwebStatusReporter {
    state: Arc<Mutex<BroadwebStatusSnapshot>>,
}

impl BroadwebStatusReporter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BroadwebStatusSnapshot::idle())),
        }
    }

    pub fn snapshot(&self) -> BroadwebStatusSnapshot {
        self.state
            .lock()
            .expect("broadweb status should not be poisoned")
            .clone()
    }

    pub fn set(
        &self,
        kind: BroadwebStatusKind,
        message: impl Into<String>,
        target: Option<String>,
        gateway: Option<String>,
    ) {
        let mut state = self
            .state
            .lock()
            .expect("broadweb status should not be poisoned");
        let sequence = state.sequence.saturating_add(1);
        *state = BroadwebStatusSnapshot {
            kind,
            message: message.into(),
            target,
            gateway,
            sequence,
        };
    }

    pub fn set_idle(&self) {
        self.set(BroadwebStatusKind::Idle, "Ready", None, None);
    }
}

impl Default for BroadwebStatusReporter {
    fn default() -> Self {
        Self::new()
    }
}
