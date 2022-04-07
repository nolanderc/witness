use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, UdpSocket},
    sync::{
        broadcast::{
            channel as broadcast_channel, Receiver as BroadcastReceiver, Sender as BroadcastSender,
        },
        mpsc::Sender,
    },
    task::JoinHandle,
    time::timeout,
};

use super::ExecutionTrigger;

pub struct NetworkWatcher {
    stop_signal: BroadcastSender<Stop>,
    handles: Vec<JoinHandle<anyhow::Result<()>>>,
}

#[derive(Debug, Copy, Clone)]
struct Stop;

impl NetworkWatcher {
    pub fn new(
        network: &crate::cli::NetworkOptions,
        triggers: Sender<ExecutionTrigger>,
    ) -> anyhow::Result<NetworkWatcher> {
        let (stop_sender, _) = broadcast_channel(1);
        let key = Arc::<str>::from(network.key.as_str());
        let mut handles = Vec::new();

        for &port in network.udp.iter() {
            let socket = std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port)))
                .with_context(|| format!("failed to bind UDP to port {port}"))?;
            socket
                .set_nonblocking(true)
                .context("could not make UDP socket nonblocking")?;
            let socket = UdpSocket::from_std(socket).unwrap();

            handles.push(tokio::spawn(handle_udp_stream(
                socket,
                stop_sender.subscribe(),
                key.clone(),
                triggers.clone(),
            )));
        }

        for &port in network.tcp.iter() {
            let listener = std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))
                .with_context(|| format!("failed to bind TCP to port {port}"))?;
            listener
                .set_nonblocking(true)
                .context("could not make TCP socket nonblocking")?;
            let listener = TcpListener::from_std(listener).unwrap();

            handles.push(tokio::spawn(handle_tcp_stream(
                listener,
                stop_sender.subscribe(),
                key.clone(),
                triggers.clone(),
            )));
        }

        Ok(NetworkWatcher {
            stop_signal: stop_sender,
            handles,
        })
    }

    #[allow(dead_code)]
    pub async fn stop(self) -> anyhow::Result<()> {
        let _ = self.stop_signal.send(Stop);

        for handle in self.handles {
            handle.await.unwrap()?;
        }

        Ok(())
    }
}

async fn handle_udp_stream(
    socket: UdpSocket,
    mut stop_signal: BroadcastReceiver<Stop>,
    key: Arc<str>,
    triggers: Sender<ExecutionTrigger>,
) -> anyhow::Result<()> {
    let mut buffer = vec![0u8; key.len() + 1];

    loop {
        debug!(addr = ?socket.local_addr(), "waiting on UDP");

        let result = tokio::select! {
            _ = stop_signal.recv() => return Ok(()),
            result = socket.recv_from(&mut buffer) => result,
        };

        let (count, addr) = result.context("failed to receive message")?;
        if buffer[..count].starts_with(key.as_bytes()) {
            debug!(?addr, "triggered by UDP client");
            let _ = triggers.try_send(ExecutionTrigger);
        }
    }
}

async fn handle_tcp_stream(
    listener: TcpListener,
    mut stop_signal: BroadcastReceiver<Stop>,
    key: Arc<str>,
    triggers: Sender<ExecutionTrigger>,
) -> anyhow::Result<()> {
    loop {
        debug!(addr = ?listener.local_addr(), "waiting on TCP");

        let incoming = tokio::select! {
            _ = stop_signal.recv() => return Ok(()),
            incoming = listener.accept() => incoming,
        };

        let (mut stream, addr) = incoming.context("failed to accept incoming client")?;
        debug!(?addr, "incoming TCP client");

        let key = key.clone();
        let triggers = triggers.clone();
        tokio::spawn(async move {
            let mut buffer = vec![0u8; key.len()];
            debug!(?addr, "waiting on keyphrase");

            let duration = std::time::Duration::from_secs(5);
            match timeout(duration, stream.read_exact(&mut buffer)).await {
                Err(_) => debug!(?addr, "client timed out"),
                Ok(Err(error)) => debug!(?addr, %error, "failed to receive keyphrase"),
                Ok(Ok(count)) => {
                    if buffer[..count].starts_with(key.as_bytes()) {
                        debug!(?addr, "triggered by TCP client");
                        let _ = triggers.try_send(ExecutionTrigger);
                    }
                }
            }
        });
    }
}
