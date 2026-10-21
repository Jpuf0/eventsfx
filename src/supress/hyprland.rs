use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tracing::info;

use crate::supress::{ConnectError, Suppressor};

const EVENT_SOCKET: &str = ".socket2.sock";
const REQUEST_SOCKET: &str = ".socket.sock";

const RELEVANT: &[&str] = &[
    "fullscreen",
    "activewindowv2",
    "closewindow",
    "workspacev2",
    "focusedmonv2",
    "movewindowv2",
];

pub struct Hyprland {
    events: BufReader<UnixStream>,
    request_socket: PathBuf,
}

impl Hyprland {
    pub fn connect() -> Result<Self, ConnectError> {
        let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ConnectError::Unsupported("HYPRLAND_INSTANCE_SIGNATURE is not set".into())
            })?;

        let mut candidates = Vec::new();
        if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
            candidates.push(Path::new(&rt).join("hypr").join(&sig));
        }
        candidates.push(Path::new("/tmp/hypr").join(&sig));

        let dir = candidates
            .iter()
            .find(|d| d.join(EVENT_SOCKET).exists())
            .ok_or_else(|| anyhow::anyhow!("no Hyprland sockets found for instance {sig}"))?;

        Ok(Self::from_dir(dir)?)
    }

    fn from_dir(dir: &Path) -> Result<Self> {
        let path = dir.join(EVENT_SOCKET);
        let events = UnixStream::connect(&path)
            .with_context(|| format!("cannot connect to {}", path.display()))?;

        Ok(Self {
            events: BufReader::new(events),
            request_socket: dir.join(REQUEST_SOCKET),
        })
    }

    fn query_fullscreen(&self) -> Result<bool> {
        let mut sock = UnixStream::connect(&self.request_socket)
            .with_context(|| format!("cannot connect to {}", self.request_socket.display()))?;
        sock.set_read_timeout(Some(Duration::from_secs(2)))?;

        sock.write_all(b"j/activewindow")?;
        let mut reply = String::new();
        sock.read_to_string(&mut reply)
            .context("reading activewindow reply")?;

        parse_active_window(&reply)
    }
}

fn parse_active_window(json: &str) -> Result<bool> {
    let val: Value = serde_json::from_str(json).context("activewindow reply is not json")?;
    Ok(match val.get("fullscreen") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_u64().is_some_and(|n| n & 2 != 0),
        Some(other) => bail!("unexpected fullscreen value: {other}"),
    })
}

impl Suppressor for Hyprland {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn run(&mut self, suppressed: &AtomicBool) -> Result<()> {
        let update = |this: &Self, last: &mut Option<bool>| -> Result<()> {
            let now = this.query_fullscreen()?;
            if *last != Some(now) {
                info!(suppressed = now, "fullscreen state changed");
                suppressed.store(now, Ordering::Relaxed);
                *last = Some(now);
            }
            Ok(())
        };

        let mut last = None;
        update(self, &mut last)?;

        let mut line = String::new();
        loop {
            line.clear();
            if self.events.read_line(&mut line)? == 0 {
                bail!("hyprland event socket closed");
            }
            let name = line.split_once(">>").map_or(line.trim_end(), |(n, _)| n);
            if RELEVANT.contains(&name) {
                update(self, &mut last)?;
            }
        }
    }
}
