mod parse;

use std::{ffi::OsString, path::PathBuf, str::FromStr, time::Duration};

use anyhow::{anyhow, Context};

/// Trigger a command in response to certain events
#[derive(Debug, clap::Parser)]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(author = "Christofer Nolander <christofer.nolander@gmail.com>")]
#[clap(global_setting = clap::AppSettings::DeriveDisplayOrder)]
#[clap(trailing_var_arg(true))]
pub struct Arguments {
    /// Enable more verbose logging.
    #[clap(long)]
    #[clap(global = true)]
    pub verbose: bool,

    /// Watch over file changes
    #[clap(next_help_heading = "FILES")]
    #[clap(flatten)]
    pub files: FileOptions,

    /// Listen for network connections
    #[clap(next_help_heading = "NETWORK")]
    #[clap(flatten)]
    pub network: NetworkOptions,

    #[clap(flatten)]
    #[clap(next_help_heading = "BEHAVIOUR")]
    pub behaviour: BehaviourOptions,

    /// The command to execute. This is passed to your shell.
    ///
    /// If you want to chain commands or pipe output from one command to another, surround the
    /// commands in quotes. Example: `witness "ls | less"` would run `ls` and pipe its output to
    /// `less`.
    #[clap(required_unless_present = "trigger")]
    #[clap(multiple_values = true)]
    #[clap(value_hint = clap::ValueHint::CommandWithArguments)]
    pub command: Vec<String>,
}

/// Options affecting how watched files are treated.
#[derive(Debug, clap::Parser)]
#[clap(
    group = clap::ArgGroup::new("files")
        .args(&["paths", "debounce", "extensions", "no-git-ignore"])
        .multiple(true)
)]
pub struct FileOptions {
    /// Paths to watch for changes
    #[clap(long = "path")]
    #[clap(default_value = ".")]
    #[clap(default_value_if("udp", None, None))]
    #[clap(default_value_if("tcp", None, None))]
    #[clap(value_delimiter = ',')]
    #[clap(multiple_occurrences = true)]
    pub paths: Vec<PathBuf>,

    /// Duration between when a file changes and execution is triggered
    #[clap(long)]
    #[clap(default_value = "100ms")]
    #[clap(parse(try_from_str = parse::duration_from_str))]
    pub debounce: Duration,

    /// Only files with these extensions trigger execution
    #[clap(short, long)]
    #[clap(value_delimiter = ',')]
    pub extensions: Option<Vec<OsString>>,

    /// Include files excluded by Git
    #[clap(long)]
    pub no_git_ignore: bool,
}

/// Options affecting how network connections are treated
#[derive(Debug, clap::Parser)]
#[clap(
    group = clap::ArgGroup::new("network")
        .args(&["udp", "tcp", "key", "trigger"])
        .multiple(true)
)]
pub struct NetworkOptions {
    /// UDP packets to these ports trigger execution
    #[clap(long)]
    #[clap(value_delimiter = ',')]
    #[clap(multiple_occurrences = true)]
    pub udp: Vec<u16>,

    /// TCP packets to these ports trigger execution
    #[clap(long)]
    #[clap(value_delimiter = ',')]
    #[clap(multiple_occurrences = true)]
    pub tcp: Vec<u16>,

    /// Only network requests containing this exact string will trigger execution.
    /// Set to the empty string to allow any request.
    #[clap(long = "key")]
    #[clap(default_value = DEFAULT_KEY)]
    pub key: String,

    /// Send a network packet instead of listening for it. Can be used to trigger another instance
    /// of witness running on the same machine.
    #[clap(long)]
    #[clap(conflicts_with_all = &["command", "files"])]
    pub trigger: bool,
}

/// The default key used for network transmissions.
const DEFAULT_KEY: &str = "witness-key";

/// Options affecting behaivour of this utility
#[derive(Debug, clap::Parser)]
pub struct BehaviourOptions {
    /// Don't clear the screen before command invocation
    #[clap(short = 'c', long)]
    pub no_clear: bool,

    /// Wait on the command to finish before restarting
    #[clap(short, long)]
    pub wait: bool,

    /// The shell used to interpret commands
    #[clap(long)]
    #[clap(env = "SHELL")]
    pub shell: OsString,
}

impl Arguments {
    pub fn parse() -> Arguments {
        <Arguments as clap::Parser>::parse()
    }

    #[allow(dead_code)]
    fn emit_error<T: std::fmt::Display>(kind: clap::ErrorKind, message: T) -> ! {
        let mut command = <Self as clap::CommandFactory>::command();
        clap::Error::raw(kind, message).format(&mut command).exit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse_args(args: &str) -> Arguments {
        Arguments::parse_from(args.split_whitespace())
    }

    /// Watch the default path
    #[test]
    fn watch_default() {
        let args = parse_args("witness cargo check");
        assert_eq!(args.files.paths, [PathBuf::from(".")]);
    }

    /// Watch a specific path
    #[test]
    fn watch_specific() {
        let args = parse_args("witness --path src cargo check");
        assert_eq!(args.files.paths, [PathBuf::from("src")]);
    }

    /// Multiple paths using a single flag
    #[test]
    fn watch_many() {
        let args = parse_args("witness --path=src,test,examples cargo check");
        assert_eq!(
            args.files.paths,
            [
                PathBuf::from("src"),
                PathBuf::from("test"),
                PathBuf::from("examples")
            ]
        );
    }

    /// Multiple paths using multiple flags
    #[test]
    fn watch_multiple() {
        let args = parse_args("witness --path=src --path=test --path=examples cargo check");
        assert_eq!(
            args.files.paths,
            [
                PathBuf::from("src"),
                PathBuf::from("test"),
                PathBuf::from("examples")
            ]
        );
    }

    /// If there is a flag enabling network usage, disable default file watching
    #[test]
    fn udp_disables_files() {
        let args = parse_args("witness --udp=1234 cargo check");
        assert_eq!(args.files.paths, Vec::<PathBuf>::new());
        assert_eq!(args.network.udp, vec![1234]);
    }

    /// If there is a flag enabling network usage, disable default file watching
    #[test]
    fn tcp_disables_files() {
        let args = parse_args("witness --tcp=1234 cargo check");
        assert_eq!(args.files.paths, Vec::<PathBuf>::new());
        assert_eq!(args.network.tcp, vec![1234]);
    }

    /// If there is a flag enabling network usage, disable default file watching
    #[test]
    fn udp_and_files() {
        let args = parse_args("witness --udp=1234 --path src cargo check");
        assert_eq!(args.files.paths, vec![PathBuf::from("src")]);
        assert_eq!(args.network.udp, vec![1234]);
    }
}
