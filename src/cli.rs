mod parse;

use std::{ffi::OsString, path::PathBuf, str::FromStr, time::Duration};

use anyhow::{anyhow, Context};

/// Trigger a command in response to certain events
#[derive(Debug, clap::Parser)]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(author = "Christofer Nolander <christofer.nolander@gmail.com>")]
#[clap(trailing_var_arg(true))]
#[clap(global_setting = clap::AppSettings::DeriveDisplayOrder)]
#[clap(args_conflicts_with_subcommands = true)]
pub struct Arguments {
    #[clap(long)]
    #[clap(global = true)]
    pub verbose: bool,

    #[clap(subcommand)]
    subcommand: Option<Subcommand>,

    /// The default subcommand
    #[clap(flatten)]
    watch: Watch,
}

#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
    Watch(Watch),
    Trigger(Trigger),
}

/// Trigger another listener
#[derive(Debug, clap::Parser)]
pub struct Trigger {
    /// Trigger any UDP listeners on these ports
    #[clap(long)]
    #[clap(value_delimiter = ',')]
    #[clap(multiple_occurrences = true)]
    pub udp: Vec<u16>,

    /// Trigger any TCP listeners on these ports
    #[clap(long)]
    #[clap(value_delimiter = ',')]
    #[clap(multiple_occurrences = true)]
    pub tcp: Vec<u16>,

    /// Key to use to trigger execution (must match that of the listener)
    #[clap(long = "key")]
    #[clap(default_value = DEFAULT_KEY)]
    pub key: String,
}

/// Listen for events that could trigger execution
#[derive(Debug, clap::Parser)]
#[clap(trailing_var_arg(true))]
pub struct Watch {
    /// The shell used for executing commands
    #[clap(long)]
    #[clap(env = "SHELL")]
    pub shell: OsString,

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

    /// The command to execute. This is passed to your shell
    #[clap(multiple_values = true)]
    #[clap(value_hint = clap::ValueHint::CommandWithArguments)]
    pub command: Vec<String>,
}

/// Options affecting how watched files are treated.
#[derive(Debug, clap::Parser)]
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
}

/// The default key used for network transmissions.
const DEFAULT_KEY: &'static str = "witness-key";

/// Options affecting behaivour of this utility
#[derive(Debug, clap::Parser)]
pub struct BehaviourOptions {
    /// Clear the screen before every command invocation
    #[clap(short, long)]
    pub clear: bool,

    /// Restart the command as soon as a source triggers
    #[clap(short, long)]
    pub restart: bool,
}

impl Arguments {
    pub fn parse() -> Arguments {
        let args = <Arguments as clap::Parser>::parse();
        args.validate();
        args
    }

    fn validate(&self) {
        let watch = match &self.subcommand {
            Some(Subcommand::Watch(watch)) => Some(watch),
            None => Some(&self.watch),
            _ => None,
        };

        if let Some(watch) = watch {
            if watch.command.is_empty() {
                Self::emit_error(
                    clap::ErrorKind::MissingRequiredArgument,
                    "Did not provide the required argument <COMMAND>...",
                );
            }
        }
    }

    fn emit_error<T: std::fmt::Display>(kind: clap::ErrorKind, message: T) -> ! {
        let mut command = <Self as clap::CommandFactory>::command();
        clap::Error::raw(kind, message).format(&mut command).exit();
    }

    /// Convert into the underlying subcommand
    pub fn subcommand(self) -> Subcommand {
        self.subcommand
            .unwrap_or_else(move || Subcommand::Watch(self.watch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse_watch(args: &str) -> Watch {
        let args = Arguments::parse_from(args.split_whitespace());
        match args.subcommand() {
            Subcommand::Watch(watch) => watch,
            Subcommand::Trigger(_) => unreachable!(),
        }
    }

    /// Watch the default path
    #[test]
    fn watch_default() {
        let args = parse_watch("witness watch cargo check");
        assert_eq!(args.files.paths, [PathBuf::from(".")]);
    }

    /// Watch a specific path
    #[test]
    fn watch_specific() {
        let args = parse_watch("witness watch --path src cargo check");
        assert_eq!(args.files.paths, [PathBuf::from("src")]);
    }

    /// Multiple paths using a single flag
    #[test]
    fn watch_many() {
        let args = parse_watch("witness watch --path=src,test,examples cargo check");
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
        let args = parse_watch("witness watch --path=src --path=test --path=examples cargo check");
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
        let args = parse_watch("witness watch --udp=1234 cargo check");
        assert_eq!(args.files.paths, Vec::<PathBuf>::new());
        assert_eq!(args.network.udp, vec![1234]);
    }

    /// If there is a flag enabling network usage, disable default file watching
    #[test]
    fn tcp_disables_files() {
        let args = parse_watch("witness watch --tcp=1234 cargo check");
        assert_eq!(args.files.paths, Vec::<PathBuf>::new());
        assert_eq!(args.network.tcp, vec![1234]);
    }

    /// If there is a flag enabling network usage, disable default file watching
    #[test]
    fn udp_and_files() {
        let args = parse_watch("witness watch --udp=1234 --path src cargo check");
        assert_eq!(args.files.paths, vec![PathBuf::from("src")]);
        assert_eq!(args.network.udp, vec![1234]);
    }
}
