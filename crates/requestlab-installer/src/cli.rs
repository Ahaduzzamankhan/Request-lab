//! Command line parsing for the installer.
//!
//! Without arguments the installer opens its window; with `--silent` it performs
//! the same installation in a console, which is what unattended setups use.

use std::path::PathBuf;

use requestlab_manager::manifest::Channel;

use crate::flow;

#[derive(Debug, Clone, PartialEq)]
pub struct SilentOptions {
    pub install_dir: PathBuf,
    pub channel: Channel,
    pub shortcuts: bool,
    pub register_uninstall: bool,
    pub launch: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Window,
    Silent(SilentOptions),
    Help,
    Version,
}

pub const USAGE: &str = "\
RequestLab installer

USAGE:
    requestlab-installer                       Open the installer window
    requestlab-installer --silent [options]    Install without a window

OPTIONS:
    --silent             Install in the console instead of a window.
    --install-dir DIR    Where to install (default: the per-user programs folder).
    --channel stable     Release channel to install from.
    --no-shortcuts       Do not create desktop or start menu shortcuts.
    --no-registry        Do not register an uninstall entry.
    --launch             Start RequestLab when the installation finishes.
    --help               Show this help.
    --version            Show the installer version.";

pub fn parse<I: IntoIterator<Item = String>>(arguments: I) -> Result<Mode, String> {
    let mut arguments = arguments.into_iter().peekable();
    if arguments.peek().is_none() {
        return Ok(Mode::Window);
    }

    let mut silent = false;
    let mut install_dir = flow::suggested_install_dir();
    let mut channel = flow::default_channel();
    let mut shortcuts = true;
    let mut register_uninstall = true;
    let mut launch = false;

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
            "--silent" | "--quiet" => silent = true,
            "--install-dir" => install_dir = PathBuf::from(value("--install-dir")?),
            "--channel" => channel = Channel::parse(&value("--channel")?)?,
            "--no-shortcuts" => shortcuts = false,
            "--no-registry" => register_uninstall = false,
            "--launch" => launch = true,
            other => return Err(format!("unknown argument '{other}'")),
        }
    }

    if !silent {
        // A window run ignores installer flags rather than silently installing.
        return Ok(Mode::Window);
    }
    Ok(Mode::Silent(SilentOptions {
        install_dir,
        channel,
        shortcuts,
        register_uninstall,
        launch,
    }))
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
    fn silent_mode_collects_its_options() {
        let mode = parse_str(&[
            "--silent",
            "--install-dir",
            "C:/Apps/RequestLab",
            "--channel",
            "stable",
            "--no-shortcuts",
            "--no-registry",
            "--launch",
        ])
        .unwrap();
        match mode {
            Mode::Silent(options) => {
                assert_eq!(options.install_dir, PathBuf::from("C:/Apps/RequestLab"));
                assert!(!options.shortcuts);
                assert!(!options.register_uninstall);
                assert!(options.launch);
            }
            other => panic!("unexpected mode {other:?}"),
        }
    }

    #[test]
    fn flags_without_silent_do_not_install() {
        assert_eq!(parse_str(&["--launch"]).unwrap(), Mode::Window);
    }

    #[test]
    fn a_missing_value_is_an_error() {
        let error = parse_str(&["--silent", "--install-dir"]).unwrap_err();
        assert!(error.contains("--install-dir needs a value"));
    }

    #[test]
    fn an_unknown_channel_is_rejected() {
        assert!(parse_str(&["--silent", "--channel", "nightly"]).is_err());
    }

    #[test]
    fn help_and_version_are_recognised() {
        assert_eq!(parse_str(&["--help"]).unwrap(), Mode::Help);
        assert_eq!(parse_str(&["--version"]).unwrap(), Mode::Version);
    }
}
