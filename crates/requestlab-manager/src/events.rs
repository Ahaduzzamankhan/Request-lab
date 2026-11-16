//! Progress vocabulary shared by the installer, the updater and the CLI.
//!
//! Front ends translate these events into a progress bar, a log line or JSON;
//! the engine never formats output itself.

use std::path::PathBuf;

use crate::version::Version;

/// Where a management operation currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Planning,
    CheckingRelease,
    WaitingForExit,
    Downloading,
    VerifyingSignature,
    VerifyingPackage,
    Extracting,
    Installing,
    CreatingShortcuts,
    RegisteringUninstall,
    RollingBack,
    Finalizing,
    Launching,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Stage::Planning => "Preparing",
            Stage::CheckingRelease => "Checking for releases",
            Stage::WaitingForExit => "Waiting for RequestLab to close",
            Stage::Downloading => "Downloading",
            Stage::VerifyingSignature => "Verifying release signature",
            Stage::VerifyingPackage => "Verifying package",
            Stage::Extracting => "Extracting",
            Stage::Installing => "Installing",
            Stage::CreatingShortcuts => "Creating shortcuts",
            Stage::RegisteringUninstall => "Registering uninstaller",
            Stage::RollingBack => "Restoring the previous version",
            Stage::Finalizing => "Finishing",
            Stage::Launching => "Starting RequestLab",
        }
    }
}

/// Byte-level download progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transfer {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub bytes_per_second: f64,
}

impl Transfer {
    pub fn fraction(&self) -> Option<f32> {
        self.total
            .filter(|total| *total > 0)
            .map(|total| (self.downloaded as f32 / total as f32).clamp(0.0, 1.0))
    }
}

/// Everything an operation reports.
#[derive(Debug, Clone, PartialEq)]
pub enum ManagerEvent {
    Stage {
        stage: Stage,
        detail: String,
    },
    Transfer(Transfer),
    /// Extraction progress, 0.0..=1.0 when the entry count is known.
    Extracting {
        fraction: Option<f32>,
        bytes: u64,
    },
    Warning(String),
    Installed {
        version: Version,
        directory: PathBuf,
    },
}

impl ManagerEvent {
    pub fn stage(stage: Stage, detail: impl Into<String>) -> Self {
        ManagerEvent::Stage {
            stage,
            detail: detail.into(),
        }
    }
}

/// Callback a front end supplies to watch an operation.
pub type EventSink<'a> = &'a mut dyn FnMut(ManagerEvent);

/// A sink that discards events, for callers that only need the result.
pub fn ignore(_: ManagerEvent) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_fraction_is_bounded() {
        let transfer = Transfer {
            downloaded: 5,
            total: Some(10),
            bytes_per_second: 1.0,
        };
        assert_eq!(transfer.fraction(), Some(0.5));
        let over = Transfer {
            downloaded: 20,
            total: Some(10),
            bytes_per_second: 1.0,
        };
        assert_eq!(over.fraction(), Some(1.0));
        assert_eq!(
            Transfer {
                downloaded: 1,
                total: None,
                bytes_per_second: 1.0
            }
            .fraction(),
            None
        );
    }

    #[test]
    fn every_stage_has_a_label() {
        for stage in [
            Stage::Planning,
            Stage::Downloading,
            Stage::Installing,
            Stage::Launching,
        ] {
            assert!(!stage.label().is_empty());
        }
    }
}
