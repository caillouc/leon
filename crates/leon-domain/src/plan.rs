//! An agent session's plan (CONV-14): the steps the agent set itself and
//! where each stands, as the CLI reports them.

/// Where a step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    /// Not started.
    Pending,
    /// Being worked on.
    InProgress,
    /// Finished.
    Done,
}

/// One step of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    /// What the step is.
    pub text: String,
    /// Where it stands.
    pub status: StepStatus,
}

impl PlanStep {
    /// A step with `text` in `status`.
    pub fn new(text: impl Into<String>, status: StepStatus) -> Self {
        Self {
            text: text.into(),
            status,
        }
    }
}
