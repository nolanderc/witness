mod files;
mod network;

use anyhow::Context;
use tokio::sync::mpsc::Receiver;

use crate::cli;

/// Watches for events on a set of sources
pub struct Watcher {
    #[allow(dead_code)]
    files: Option<files::FileWatcher>,
    #[allow(dead_code)]
    network: Option<network::NetworkWatcher>,
    pub receiver: Receiver<ExecutionTrigger>,
}

/// Sent when a source triggers re-execution of the command
pub struct ExecutionTrigger;

impl Watcher {
    pub fn new(args: &cli::Arguments) -> anyhow::Result<Watcher> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);

        let files = files::FileWatcher::new(&args.files, sender.clone())
            .context("failed to create file watcher")?;

        let network = network::NetworkWatcher::new(&args.network, sender)
            .context("failed to create network listener")?;

        Ok(Watcher {
            files: Some(files),
            network: Some(network),
            receiver,
        })
    }
}
