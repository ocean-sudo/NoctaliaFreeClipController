//! FreeClip 2 helper for the Noctalia plugin.
//!
//! `serve` watches BlueZ and, once the headset is already connected, opens the
//! vendor RFCOMM channel. State is printed to stdout as one JSON object per
//! line. `ctl` sends one JSON command to the running daemon.

mod discover;
mod protocol;
mod session;
mod spp;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::{Path, PathBuf};
use std::process;
use std::str::FromStr;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot, watch};

use crate::discover::FoundDevice;
use crate::protocol::{Model, Request, Snapshot};

const REQUEST_LIMIT: usize = 8192;

struct Job {
    line: String,
    reply: oneshot::Sender<String>,
}

struct ServeOptions {
    address: Option<bluer::Address>,
    socket: Option<PathBuf>,
    channel: u8,
}

struct CtlOptions {
    socket: Option<PathBuf>,
    json: String,
}

enum Command {
    Help,
    Version,
    Serve(ServeOptions),
    Ctl(CtlOptions),
}

#[derive(PartialEq, Eq)]
enum Step {
    Continue,
    Rescan,
    DropLink,
    Stop,
}

struct Cleanup {
    socket: PathBuf,
    pid: PathBuf,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.pid);
        let _ = fs::remove_file(&self.socket);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let code = match parse_args(&args) {
        Ok(Command::Help) => {
            print_help();
            0
        }
        Ok(Command::Version) => {
            println!("freeclipd {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok(Command::Ctl(options)) => ctl(&options),
        Ok(Command::Serve(options)) => match runtime() {
            Ok(runtime) => runtime.block_on(serve(options)),
            Err(err) => {
                eprintln!("freeclipd: {err}");
                1
            }
        },
        Err(err) => {
            eprintln!("freeclipd: {err}");
            eprintln!("Run `freeclipd --help` for usage.");
            2
        }
    };
    process::exit(code);
}

fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
}

fn print_help() {
    println!(
        "\
freeclipd {version}
Lightweight HUAWEI FreeClip 2 SPP daemon for Noctalia.

Usage:
  freeclipd serve [--address MAC] [--socket PATH] [--channel N]
  freeclipd ctl [--socket PATH] <json>
  freeclipd --help
  freeclipd --version

`serve` prints state JSON to stdout and logs to stderr. It opens RFCOMM only
after BlueZ already shows the headset connected. Pairing and audio routing stay
with the desktop. `ctl` sends one command and prints the acknowledgement.

The control socket defaults to $XDG_RUNTIME_DIR/noctalia-freeclip.sock.
",
        version = env!("CARGO_PKG_VERSION")
    );
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    if args.len() <= 1 {
        return Ok(Command::Help);
    }
    match args[1].as_str() {
        "-h" | "--help" | "help" => Ok(Command::Help),
        "-V" | "--version" | "version" => Ok(Command::Version),
        "serve" => parse_serve(&args[2..]).map(Command::Serve),
        "ctl" => parse_ctl(&args[2..]).map(Command::Ctl),
        other => Err(format!("unknown command '{other}'")),
    }
}

fn parse_serve(args: &[String]) -> Result<ServeOptions, String> {
    let mut options = ServeOptions {
        address: None,
        socket: None,
        channel: 1,
    };
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--address" => {
                options.address = Some(parse_address(&take_value(args, &mut index, "--address")?)?);
            }
            "--socket" => {
                options.socket = Some(PathBuf::from(take_value(args, &mut index, "--socket")?))
            }
            "--channel" => {
                options.channel = parse_channel(&take_value(args, &mut index, "--channel")?)?
            }
            _ if let Some(value) = arg.strip_prefix("--address=") => {
                options.address = Some(parse_address(value)?)
            }
            _ if let Some(value) = arg.strip_prefix("--socket=") => {
                options.socket = Some(PathBuf::from(value))
            }
            _ if let Some(value) = arg.strip_prefix("--channel=") => {
                options.channel = parse_channel(value)?
            }
            "--" => {
                if index + 1 != args.len() {
                    return Err("serve does not take a positional argument".into());
                }
            }
            other => return Err(format!("unknown serve option '{other}'")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_ctl(args: &[String]) -> Result<CtlOptions, String> {
    let mut socket = None;
    let mut json = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--socket" => socket = Some(PathBuf::from(take_value(args, &mut index, "--socket")?)),
            _ if let Some(value) = arg.strip_prefix("--socket=") => {
                socket = Some(PathBuf::from(value))
            }
            "--" => {}
            other if other.starts_with('-') => return Err(format!("unknown ctl option '{other}'")),
            other => {
                if json.is_some() {
                    return Err("ctl accepts one JSON command".into());
                }
                json = Some(other.to_string());
            }
        }
        index += 1;
    }
    Ok(CtlOptions {
        socket,
        json: json.ok_or("ctl requires a JSON command")?,
    })
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_channel(value: &str) -> Result<u8, String> {
    let channel: u8 = value
        .parse()
        .map_err(|_| format!("channel '{value}' is not a number"))?;
    if (1..=30).contains(&channel) {
        Ok(channel)
    } else {
        Err("RFCOMM channel must be from 1 to 30".into())
    }
}

fn parse_address(value: &str) -> Result<bluer::Address, String> {
    bluer::Address::from_str(value).map_err(|_| format!("'{value}' is not a bluetooth address"))
}

fn socket_from(explicit: Option<PathBuf>, runtime_dir: Option<&OsStr>, uid: u32) -> PathBuf {
    if let Some(path) = explicit {
        return path;
    }
    if let Some(dir) = runtime_dir {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("noctalia-freeclip.sock");
        }
    }
    PathBuf::from(format!("/tmp/noctalia-freeclip-{uid}.sock"))
}

fn ctl(options: &CtlOptions) -> i32 {
    let socket = socket_from(
        options.socket.clone(),
        env::var_os("XDG_RUNTIME_DIR").as_deref(),
        user_id(),
    );
    let mut stream = match StdUnixStream::connect(&socket) {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!("freeclipd: cannot connect to {}: {err}", socket.display());
            return 1;
        }
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(40)));
    if writeln!(stream, "{}", options.json).is_err() || stream.flush().is_err() {
        eprintln!("freeclipd: failed to send the command");
        return 1;
    }
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() || line.trim().is_empty() {
        eprintln!("freeclipd: the daemon did not answer");
        return 1;
    }
    print!("{line}");
    if !line.ends_with('\n') {
        println!();
    }
    match serde_json::from_str::<Value>(line.trim()) {
        Ok(value) if value.get("ok").and_then(Value::as_bool) == Some(true) => 0,
        _ => 1,
    }
}

async fn serve(options: ServeOptions) -> i32 {
    let socket = socket_from(
        options.socket.clone(),
        env::var_os("XDG_RUNTIME_DIR").as_deref(),
        user_id(),
    );
    let pid_path = pid_path_for(&socket);
    takeover(&pid_path).await;
    if let Err(err) = prepare_socket(&socket) {
        eprintln!("freeclipd: {err}");
        return 1;
    }
    let listener = match UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("freeclipd: cannot listen on {}: {err}", socket.display());
            return 1;
        }
    };
    if let Err(err) = fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)) {
        eprintln!("freeclipd: cannot restrict {}: {err}", socket.display());
        let _ = fs::remove_file(&socket);
        return 1;
    }
    if let Err(err) = write_pid(&pid_path) {
        eprintln!("freeclipd: {err}");
        let _ = fs::remove_file(&socket);
        return 1;
    }
    let _cleanup = Cleanup {
        socket: socket.clone(),
        pid: pid_path,
    };

    let (jobs_tx, mut jobs) = mpsc::channel(8);
    tokio::spawn(async move { accept_loop(listener, jobs_tx).await });
    let (shutdown_tx, mut shutdown) = watch::channel(false);
    tokio::spawn(async move {
        if let Err(err) = wait_for_signal().await {
            eprintln!("freeclipd: {err}");
        }
        let _ = shutdown_tx.send(true);
    });

    let mut model = Model::new();
    let mut published = None;
    if !publish(&mut published, &mut model) {
        return 0;
    }

    let mut failures: u32 = 0;
    loop {
        if stopped(&shutdown) {
            break;
        }
        let session =
            match tokio::time::timeout(Duration::from_secs(8), bluer::Session::new()).await {
                Ok(Ok(session)) => session,
                Ok(Err(err)) => {
                    fail_phase(&mut model, "unavailable", format!("bluetooth: {err}"));
                    if !publish(&mut published, &mut model) {
                        break;
                    }
                    if idle(
                        Duration::from_secs(2),
                        &mut jobs,
                        &mut shutdown,
                        &mut model,
                        &mut published,
                    )
                    .await
                        == Step::Stop
                    {
                        break;
                    }
                    continue;
                }
                Err(_) => {
                    fail_phase(&mut model, "unavailable", "bluetooth bus timed out".into());
                    if !publish(&mut published, &mut model) {
                        break;
                    }
                    if idle(
                        Duration::from_secs(2),
                        &mut jobs,
                        &mut shutdown,
                        &mut model,
                        &mut published,
                    )
                    .await
                        == Step::Stop
                    {
                        break;
                    }
                    continue;
                }
            };
        let adapter =
            match tokio::time::timeout(Duration::from_secs(8), session.default_adapter()).await {
                Ok(Ok(adapter)) => adapter,
                Ok(Err(err)) => {
                    fail_phase(
                        &mut model,
                        "unavailable",
                        format!("bluetooth adapter: {err}"),
                    );
                    if !publish(&mut published, &mut model)
                        || idle_or_stop(
                            Duration::from_secs(2),
                            &mut jobs,
                            &mut shutdown,
                            &mut model,
                            &mut published,
                        )
                        .await
                    {
                        break;
                    }
                    continue;
                }
                Err(_) => {
                    fail_phase(
                        &mut model,
                        "unavailable",
                        "bluetooth adapter timed out".into(),
                    );
                    if !publish(&mut published, &mut model)
                        || idle_or_stop(
                            Duration::from_secs(2),
                            &mut jobs,
                            &mut shutdown,
                            &mut model,
                            &mut published,
                        )
                        .await
                    {
                        break;
                    }
                    continue;
                }
            };

        loop {
            match adapter.is_powered().await {
                Ok(true) => {}
                Ok(false) => {
                    fail_phase(
                        &mut model,
                        "unavailable",
                        "bluetooth adapter is powered off".into(),
                    );
                    if !publish(&mut published, &mut model)
                        || idle_or_stop(
                            Duration::from_secs(2),
                            &mut jobs,
                            &mut shutdown,
                            &mut model,
                            &mut published,
                        )
                        .await
                    {
                        return 0;
                    }
                    continue;
                }
                Err(err) => {
                    fail_phase(
                        &mut model,
                        "unavailable",
                        format!("bluetooth adapter: {err}"),
                    );
                    if !publish(&mut published, &mut model)
                        || idle_or_stop(
                            Duration::from_secs(2),
                            &mut jobs,
                            &mut shutdown,
                            &mut model,
                            &mut published,
                        )
                        .await
                    {
                        return 0;
                    }
                    break;
                }
            }

            let found = match discover::find_device(&adapter, options.address).await {
                Ok(found) => found,
                Err(err) => {
                    fail_phase(&mut model, "unavailable", err);
                    if !publish(&mut published, &mut model)
                        || idle_or_stop(
                            Duration::from_secs(2),
                            &mut jobs,
                            &mut shutdown,
                            &mut model,
                            &mut published,
                        )
                        .await
                    {
                        return 0;
                    }
                    break;
                }
            };

            let Some(device) = found else {
                note_missing(&mut model, options.address);
                if !publish(&mut published, &mut model) {
                    return 0;
                }
                match idle(
                    Duration::from_secs(2),
                    &mut jobs,
                    &mut shutdown,
                    &mut model,
                    &mut published,
                )
                .await
                {
                    Step::Stop => return 0,
                    Step::Rescan | Step::Continue | Step::DropLink => continue,
                }
            };

            remember_device(&mut model, &device);
            if !device.connected {
                model.snap.spp_connected = false;
                model.snap.audio_connected = false;
                set_phase(&mut model, "paired", None);
                if !publish(&mut published, &mut model) {
                    return 0;
                }
                match idle(
                    Duration::from_secs(2),
                    &mut jobs,
                    &mut shutdown,
                    &mut model,
                    &mut published,
                )
                .await
                {
                    Step::Stop => return 0,
                    Step::Rescan | Step::Continue | Step::DropLink => continue,
                }
            }

            model.snap.audio_connected = true;
            model.snap.spp_connected = false;
            set_phase(&mut model, "connecting", None);
            if !publish(&mut published, &mut model) {
                return 0;
            }

            match session::connect(device.address, options.channel).await {
                Err(err) => {
                    failures = failures.saturating_add(1);
                    fail_phase(&mut model, "reconnecting", format!("SPP: {err}"));
                    if !publish(&mut published, &mut model) {
                        return 0;
                    }
                    if idle_or_stop(
                        backoff(failures),
                        &mut jobs,
                        &mut shutdown,
                        &mut model,
                        &mut published,
                    )
                    .await
                    {
                        return 0;
                    }
                }
                Ok(stream) => {
                    let mut live = session::LiveSession::start(stream);
                    model.snap.spp_connected = true;
                    let pulled = live.pull(&mut model).await;
                    match pulled {
                        Err(err) => {
                            drop(live);
                            failures = failures.saturating_add(1);
                            model.snap.spp_connected = false;
                            fail_phase(&mut model, "reconnecting", err);
                            if !publish(&mut published, &mut model) {
                                return 0;
                            }
                            if idle_or_stop(
                                backoff(failures),
                                &mut jobs,
                                &mut shutdown,
                                &mut model,
                                &mut published,
                            )
                            .await
                            {
                                return 0;
                            }
                        }
                        Ok(()) => {
                            warn_if_unexpected_model(&model);
                            failures = 0;
                            model.snap.spp_connected = true;
                            model.snap.audio_connected = true;
                            set_phase(&mut model, "ready", None);
                            if !publish(&mut published, &mut model) {
                                return 0;
                            }
                            match run_ready(
                                &mut live,
                                &mut jobs,
                                &mut shutdown,
                                &mut model,
                                &mut published,
                            )
                            .await
                            {
                                Step::Stop => return 0,
                                Step::DropLink | Step::Rescan | Step::Continue => {
                                    drop(live);
                                    failures = failures.saturating_add(1);
                                    model.snap.spp_connected = false;
                                    if model.snap.phase != "reconnecting" {
                                        let error = model.snap.error.clone();
                                        set_phase(&mut model, "reconnecting", error);
                                    }
                                    if !publish(&mut published, &mut model) {
                                        return 0;
                                    }
                                    if idle_or_stop(
                                        backoff(failures),
                                        &mut jobs,
                                        &mut shutdown,
                                        &mut model,
                                        &mut published,
                                    )
                                    .await
                                    {
                                        return 0;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    0
}

async fn run_ready(
    live: &mut session::LiveSession,
    jobs: &mut mpsc::Receiver<Job>,
    shutdown: &mut watch::Receiver<bool>,
    model: &mut Model,
    published: &mut Option<Snapshot>,
) -> Step {
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Step::Stop;
                }
            }
            job = jobs.recv() => {
                let Some(job) = job else { return Step::Stop };
                match handle_online(live, model, published, job).await {
                    Step::Continue => {}
                    other => return other,
                }
            }
            event = live.wait_event(model) => {
                match event {
                    Ok(_) => {
                        if !publish(published, model) {
                            return Step::Stop;
                        }
                    }
                    Err(err) => {
                        model.snap.spp_connected = false;
                        fail_phase(model, "reconnecting", err);
                        let _ = publish(published, model);
                        return Step::DropLink;
                    }
                }
            }
        }
    }
}

async fn handle_online(
    live: &mut session::LiveSession,
    model: &mut Model,
    published: &mut Option<Snapshot>,
    job: Job,
) -> Step {
    let request = match parse_job(&job.line) {
        Ok(request) => request,
        Err(err) => {
            answer(job, false, Some(&err), model);
            return Step::Continue;
        }
    };
    if matches!(request, Request::Get) {
        answer(job, true, None, model);
        return Step::Continue;
    }
    match live.handle(model, request).await {
        Ok(()) => {
            if !publish(published, model) {
                answer(job, false, Some("the plugin stopped reading state"), model);
                return Step::Stop;
            }
            answer(job, true, None, model);
            Step::Continue
        }
        Err(err) => {
            let dead = link_is_dead(&err);
            if dead {
                model.snap.spp_connected = false;
                fail_phase(model, "reconnecting", err.clone());
                if !publish(published, model) {
                    answer(job, false, Some(&err), model);
                    return Step::Stop;
                }
            }
            answer(job, false, Some(&err), model);
            if dead {
                Step::DropLink
            } else {
                Step::Continue
            }
        }
    }
}

async fn idle(
    duration: Duration,
    jobs: &mut mpsc::Receiver<Job>,
    shutdown: &mut watch::Receiver<bool>,
    model: &mut Model,
    published: &mut Option<Snapshot>,
) -> Step {
    let sleep = tokio::time::sleep(duration);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut sleep => return Step::Continue,
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return Step::Stop;
                }
            }
            job = jobs.recv() => {
                let Some(job) = job else { return Step::Stop };
                match handle_offline(model, published, job).await {
                    Step::Continue => {}
                    other => return other,
                }
            }
        }
    }
}

async fn idle_or_stop(
    duration: Duration,
    jobs: &mut mpsc::Receiver<Job>,
    shutdown: &mut watch::Receiver<bool>,
    model: &mut Model,
    published: &mut Option<Snapshot>,
) -> bool {
    matches!(
        idle(duration, jobs, shutdown, model, published).await,
        Step::Stop
    )
}

async fn handle_offline(model: &mut Model, published: &mut Option<Snapshot>, job: Job) -> Step {
    let request = match parse_job(&job.line) {
        Ok(request) => request,
        Err(err) => {
            answer(job, false, Some(&err), model);
            return Step::Continue;
        }
    };
    match request {
        Request::Get => {
            answer(job, true, None, model);
            Step::Continue
        }
        Request::Refresh => {
            if !publish(published, model) {
                answer(job, false, Some("the plugin stopped reading state"), model);
                return Step::Stop;
            }
            answer(job, true, None, model);
            Step::Rescan
        }
        _ => {
            answer(job, false, Some("FreeClip 2 is not connected"), model);
            Step::Continue
        }
    }
}

fn parse_job(line: &str) -> Result<Request, String> {
    let value: Value = serde_json::from_str(line).map_err(|err| format!("invalid json: {err}"))?;
    protocol::parse_request(&value)
}

fn answer(job: Job, ok: bool, error: Option<&str>, model: &Model) {
    let line = serde_json::to_string(&protocol::ack(ok, error, &model.snap)).unwrap_or_else(|_| {
        "{\"type\":\"ack\",\"ok\":false,\"error\":\"failed to encode the reply\"}".into()
    });
    let _ = job.reply.send(line);
}

fn publish(published: &mut Option<Snapshot>, model: &mut Model) -> bool {
    if published
        .as_ref()
        .is_some_and(|previous| protocol::same_state(previous, &model.snap))
    {
        if let Some(previous) = published {
            model.snap.seq = previous.seq;
        }
        return true;
    }
    let phase_changed = published.as_ref().is_none_or(|previous| {
        previous.phase != model.snap.phase || previous.error != model.snap.error
    });
    model.snap.seq = published
        .as_ref()
        .map(|previous| previous.seq.saturating_add(1))
        .unwrap_or(1);
    if let Err(err) = emit(&model.snap) {
        if err.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("freeclipd: stdout: {err}");
        }
        return false;
    }
    if phase_changed {
        match &model.snap.error {
            Some(error) => eprintln!("freeclipd: {} ({error})", model.snap.phase),
            None => eprintln!("freeclipd: {}", model.snap.phase),
        }
    }
    *published = Some(model.snap.clone());
    true
}

fn emit(snapshot: &Snapshot) -> io::Result<()> {
    let line = serde_json::to_string(snapshot)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let mut out = io::stdout().lock();
    writeln!(out, "{line}")?;
    out.flush()
}

fn set_phase(model: &mut Model, phase: &str, error: Option<String>) {
    model.snap.phase = phase.to_string();
    model.snap.error = error;
}

fn fail_phase(model: &mut Model, phase: &str, error: String) {
    model.snap.spp_connected = false;
    set_phase(model, phase, Some(error));
}

fn note_missing(model: &mut Model, wanted: Option<bluer::Address>) {
    model.snap.phase = "searching".into();
    model.snap.spp_connected = false;
    model.snap.audio_connected = false;
    model.snap.paired = false;
    model.snap.error = None;
    model.snap.name = None;
    model.snap.address = wanted.map(|address| address.to_string());
}

fn remember_device(model: &mut Model, device: &FoundDevice) {
    let address = device.address.to_string();
    if model.snap.address.as_deref() != Some(address.as_str()) {
        let seq = model.snap.seq;
        *model = Model::new();
        model.snap.seq = seq;
    }
    model.snap.address = Some(address);
    model.snap.name = Some(if device.label.is_empty() {
        device.address.to_string()
    } else {
        device.label.clone()
    });
    model.snap.paired = device.paired;
    model.snap.audio_connected = device.connected;
}

fn warn_if_unexpected_model(model: &Model) {
    let mut names = Vec::new();
    if let Some(name) = &model.snap.info.model {
        names.push(name.clone());
    }
    if let Some(name) = &model.snap.info.submodel {
        names.push(name.clone());
    }
    if names.is_empty() {
        return;
    }
    let folded = names.join(" ").to_ascii_lowercase();
    if !folded.contains("btft0027") && !folded.contains("freeclip") {
        eprintln!(
            "freeclipd: device reports model {}, expected FreeClip 2 (BTFT0027)",
            names.join(" / ")
        );
    }
}

fn link_is_dead(err: &str) -> bool {
    let folded = err.to_ascii_lowercase();
    folded.contains("closed")
        || folded.contains("broken pipe")
        || folded.contains("connection reset")
        || folded.contains("not connected")
}

fn backoff(failures: u32) -> Duration {
    Duration::from_secs(5 * u64::from(failures.clamp(1, 3)))
}

fn stopped(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow()
}

async fn wait_for_signal() -> io::Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
    Ok(())
}

async fn accept_loop(listener: UnixListener, jobs: mpsc::Sender<Job>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let jobs = jobs.clone();
                tokio::spawn(async move {
                    if let Err(err) = serve_client(stream, jobs).await {
                        eprintln!("freeclipd: control client: {err}");
                    }
                });
            }
            Err(err) => {
                eprintln!("freeclipd: control socket: {err}");
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

enum ClientRead {
    Line(String),
    Reject(&'static str),
    Failed(io::Error),
}

async fn serve_client(
    mut stream: tokio::net::UnixStream,
    jobs: mpsc::Sender<Job>,
) -> io::Result<()> {
    let read = {
        let mut reader = AsyncBufReader::new(&mut stream);
        let mut buf = Vec::new();
        let read =
            tokio::time::timeout(Duration::from_secs(2), reader.read_until(b'\n', &mut buf)).await;
        match read {
            Ok(Ok(0)) => ClientRead::Reject("empty command"),
            Ok(Ok(_)) => {
                while matches!(buf.last(), Some(b'\n' | b'\r')) {
                    buf.pop();
                }
                if buf.len() > REQUEST_LIMIT {
                    ClientRead::Reject("command is too long")
                } else {
                    match String::from_utf8(buf) {
                        Ok(line) if !line.is_empty() => ClientRead::Line(line),
                        _ => ClientRead::Reject("command is not utf-8"),
                    }
                }
            }
            Ok(Err(err)) => ClientRead::Failed(err),
            Err(_) => ClientRead::Reject("command timed out"),
        }
    };
    let line = match read {
        ClientRead::Line(line) => line,
        ClientRead::Failed(err) => return Err(err),
        ClientRead::Reject(reason) => {
            let body = format!("{{\"type\":\"ack\",\"ok\":false,\"error\":\"{reason}\"}}");
            return write_text(&mut stream, &body).await;
        }
    };

    let (tx, rx) = oneshot::channel();
    if jobs.send(Job { line, reply: tx }).await.is_err() {
        return write_text(
            &mut stream,
            "{\"type\":\"ack\",\"ok\":false,\"error\":\"daemon stopped\"}",
        )
        .await;
    }
    let response = match tokio::time::timeout(Duration::from_secs(40), rx).await {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => "{\"type\":\"ack\",\"ok\":false,\"error\":\"daemon stopped\"}".into(),
        Err(_) => "{\"type\":\"ack\",\"ok\":false,\"error\":\"daemon did not answer\"}".into(),
    };
    write_text(&mut stream, &response).await
}

async fn write_text(stream: &mut tokio::net::UnixStream, body: &str) -> io::Result<()> {
    stream.write_all(body.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await
}

fn prepare_socket(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(format!(
                "socket directory {} does not exist",
                parent.display()
            ));
        }
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("cannot replace {}: {err}", path.display())),
    }
}

fn pid_path_for(socket: &Path) -> PathBuf {
    let mut path = socket.as_os_str().to_os_string();
    path.push(".pid");
    PathBuf::from(path)
}

fn write_pid(path: &Path) -> Result<(), String> {
    fs::write(path, format!("{}\n", process::id()))
        .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|err| format!("cannot restrict {}: {err}", path.display()))
}

async fn takeover(pid_path: &Path) {
    let Ok(text) = fs::read_to_string(pid_path) else {
        return;
    };
    let Ok(pid) = text.trim().parse::<i32>() else {
        return;
    };
    if pid <= 0 || pid == process::id() as i32 || !is_freeclipd(pid) {
        return;
    }
    eprintln!("freeclipd: stopping previous daemon (pid {pid})");
    signal_pid(pid, libc::SIGTERM);
    for _ in 0..20 {
        if !process_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    signal_pid(pid, libc::SIGKILL);
    tokio::time::sleep(Duration::from_millis(100)).await;
}

fn is_freeclipd(pid: i32) -> bool {
    fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .is_some_and(|bytes| String::from_utf8_lossy(&bytes).contains("freeclipd"))
}

fn process_alive(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn signal_pid(pid: i32, signal: i32) {
    unsafe {
        libc::kill(pid, signal);
    }
}

fn user_id() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn serve_parses_address_channel_and_socket() {
        let command = parse_args(&args(&[
            "freeclipd",
            "serve",
            "--address",
            "aa:bb:cc:dd:ee:ff",
            "--channel=3",
            "--socket",
            "/tmp/freeclip-test.sock",
        ]))
        .unwrap();
        match command {
            Command::Serve(options) => {
                assert_eq!(
                    options.address.unwrap().to_string().to_ascii_lowercase(),
                    "aa:bb:cc:dd:ee:ff"
                );
                assert_eq!(options.channel, 3);
                assert_eq!(
                    options.socket.unwrap(),
                    PathBuf::from("/tmp/freeclip-test.sock")
                );
            }
            _ => panic!("expected serve"),
        }
    }

    #[test]
    fn ctl_requires_one_json_command() {
        let command = parse_args(&args(&["freeclipd", "ctl", "{\"cmd\":\"get\"}"])).unwrap();
        match command {
            Command::Ctl(options) => assert_eq!(options.json, "{\"cmd\":\"get\"}"),
            _ => panic!("expected ctl"),
        }
        assert!(parse_args(&args(&["freeclipd", "ctl"])).is_err());
        assert!(parse_args(&args(&["freeclipd", "serve", "--channel", "0"])).is_err());
    }

    #[test]
    fn socket_uses_runtime_dir_then_tmp() {
        let runtime = socket_from(None, Some(OsStr::new("/run/user/1000")), 1000);
        assert_eq!(
            runtime,
            PathBuf::from("/run/user/1000/noctalia-freeclip.sock")
        );
        let fallback = socket_from(None, None, 1000);
        assert_eq!(fallback, PathBuf::from("/tmp/noctalia-freeclip-1000.sock"));
        let explicit = socket_from(
            Some(PathBuf::from("/tmp/chosen.sock")),
            Some(OsStr::new("/run/user/1")),
            1,
        );
        assert_eq!(explicit, PathBuf::from("/tmp/chosen.sock"));
    }
}
