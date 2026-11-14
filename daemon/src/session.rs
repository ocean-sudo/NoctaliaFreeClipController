//! One RFCOMM session with a connected FreeClip 2.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{split, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::protocol::{
    self, DualRow, Model, Request, TapKind, CMD_AUTO_PAUSE_READ, CMD_DOUBLE_READ, CMD_DUAL_CHANGED,
    CMD_DUAL_ENABLED_READ, CMD_DUAL_ENUMERATE, CMD_EQ_READ, CMD_LANGUAGE_READ, CMD_LATENCY,
    CMD_LONG_READ, CMD_NOISE_READ, CMD_QUALITY_READ, CMD_SWIPE_READ, CMD_TRIPLE_READ,
};
use crate::spp::{self, Packet};

struct LinkInner {
    write: Mutex<tokio::io::WriteHalf<bluer::rfcomm::Stream>>,
    /// Synchronous so a cancelled request can drop its waiter without awaiting.
    pending: StdMutex<HashMap<u16, oneshot::Sender<Packet>>>,
}

fn lock_pending(
    pending: &StdMutex<HashMap<u16, oneshot::Sender<Packet>>>,
) -> std::sync::MutexGuard<'_, HashMap<u16, oneshot::Sender<Packet>>> {
    pending.lock().unwrap_or_else(|err| err.into_inner())
}

/// Removes the in-flight waiter if the request future is dropped or times out.
struct PendingGuard {
    inner: Arc<LinkInner>,
    command: u16,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        lock_pending(&self.inner.pending).remove(&self.command);
    }
}

#[derive(Clone)]
struct Link {
    inner: Arc<LinkInner>,
}

impl Link {
    async fn send(&self, packet: &Packet) -> Result<(), String> {
        spp::log_packet("tx", packet);
        let bytes = packet.encode();
        let mut write = self.inner.write.lock().await;
        write
            .write_all(&bytes)
            .await
            .map_err(|err| err.to_string())?;
        write.flush().await.map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn request(&self, packet: Packet) -> Result<Packet, String> {
        let command = packet.command;
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = lock_pending(&self.inner.pending);
            if pending.contains_key(&command) {
                return Err(format!("command {command:04x} is already waiting"));
            }
            pending.insert(command, tx);
        }
        let _guard = PendingGuard {
            inner: Arc::clone(&self.inner),
            command,
        };
        self.send(&packet).await?;
        match tokio::time::timeout(Duration::from_secs(3), rx).await {
            Ok(Ok(packet)) => Ok(packet),
            Ok(Err(_)) => Err("session closed while waiting for the headset".into()),
            Err(_) => Err(format!("headset did not answer command {command:04x}")),
        }
    }
}

pub async fn connect(
    address: bluer::Address,
    channel: u8,
) -> Result<bluer::rfcomm::Stream, String> {
    let socket = bluer::rfcomm::Socket::new().map_err(|err| format!("rfcomm socket: {err}"))?;
    let target = bluer::rfcomm::SocketAddr::new(address, channel);
    match tokio::time::timeout(Duration::from_secs(8), socket.connect(target)).await {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(err)) => Err(format!("rfcomm connect: {err}")),
        Err(_) => Err("rfcomm connect timed out".into()),
    }
}

pub struct LiveSession {
    link: Link,
    incoming: mpsc::Receiver<Packet>,
}

impl LiveSession {
    pub fn start(stream: bluer::rfcomm::Stream) -> Self {
        let (read, write) = split(stream);
        let (tx, rx) = mpsc::channel(32);
        let link = Link {
            inner: Arc::new(LinkInner {
                write: Mutex::new(write),
                pending: StdMutex::new(HashMap::new()),
            }),
        };
        let reader_link = link.clone();
        tokio::spawn(async move { read_loop(read, reader_link, tx).await });
        Self { link, incoming: rx }
    }

    pub async fn pull(&mut self, model: &mut Model) -> Result<(), String> {
        let battery = self.link.request(protocol::battery_query()).await?;
        protocol::apply_battery(model, &battery);
        self.drain(model);

        let info = self
            .optional(model, protocol::info_query(), "device info")
            .await;
        if let Some(packet) = info {
            protocol::apply_info(model, &packet);
        }
        self.query_features(model).await;
        Ok(())
    }

    pub async fn handle(&mut self, model: &mut Model, request: Request) -> Result<(), String> {
        match request {
            Request::Get => Ok(()),
            Request::Refresh => self.pull(model).await,
            Request::SetAutoPause(enabled) => {
                let packet = self
                    .link
                    .request(protocol::auto_pause_write(enabled))
                    .await?;
                protocol::apply_auto_pause(model, &packet);
                self.reread(
                    model,
                    Packet::read(CMD_AUTO_PAUSE_READ, &[1]),
                    protocol::apply_auto_pause,
                )
                .await
            }
            Request::SetLowLatency(enabled) => {
                self.link.request(protocol::latency_write(enabled)).await?;
                tokio::time::sleep(Duration::from_millis(400)).await;
                self.reread(
                    model,
                    Packet::read(CMD_LATENCY, &[2]),
                    protocol::apply_latency,
                )
                .await
            }
            Request::SetSoundQuality(value) => {
                self.link.request(protocol::quality_write(&value)?).await?;
                self.reread(
                    model,
                    Packet::read(CMD_QUALITY_READ, &[1]),
                    protocol::apply_quality,
                )
                .await
            }
            Request::SetEqualizer(value) => {
                self.link
                    .request(protocol::equalizer_write(&value)?)
                    .await?;
                self.reread(
                    model,
                    Packet::read(CMD_EQ_READ, &[1, 2, 3, 4, 5, 6, 7, 8]),
                    protocol::apply_equalizer,
                )
                .await
            }
            Request::SetGesture { gesture, value } => {
                let packet = protocol::gesture_write(model, &gesture, &value)?;
                self.link.request(packet).await?;
                model.snap.gestures = remembered_gesture(&model.snap.gestures, &gesture, &value);
                Ok(())
            }
            Request::SetLanguage(locale) => {
                self.link.request(protocol::language_write(&locale)).await?;
                model.snap.language = Some(locale);
                self.reread(
                    model,
                    Packet::read(CMD_LANGUAGE_READ, &[1, 2]),
                    protocol::apply_language,
                )
                .await
            }
            Request::SetDualEnabled(enabled) => {
                self.link
                    .request(protocol::dual_enabled_write(enabled))
                    .await?;
                self.reread(
                    model,
                    Packet::read(CMD_DUAL_ENABLED_READ, &[1]),
                    protocol::apply_dual_enabled,
                )
                .await?;
                self.enumerate_dual(model).await;
                Ok(())
            }
            Request::Dual {
                action,
                mac,
                enabled,
            } => {
                let packet = protocol::dual_execute(&action, &mac, enabled)?;
                self.link.send(&packet).await?;
                tokio::time::sleep(Duration::from_millis(400)).await;
                self.enumerate_dual(model).await;
                Ok(())
            }
        }
    }

    pub async fn wait_event(&mut self, model: &mut Model) -> Result<bool, String> {
        let battery = tokio::time::sleep(Duration::from_secs(60));
        tokio::pin!(battery);
        tokio::select! {
            packet = self.incoming.recv() => {
                let Some(packet) = packet else {
                    return Err("headset closed the SPP link".into());
                };
                if packet.command == CMD_DUAL_CHANGED {
                    self.enumerate_dual(model).await;
                    return Ok(true);
                }
                Ok(protocol::apply_notification(model, &packet))
            }
            () = &mut battery => {
                match self.link.request(protocol::battery_query()).await {
                    Ok(packet) => {
                        protocol::apply_battery(model, &packet);
                        Ok(true)
                    }
                    Err(err) => Err(err),
                }
            }
        }
    }

    async fn query_features(&mut self, model: &mut Model) {
        let packet = self
            .optional(model, Packet::read(CMD_AUTO_PAUSE_READ, &[1]), "auto-pause")
            .await;
        if let Some(packet) = packet {
            protocol::apply_auto_pause(model, &packet);
        }
        let packet = self
            .optional(model, Packet::read(CMD_DOUBLE_READ, &[1, 2]), "double tap")
            .await;
        if let Some(packet) = packet {
            protocol::apply_tap(model, &packet, TapKind::Double);
        }
        let packet = self
            .optional(model, Packet::read(CMD_TRIPLE_READ, &[1, 2]), "triple tap")
            .await;
        if let Some(packet) = packet {
            protocol::apply_tap(model, &packet, TapKind::Triple);
        }
        let packet = self
            .optional(model, Packet::read(CMD_LONG_READ, &[1, 2]), "long tap")
            .await;
        if let Some(packet) = packet {
            protocol::apply_long_tap(model, &packet);
        }
        let packet = self
            .optional(model, Packet::read(CMD_SWIPE_READ, &[1, 2]), "swipe")
            .await;
        if let Some(packet) = packet {
            protocol::apply_swipe(model, &packet);
        }
        let packet = self
            .optional(
                model,
                Packet::read(CMD_EQ_READ, &[1, 2, 3, 4, 5, 6, 7, 8]),
                "equalizer",
            )
            .await;
        if let Some(packet) = packet {
            protocol::apply_equalizer(model, &packet);
        }
        let packet = self
            .optional(model, Packet::read(CMD_QUALITY_READ, &[1]), "sound quality")
            .await;
        if let Some(packet) = packet {
            protocol::apply_quality(model, &packet);
        }
        let packet = self
            .optional(model, Packet::read(CMD_LATENCY, &[2]), "low latency")
            .await;
        if let Some(packet) = packet {
            protocol::apply_latency(model, &packet);
        }
        let packet = self
            .optional(
                model,
                Packet::read(CMD_LANGUAGE_READ, &[1, 2]),
                "voice language",
            )
            .await;
        if let Some(packet) = packet {
            protocol::apply_language(model, &packet);
        }
        let packet = self
            .optional(
                model,
                Packet::read(CMD_DUAL_ENABLED_READ, &[1]),
                "dual connect",
            )
            .await;
        if let Some(packet) = packet {
            protocol::apply_dual_enabled(model, &packet);
            self.enumerate_dual(model).await;
        }
        let packet = self
            .optional(
                model,
                Packet::read(CMD_NOISE_READ, &[1, 2]),
                "noise control",
            )
            .await;
        if let Some(packet) = packet {
            protocol::apply_noise(model, &packet);
        }
    }

    async fn reread(
        &mut self,
        model: &mut Model,
        packet: Packet,
        apply: fn(&mut Model, &Packet),
    ) -> Result<(), String> {
        let packet = self.optional(model, packet, "reread").await;
        if let Some(packet) = packet {
            apply(model, &packet);
        }
        Ok(())
    }

    async fn optional(&mut self, model: &mut Model, packet: Packet, label: &str) -> Option<Packet> {
        match self.link.request(packet).await {
            Ok(packet) => {
                self.drain(model);
                Some(packet)
            }
            Err(err) => {
                eprintln!("freeclipd: {label} unavailable: {err}");
                None
            }
        }
    }

    fn drain(&mut self, model: &mut Model) {
        while let Ok(packet) = self.incoming.try_recv() {
            if packet.command == CMD_DUAL_CHANGED {
                continue;
            }
            protocol::apply_notification(model, &packet);
        }
    }

    async fn enumerate_dual(&mut self, model: &mut Model) {
        let query = Packet::new(CMD_DUAL_ENUMERATE, vec![(1, Vec::new())]);
        if let Err(err) = self.link.send(&query).await {
            eprintln!("freeclipd: dual-device list unavailable: {err}");
            return;
        }

        let deadline = tokio::time::Instant::now() + Duration::from_millis(1200);
        let mut rows: Vec<(i16, protocol::Peer)> = Vec::new();
        let mut expected: Option<i16> = None;
        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let packet = match tokio::time::timeout(remaining, self.incoming.recv()).await {
                Ok(Some(packet)) => packet,
                _ => break,
            };
            if packet.command != CMD_DUAL_ENUMERATE {
                if packet.command != CMD_DUAL_CHANGED {
                    protocol::apply_notification(model, &packet);
                }
                continue;
            }
            let Some(DualRow {
                index,
                expected: count,
                peer,
            }) = protocol::parse_dual_row(&packet)
            else {
                continue;
            };
            expected = Some(count);
            if let Some(peer) = peer {
                rows.push((index, peer));
            }
            if expected == Some(0) || expected.is_some_and(|count| rows.len() as i16 >= count) {
                break;
            }
        }
        if expected.is_some() {
            rows.sort_by_key(|(index, _)| *index);
            protocol::finish_dual_list(model, rows.into_iter().map(|(_, peer)| peer).collect());
        }
    }
}

fn remembered_gesture(
    gestures: &protocol::Gestures,
    gesture: &str,
    value: &str,
) -> protocol::Gestures {
    let mut next = gestures.clone();
    let slot = match gesture {
        "double_tap_left" => &mut next.double_tap_left,
        "double_tap_right" => &mut next.double_tap_right,
        "triple_tap_left" => &mut next.triple_tap_left,
        "triple_tap_right" => &mut next.triple_tap_right,
        "long_tap" => &mut next.long_tap,
        "swipe" => &mut next.swipe,
        "noise_control" => &mut next.noise_control,
        _ => return next,
    };
    *slot = Some(value.to_string());
    next
}

async fn read_loop(
    mut read: tokio::io::ReadHalf<bluer::rfcomm::Stream>,
    link: Link,
    incoming: mpsc::Sender<Packet>,
) {
    loop {
        match spp::read_packet(&mut read).await {
            Ok(packet) => {
                spp::log_packet("rx", &packet);
                let waiter = lock_pending(&link.inner.pending).remove(&packet.command);
                if let Some(waiter) = waiter {
                    let _ = waiter.send(packet);
                    continue;
                }
                match incoming.try_send(packet) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => break,
                }
            }
            Err(err) => {
                eprintln!("freeclipd: SPP closed: {err}");
                lock_pending(&link.inner.pending).clear();
                break;
            }
        }
    }
}
