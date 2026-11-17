//! Applying a staged update after the application has exited.
//!
//! The engine performs the transactional swap; this module supplies the parts a
//! separate process must do: wait for the old process, recover an interrupted
//! earlier attempt, keep a log the user can read if a window never reappears, and
//! optionally start the new build.

use std::io::Write as _;
use std::path::Path;

use requestlab_manager::events::ManagerEvent;
use requestlab_manager::installer::InstallOptions;
use requestlab_manager::rollback::{self, Recovery};
use requestlab_manager::updater::{self, StagedUpdate};
use requestlab_manager::{paths, platform};

use crate::args::ApplyArgs;

/// A log that goes both to stderr and to a file inside the installation.
pub struct HelperLog {
    file: Option<std::fs::File>,
}

impl HelperLog {
    pub fn open(install_dir: &Path) -> Self {
        let _ = paths::ensure_dir(&paths::work_dir(install_dir));
        let path = paths::work_dir(install_dir).join("update-helper.log");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        Self { file }
    }

    pub fn line(&mut self, message: impl AsRef<str>) {
        let message = message.as_ref();
        eprintln!("{message}");
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "[{}] {message}", chrono::Utc::now().to_rfc3339());
            let _ = file.flush();
        }
    }
}

/// A short, secret-free description of an engine event.
pub fn describe(event: &ManagerEvent) -> String {
    match event {
        ManagerEvent::Stage { stage, detail } => format!("{}: {detail}", stage.label()),
        ManagerEvent::Transfer(transfer) => match transfer.total {
            Some(total) if total > 0 => format!(
                "Downloaded {} of {}",
                requestlab_manager::downloader::human_bytes(transfer.downloaded),
                requestlab_manager::downloader::human_bytes(total)
            ),
            _ => format!(
                "Downloaded {}",
                requestlab_manager::downloader::human_bytes(transfer.downloaded)
            ),
        },
        ManagerEvent::Extracting { fraction, bytes } => match fraction {
            Some(fraction) => format!("Extracting {:.0}%", fraction * 100.0),
            None => format!("Extracting {} bytes", bytes),
        },
        ManagerEvent::Warning(message) => format!("warning: {message}"),
        ManagerEvent::Installed { version, directory } => {
            format!("installed {version} in {}", directory.display())
        }
    }
}

/// Apply the staged update for an installation.
pub fn run(args: &ApplyArgs) -> Result<(), String> {
    let install_dir = args.install_dir.clone();
    if !install_dir.is_dir() {
        return Err(format!(
            "{} is not an installation directory",
            install_dir.display()
        ));
    }
    let mut log = HelperLog::open(&install_dir);
    log.line(format!(
        "update helper {} starting for {}",
        requestlab_manager::VERSION,
        install_dir.display()
    ));

    if let Some(pid) = args.pid.filter(|pid| *pid > 0) {
        log.line(format!("waiting for pid {pid} to exit"));
        updater::wait_for_application(pid, args.timeout).map_err(|error| {
            let message = error.to_string();
            log.line(format!("giving up: {message}"));
            message
        })?;
        if platform::process_is_running(pid) {
            let message = format!("RequestLab (pid {pid}) is still running");
            log.line(&message);
            return Err(message);
        }
    }

    match rollback::recover_if_interrupted(&install_dir) {
        Ok(Recovery::Clean) => log.line("no interrupted update to recover"),
        Ok(Recovery::CleanedUp) => log.line("cleaned up a partial update that never started"),
        Ok(Recovery::Restored(version)) => log.line(format!(
            "restored RequestLab {version} after an interrupted update"
        )),
        Err(error) => {
            let message = format!("could not recover the previous installation: {error}");
            log.line(&message);
            return Err(message);
        }
    }

    let record = StagedUpdate::record_path(&install_dir);
    if let Some(given) = &args.record {
        if given != &record {
            let message = format!(
                "the staged record {} is not the one for this installation ({})",
                given.display(),
                record.display()
            );
            log.line(&message);
            return Err(message);
        }
    }
    if !record.is_file() {
        let message = "no staged update is waiting; download the update again".to_string();
        log.line(&message);
        return Err(message);
    }

    let staged = StagedUpdate::load(&install_dir).map_err(|error| {
        let message = error.to_string();
        log.line(format!("cannot read the staged update: {message}"));
        message
    })?;
    log.line(format!(
        "applying RequestLab {} ({})",
        staged.version,
        staged.channel.as_str()
    ));

    let options = InstallOptions {
        install_dir: install_dir.clone(),
        channel: staged.channel,
        shortcuts: true,
        register_uninstall: true,
    };

    let outcome = {
        let mut sink = |event: ManagerEvent| log.line(describe(&event));
        updater::apply_staged(&staged, &install_dir, &options, &mut sink)
    };

    match outcome {
        Ok(outcome) => {
            log.line(format!(
                "RequestLab {} is installed in {}",
                outcome.metadata.version,
                install_dir.display()
            ));
            for warning in &outcome.warnings {
                log.line(format!("warning: {warning}"));
            }
            if args.launch {
                let executable = paths::app_path(&install_dir);
                log.line(format!("starting {}", executable.display()));
                if let Err(error) = platform::launch(&executable) {
                    let message = format!("the update is installed but could not start: {error}");
                    log.line(&message);
                    return Err(message);
                }
            }
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            log.line(format!("the update failed: {message}"));
            log.line("the previous version was restored");
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::ApplyArgs;
    use requestlab_manager::events::Stage;
    use std::path::PathBuf;
    use std::time::Duration;

    fn args(install_dir: PathBuf) -> ApplyArgs {
        ApplyArgs {
            record: None,
            install_dir,
            pid: None,
            launch: false,
            timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn a_missing_installation_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("does-not-exist");
        let error = run(&args(missing)).unwrap_err();
        assert!(error.contains("not an installation directory"));
    }

    #[test]
    fn without_a_staged_record_the_helper_refuses_to_act() {
        let temp = tempfile::tempdir().unwrap();
        let error = run(&args(temp.path().to_path_buf())).unwrap_err();
        assert!(error.contains("no staged update"));
    }

    #[test]
    fn a_mismatched_record_is_rejected_before_anything_changes() {
        let temp = tempfile::tempdir().unwrap();
        let mut options = args(temp.path().to_path_buf());
        options.record = Some(temp.path().join("elsewhere.json"));
        let error = run(&options).unwrap_err();
        assert!(error.contains("not the one for this installation"));
    }

    #[test]
    fn events_are_described_without_secrets() {
        let text = describe(&ManagerEvent::stage(Stage::Installing, "Copying files"));
        assert_eq!(text, "Installing: Copying files");
        let installed = describe(&ManagerEvent::Installed {
            version: requestlab_manager::Version::new(0, 1, 2),
            directory: PathBuf::from("/tmp/RequestLab"),
        });
        assert!(installed.contains("0.1.2"));
    }
}
