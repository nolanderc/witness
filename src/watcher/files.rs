use anyhow::Context;
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    path::Path,
    time::Duration,
};
use tokio::sync::mpsc::Sender;

use crate::cli;

use super::ExecutionTrigger;

pub struct FileWatcher {
    /// We keep the watcher around so that it keeps sending events in the background
    #[allow(dead_code)]
    watcher: notify::RecommendedWatcher,
}

impl FileWatcher {
    pub fn new(
        options: &cli::FileOptions,
        triggers: Sender<ExecutionTrigger>,
    ) -> anyhow::Result<FileWatcher> {
        use notify::Watcher as _;

        let debounce = options.debounce;

        let (sender, receiver) = std::sync::mpsc::channel();
        let mut watcher = notify::raw_watcher(sender)?;

        // Watch the given path
        for path in options.paths.iter() {
            info!(?path, "watching path");
            watcher
                .watch(path, notify::RecursiveMode::Recursive)
                .with_context(|| format!("failed to watch path: {}", path.display()))?;
        }

        let filter = FileFilter::from_args(options);

        // Create a thread to glue sync and async parts together
        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                if let Some(path) = Self::modified_file(&event) {
                    match filter.matches_path(path) {
                        Ok(()) => {
                            info!(?path, ?event, "file trigger");
                            let _ = triggers.try_send(ExecutionTrigger);

                            // skip all remaining values to avoid triggering twice
                            Self::skip_for_duration(&receiver, debounce);
                        }
                        Err(reason) => {
                            info!(?reason, ?path, "ignoring modification");
                        }
                    }
                }
            }
        });

        Ok(FileWatcher { watcher })
    }

    /// Skip all elements in the receiver for the full duration
    fn skip_for_duration<T>(receiver: &std::sync::mpsc::Receiver<T>, duration: Duration) {
        let deadline = std::time::Instant::now() + duration;
        loop {
            // how much time until the deadline is reached?
            let now = std::time::Instant::now();
            let remaining = match deadline.checked_duration_since(now) {
                Some(duration) => duration,
                None => break,
            };

            // skip messages while we are within the deadline
            if receiver.recv_timeout(remaining).is_err() {
                break;
            }
        }
    }

    /// Given an event, returns the path that has been modified (if any)
    fn modified_file(event: &notify::RawEvent) -> Option<&Path> {
        use notify::Op;
        let op = *event.op.as_ref().ok()?;
        if op.intersects(Op::WRITE | Op::CREATE | Op::REMOVE | Op::RENAME | Op::CHMOD) {
            event.path.as_deref()
        } else {
            None
        }
    }
}

pub struct FileFilter {
    /// Only allow these specific extensions, or anything
    extensions: Option<BTreeSet<OsString>>,

    /// Files ignored by git should be respected
    git_ignore: bool,
}

#[derive(Debug)]
enum FilterReason {
    Extension,
    GitIgnore,
}

impl FileFilter {
    pub fn from_args(options: &cli::FileOptions) -> FileFilter {
        FileFilter {
            extensions: options
                .extensions
                .as_ref()
                .map(|extensions| extensions.iter().cloned().collect()),

            git_ignore: !options.no_git_ignore,
        }
    }

    fn matches_path(&self, path: &Path) -> Result<(), FilterReason> {
        self.check_extension(path)?;
        if self.git_ignore {
            self.check_git_ignore(path)?;
        }
        Ok(())
    }

    fn check_extension(&self, path: &Path) -> Result<(), FilterReason> {
        if let Some(extensions) = &self.extensions {
            match path.extension() {
                Some(ext) if extensions.contains(ext) => {}
                _ => return Err(FilterReason::Extension),
            }
        }

        Ok(())
    }

    fn check_git_ignore(&self, path: &Path) -> Result<(), FilterReason> {
        use std::process::{Command, Stdio};

        if Self::contains_git_dir(path) {
            return Err(FilterReason::GitIgnore);
        }

        let result = Command::new("git")
            .arg("check-ignore")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match result {
            Err(error) => warn!(?error, "could not execute `git`"),
            Ok(status) if status.code() == Some(0) => return Err(FilterReason::GitIgnore),
            Ok(status) if status.code() == Some(1) => return Ok(()),
            Ok(status) => warn!(?status, "`git-check-ignore` exited with error"),
        }

        Ok(())
    }

    fn contains_git_dir(path: &Path) -> bool {
        path.ancestors()
            .any(|path| path.file_name() == Some(OsStr::new(".git")))
    }
}
