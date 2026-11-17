//! Shared state model for the RequestLab installer and updater windows.
//!
//! The engine in `requestlab-manager` reports [`ManagerEvent`]s and never
//! formats output. This crate turns those events into the state a window draws:
//! one stage line, one progress bar, a bounded log and a terminal result. It has
//! no GPUI dependency, so the logic is unit tested on any host; the installer and
//! updater binaries only render it.

use std::path::Path;

use requestlab_manager::downloader::{human_bytes, human_rate};
use requestlab_manager::events::{ManagerEvent, Stage};
use requestlab_manager::updater::AvailableUpdate;
use requestlab_manager::version::Version;

/// The log is trimmed from the front once it grows past this many lines.
pub const MAX_LOG_LINES: usize = 400;

/// Which management operation a window is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Install,
    Update,
}

impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Operation::Install => "Install RequestLab",
            Operation::Update => "Update RequestLab",
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            Operation::Install => "Install",
            Operation::Update => "Update",
        }
    }

    pub fn working_title(self) -> &'static str {
        match self {
            Operation::Install => "Installing RequestLab",
            Operation::Update => "Updating RequestLab",
        }
    }
}

/// Which part of the flow is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Waiting for the user to start.
    Ready,
    /// Running; progress is live.
    Working,
    /// Finished successfully.
    Finished,
    /// Stopped with an error; nothing was left half-installed.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
}

/// What an available or installed release looks like to the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: Version,
    pub channel: &'static str,
    pub size: u64,
    pub notes_url: Option<String>,
}

impl ReleaseInfo {
    pub fn from_update(update: &AvailableUpdate) -> Self {
        Self {
            version: update.version.clone(),
            channel: update.channel.as_str(),
            size: update.size(),
            notes_url: update.notes_url.clone(),
        }
    }

    pub fn size_text(&self) -> String {
        human_bytes(self.size)
    }
}

/// Everything an installer or updater window needs to draw itself.
#[derive(Debug, Clone)]
pub struct FlowState {
    pub operation: Operation,
    pub screen: Screen,
    pub stage: Option<Stage>,
    pub detail: String,
    /// Download or extraction progress, when the engine reports a denominator.
    pub fraction: Option<f32>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub rate: f64,
    pub log: Vec<LogLine>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub installed_version: Option<Version>,
    pub install_dir: String,
}

impl FlowState {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            screen: Screen::Ready,
            stage: None,
            detail: String::new(),
            fraction: None,
            downloaded: 0,
            total: None,
            rate: 0.0,
            log: Vec::new(),
            warnings: Vec::new(),
            error: None,
            installed_version: None,
            install_dir: String::new(),
        }
    }

    /// Move to the working screen and clear any previous result.
    pub fn start(&mut self) {
        self.screen = Screen::Working;
        self.error = None;
        self.warnings.clear();
        self.fraction = None;
        self.stage = Some(Stage::Planning);
        self.detail = Stage::Planning.label().to_string();
        self.push_log(LogLevel::Info, self.detail.clone());
    }

    /// Apply one engine event.
    pub fn apply(&mut self, event: ManagerEvent) {
        match event {
            ManagerEvent::Stage { stage, detail } => {
                self.stage = Some(stage);
                self.detail = detail.clone();
                if stage != Stage::Downloading {
                    self.fraction = None;
                    self.rate = 0.0;
                }
                self.push_log(LogLevel::Info, format!("{}: {detail}", stage.label()));
            }
            ManagerEvent::Transfer(transfer) => {
                self.downloaded = transfer.downloaded;
                self.total = transfer.total;
                self.rate = transfer.bytes_per_second;
                self.fraction = transfer.fraction();
                self.stage = Some(Stage::Downloading);
                self.detail = self.transfer_text();
            }
            ManagerEvent::Extracting { fraction, bytes } => {
                self.stage = Some(Stage::Extracting);
                self.fraction = fraction;
                self.detail = match (fraction, bytes) {
                    (Some(fraction), _) => format!("Unpacking ({:.0}%)", fraction * 100.0),
                    (None, 0) => "Unpacking".to_string(),
                    (None, bytes) => format!("Unpacking {}", human_bytes(bytes)),
                };
            }
            ManagerEvent::Warning(message) => {
                self.warnings.push(message.clone());
                self.push_log(LogLevel::Warning, message);
            }
            ManagerEvent::Installed { version, directory } => {
                self.installed_version = Some(version.clone());
                self.install_dir = directory.display().to_string();
                self.push_log(
                    LogLevel::Info,
                    format!(
                        "RequestLab {version} is installed in {}",
                        directory.display()
                    ),
                );
            }
        }
    }

    /// Record a successful completion.
    pub fn succeed(&mut self, version: Version, install_dir: &Path) {
        self.screen = Screen::Finished;
        self.installed_version = Some(version.clone());
        self.install_dir = install_dir.display().to_string();
        self.fraction = Some(1.0);
        self.detail = format!("RequestLab {version} is ready");
        self.push_log(LogLevel::Info, self.detail.clone());
    }

    /// Record a failure. The engine rolls back on its own, so the window only has
    /// to report what went wrong.
    pub fn fail(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.screen = Screen::Failed;
        self.fraction = None;
        self.detail = "The operation did not finish".to_string();
        self.error = Some(error.clone());
        self.push_log(LogLevel::Error, error);
    }

    pub fn is_working(&self) -> bool {
        self.screen == Screen::Working
    }

    pub fn is_finished(&self) -> bool {
        self.screen == Screen::Finished
    }

    /// Stage label plus its detail line, ready to draw.
    pub fn status_line(&self) -> String {
        match (&self.stage, self.screen) {
            (_, Screen::Failed) => self
                .error
                .clone()
                .unwrap_or_else(|| "The operation did not finish".to_string()),
            (_, Screen::Finished) => self.detail.clone(),
            (Some(stage), _) if !self.detail.is_empty() => {
                format!("{}: {}", stage.label(), self.detail)
            }
            (Some(stage), _) => stage.label().to_string(),
            (None, _) => "Ready".to_string(),
        }
    }

    /// Human readable transfer or stage progress.
    pub fn progress_text(&self) -> String {
        if let Some(fraction) = self.fraction {
            if self.stage == Some(Stage::Downloading) && self.downloaded > 0 {
                return format!("{:.0}% · {}", fraction * 100.0, self.transfer_text());
            }
            return format!("{:.0}%", fraction * 100.0);
        }
        String::new()
    }

    /// 0.0..=1.0 for a progress bar; `None` while the total is unknown.
    pub fn percent(&self) -> Option<f32> {
        self.fraction.map(|fraction| fraction.clamp(0.0, 1.0))
    }

    /// The window title for the current screen.
    pub fn title(&self) -> &'static str {
        match self.screen {
            Screen::Ready => self.operation.title(),
            Screen::Working => self.operation.working_title(),
            Screen::Finished => match self.operation {
                Operation::Install => "RequestLab is installed",
                Operation::Update => "RequestLab is up to date",
            },
            Screen::Failed => "RequestLab setup stopped",
        }
    }

    /// The primary button label for the current screen.
    pub fn primary_label(&self) -> &'static str {
        match self.screen {
            Screen::Ready => self.operation.verb(),
            Screen::Working => "Working…",
            Screen::Finished => "Close",
            Screen::Failed => "Try again",
        }
    }

    /// Whether the primary button should be clickable.
    pub fn primary_enabled(&self) -> bool {
        !self.is_working()
    }

    pub fn transfer_text(&self) -> String {
        let downloaded = human_bytes(self.downloaded);
        let total = self
            .total
            .map(human_bytes)
            .unwrap_or_else(|| "unknown size".to_string());
        if self.rate > 0.0 {
            format!("{downloaded} / {total} · {}", human_rate(self.rate))
        } else {
            format!("{downloaded} / {total}")
        }
    }

    fn push_log(&mut self, level: LogLevel, text: impl Into<String>) {
        self.log.push(LogLine {
            level,
            text: text.into(),
        });
        if self.log.len() > MAX_LOG_LINES {
            let excess = self.log.len() - MAX_LOG_LINES;
            self.log.drain(0..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_manager::events::Transfer;
    use std::path::PathBuf;

    fn install_flow() -> FlowState {
        FlowState::new(Operation::Install)
    }

    #[test]
    fn a_new_flow_is_ready_and_starts_on_planning() {
        let mut flow = install_flow();
        assert_eq!(flow.screen, Screen::Ready);
        assert_eq!(flow.title(), "Install RequestLab");
        flow.start();
        assert_eq!(flow.screen, Screen::Working);
        assert_eq!(flow.stage, Some(Stage::Planning));
        assert!(flow.is_working());
        assert!(!flow.primary_enabled());
    }

    #[test]
    fn transfer_events_drive_the_progress_bar() {
        let mut flow = install_flow();
        flow.start();
        flow.apply(ManagerEvent::Transfer(Transfer {
            downloaded: 5_000_000,
            total: Some(10_000_000),
            bytes_per_second: 2_500_000.0,
        }));
        assert_eq!(flow.percent(), Some(0.5));
        assert_eq!(flow.downloaded, 5_000_000);
        assert!(flow.progress_text().starts_with("50%"));
        assert!(flow.detail.contains("MiB"));
    }

    #[test]
    fn an_unknown_total_leaves_the_bar_indeterminate() {
        let mut flow = install_flow();
        flow.apply(ManagerEvent::Transfer(Transfer {
            downloaded: 10,
            total: None,
            bytes_per_second: 0.0,
        }));
        assert_eq!(flow.percent(), None);
        assert_eq!(flow.progress_text(), "");
        assert!(flow.transfer_text().contains("unknown size"));
    }

    #[test]
    fn stage_changes_clear_stale_download_progress() {
        let mut flow = install_flow();
        flow.apply(ManagerEvent::Transfer(Transfer {
            downloaded: 1,
            total: Some(2),
            bytes_per_second: 1.0,
        }));
        flow.apply(ManagerEvent::stage(Stage::Installing, "Copying files"));
        assert_eq!(flow.percent(), None);
        assert_eq!(flow.status_line(), "Installing: Copying files");
    }

    #[test]
    fn extracting_without_a_count_reports_bytes() {
        let mut flow = install_flow();
        flow.apply(ManagerEvent::Extracting {
            fraction: None,
            bytes: 2048,
        });
        assert!(flow.detail.starts_with("Unpacking"));
        flow.apply(ManagerEvent::Extracting {
            fraction: Some(0.25),
            bytes: 0,
        });
        assert_eq!(flow.percent(), Some(0.25));
    }

    #[test]
    fn warnings_are_kept_separately_from_the_error() {
        let mut flow = install_flow();
        flow.apply(ManagerEvent::Warning("shortcut skipped".to_string()));
        assert_eq!(flow.warnings, vec!["shortcut skipped".to_string()]);
        assert!(flow.error.is_none());
        assert_eq!(flow.log.len(), 1);
        assert_eq!(flow.log[0].level, LogLevel::Warning);
    }

    #[test]
    fn installed_event_sets_the_version_and_directory() {
        let mut flow = install_flow();
        flow.apply(ManagerEvent::Installed {
            version: Version::new(0, 1, 2),
            directory: PathBuf::from("/tmp/RequestLab"),
        });
        assert_eq!(flow.installed_version, Some(Version::new(0, 1, 2)));
        assert_eq!(flow.install_dir, "/tmp/RequestLab");
    }

    #[test]
    fn success_and_failure_are_terminal_and_retryable() {
        let mut flow = install_flow();
        flow.start();
        flow.succeed(Version::new(0, 1, 2), Path::new("/tmp/RequestLab"));
        assert!(flow.is_finished());
        assert_eq!(flow.title(), "RequestLab is installed");
        assert_eq!(flow.primary_label(), "Close");
        assert_eq!(flow.percent(), Some(1.0));

        let mut flow = install_flow();
        flow.start();
        flow.fail("no trusted signing key");
        assert_eq!(flow.screen, Screen::Failed);
        assert_eq!(flow.primary_label(), "Try again");
        assert!(flow.primary_enabled());
        assert_eq!(flow.status_line(), "no trusted signing key");
        assert_eq!(flow.log.last().unwrap().level, LogLevel::Error);
    }

    #[test]
    fn the_log_is_bounded() {
        let mut flow = install_flow();
        for index in 0..MAX_LOG_LINES + 50 {
            flow.apply(ManagerEvent::Warning(format!("warning {index}")));
        }
        assert_eq!(flow.log.len(), MAX_LOG_LINES);
        assert!(flow.log.last().unwrap().text.contains("49"));
    }

    #[test]
    fn starting_again_clears_the_previous_error() {
        let mut flow = install_flow();
        flow.fail("boom");
        flow.start();
        assert!(flow.error.is_none());
        assert_eq!(flow.screen, Screen::Working);
    }

    #[test]
    fn release_information_formats_the_size() {
        let info = ReleaseInfo {
            version: Version::new(1, 2, 3),
            channel: "stable",
            size: 12 * 1024 * 1024,
            notes_url: None,
        };
        assert!(info.size_text().ends_with("MiB"));
    }

    #[test]
    fn update_wording_differs_from_install() {
        let flow = FlowState::new(Operation::Update);
        assert_eq!(flow.title(), "Update RequestLab");
        assert_eq!(flow.primary_label(), "Update");
    }
}
