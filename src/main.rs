//! CLI utility which allows you to listen for specific events and run commands in response.

#![allow(clippy::single_char_pattern)]

#[macro_use]
extern crate tracing;

mod cli;
mod watcher;

use anyhow::{anyhow, Context};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = cli::Arguments::parse();
    init_tracing(&args).context("failed to initialize logging")?;

    if args.network.trigger {
        run_trigger(&args.network).await
    } else {
        run_watch(&args).await
    }
}

async fn run_trigger(args: &cli::NetworkOptions) -> anyhow::Result<()> {
    trigger_udp(&args.udp, &args.key).await?;
    trigger_tcp(&args.tcp, &args.key).await?;
    Ok(())
}

async fn trigger_udp(ports: &[u16], key: &str) -> anyhow::Result<()> {
    use std::net::SocketAddr;

    let socket = tokio::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
        .await
        .context("failed to bind UDP socket")?;

    for &port in ports {
        let count = socket
            .send_to(key.as_bytes(), SocketAddr::from(([0, 0, 0, 0], port)))
            .await
            .with_context(|| format!("failed to send UDP trigger on port {port}"))?;
        if count != key.len() {
            return Err(anyhow!(
                "failed to send entire key over UDP. Maybe it's too big?"
            ));
        }
    }

    Ok(())
}

async fn trigger_tcp(ports: &[u16], key: &str) -> anyhow::Result<()> {
    use std::net::SocketAddr;

    for &port in ports {
        let mut stream = tokio::net::TcpStream::connect(SocketAddr::from(([0, 0, 0, 0], port)))
            .await
            .with_context(|| format!("failed to connect to TCP port {port}"))?;

        stream
            .write_all(key.as_bytes())
            .await
            .with_context(|| format!("failed to write to TCP port {port}"))?;
    }

    Ok(())
}

async fn run_watch(args: &cli::Arguments) -> anyhow::Result<()> {
    // watch sources for updates
    let mut watcher = watcher::Watcher::new(args)?;

    // Setup options for launching the specified command
    let mut command: Command;
    if args.command.len() == 1 {
        command = Command::new(&args.behaviour.shell);
        command.arg("-c").arg(&args.command[0]);
    } else {
        command = Command::new(&args.command[0]);
        command.args(&args.command[1..]);
    }

    command
        .kill_on_drop(true)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);

    'outer: loop {
        // Clear screen before running command
        let clear = !args.behaviour.no_clear;
        if clear {
            let mut stdout = tokio::io::stdout();
            stdout.write_all(b"\x1bc").await?; // <-- VT100 escape code to clear screen
            stdout.flush().await?;
        }

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to run command: {}", args.command.join(" ")))?;

        // if the child process should be restarted as soon as it's done
        let mut restart_pending = false;

        loop {
            tokio::select! {
                // wait for the child to terminate before restarting
                exit_status = child.wait(), if restart_pending => {
                    let status = exit_status.context("waiting for child to terminate")?;
                    info!(exit_status = status.code(), "command terminated");
                    break;
                }

                // look for execution triggers
                event = watcher.receiver.recv() => {
                    match event {
                        None => break 'outer Err(anyhow!("file watcher closed unexpectedly")),
                        Some(watcher::ExecutionTrigger) => {
                            if args.behaviour.wait {
                                restart_pending = true;
                            } else {
                                terminate_process(child).await?;
                                break
                            }
                        },
                    }
                }

                // catch any interrupts so that we can cleanup properly
                _ = &mut interrupt => {
                    return Ok(())
                }
            }
        }
    }
}

async fn terminate_process(mut child: Child) -> anyhow::Result<()> {
    info!(
        pid = child.id(),
        "waiting for child process to terminate..."
    );
    let _ = child.start_kill();
    child.wait().await?;
    Ok(())
}

fn init_tracing(args: &cli::Arguments) -> anyhow::Result<()> {
    use tracing::level_filters::LevelFilter;
    let default_filter = if args.verbose {
        LevelFilter::INFO
    } else {
        LevelFilter::WARN
    };

    let variable_name = "WITNESS_LOG";

    let directives = match std::env::var(variable_name) {
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(anyhow!("WITNESS_LOG did not contain valid Unicode data"))
        }
        Ok(level) => level,
    };

    let env_filter = tracing_subscriber::filter::EnvFilter::builder()
        .with_default_directive(default_filter.into())
        .parse(&directives)
        .with_context(|| {
            format!("{variable_name} contained an invalid directive: {directives:?}")
        })?;

    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .init();

    Ok(())
}
