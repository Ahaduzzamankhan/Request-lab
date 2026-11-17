//! Command line parsing for the updater.

use std::path::PathBuf;

use requestlab_manager::manifest::Channel;
use requestlab_manager::paths;

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub install_dir: PathBuf,
    pub channel: Channel,
    pub pid: Option<u32>,
    pub launch: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Window,
    /// Report whether an update exists; no files are touched.
    Check(Options),
    /// Download and install.
    Silent(Options),
    Help,
    Version,
}

pub const USAGE: &str = "\
RequestLab updater

USAGE:
    requestlab-updater                          Open the updater window
    requestlab-updater --check [options]        Report whether an update exists
    requestlab-updater --silent [options]       Download and install it

OPTIONS:
    --check              Only check; exit code 0 means an update is available.
    --silent             Install without a window.
    --install-dir DIR    The installation to update.
    --channel stable     Release channel to follow.
    --pid PID            Wait for this RequestLab process to exit (uses the helper).
    --launch             Start RequestLab when the update finishes.
    --help               Show this help.
    --version            Show the updater version.";

pub fn parse<I: IntoIterator<Item = String>>(arguments: I) -> Result<Mode, String> {
    let mut arguments = arguments.into_iter().peekable();
    if arguments.peek().is_none() {
        return Ok(Mode::Window);
    }

    let mut check = false;
    let mut silent = false;
    let mut options = Options {
        install_dir: paths::default_install_dir(),
        channel: Channel::Stable,
        pid: None,
        launch: false,
    };

    while let Some(arg) = arguments.next() {
        let mut value = |name: &str| -> Result<String, String> {
            arguments
                .next()
                .map(|value| value.trim().to_string())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--help" | "-h" => return Ok(Mode::Help),
            "--version" | "-V" => return Ok(Mode::Version),
            "--check" => check = true,
            "--silent" | "--quiet" => silent = true,
            "--install-dir" => options.install_dir = PathBuf::from(value("--install-dir")?),
            "--channel" => options.channel = Channel::parse(&value("--channel")?)?,
            "--pid" => {
                let raw = value("--pid")?;
                options.pid = Some(
                    raw.parse::<u32>()
                        .map_err(|_| format!("--pid expects a number, got '{raw}'"))?,
                );
            }
            "--launch" => options.launch = true,
            other => return Err(format!("unknown argument '{other}'")),
        }
    }

    if check {
        return Ok(Mode::Check(options));
    }
    if silent {
        return Ok(Mode::Silent(options));
    }
    Ok(Mode::Window)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Mode, String> {
        parse(args.iter().map(|value| value.to_string()))
    }

    #[test]
    fn no_arguments_opens_the_window() {
        assert_eq!(parse_str(&[]).unwrap(), Mode::Window);
    }

    #[test]
    fn check_and_silent_are_distinct_modes() {
        match parse_str(&["--check"]).unwrap() {
            Mode::Check(options) => assert!(options.pid.is_none()),
            other => panic!("unexpected mode {other:?}"),
        }
        match parse_str(&["--silent", "--pid", "7", "--launch"]).unwrap() {
            Mode::Silent(options) => {
                assert_eq!(options.pid, Some(7));
                assert!(options.launch);
            }
            other => panic!("unexpected mode {other:?}"),
        }
    }

    #[test]
    fn a_check_wins_over_silent() {
        assert!(matches!(
            parse_str(&["--silent", "--check"]).unwrap(),
            Mode::Check(_)
        ));
    }

    #[test]
    fn flags_without_a_mode_do_not_act() {
        assert_eq!(parse_str(&["--launch"]).unwrap(), Mode::Window);
    }

    #[test]
    fn an_invalid_pid_is_rejected() {
        assert!(parse_str(&["--silent", "--pid", "later"]).is_err());
    }

    #[test]
    fn help_and_version_are_recognised() {
        assert_eq!(parse_str(&["--help"]).unwrap(), Mode::Help);
        assert_eq!(parse_str(&["--version"]).unwrap(), Mode::Version);
    }
}
