//! Centralized process exit statuses.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitStatus {
    Success = 0,
    ExecutionFailure = 1,
    ValidationFailure = 2,
    Cancellation = 3,
    RollbackFailure = 4,
}

impl ExitStatus {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::ExecutionFailure => "execution failure",
            Self::ValidationFailure => "validation failure",
            Self::Cancellation => "cancellation",
            Self::RollbackFailure => "rollback failure",
        }
    }
}

impl From<ExitStatus> for std::process::ExitCode {
    fn from(value: ExitStatus) -> Self {
        std::process::ExitCode::from(value.code())
    }
}
