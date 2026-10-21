use std::{
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use anyhow::Result;
use tracing::{debug, error, info, warn};

#[cfg(feature = "gnome")]
mod gnome;
mod hyprland;
mod none;

pub use none::NoSuppress;

pub trait Suppressor: Send {
    fn name(&self) -> &'static str;

    fn run(&mut self, suppressed: &AtomicBool) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    #[default]
    Auto,
    None,
    Hyprland,
    Gnome,
}

fn env_set(var: &str) -> bool {
    env::var_os(var).is_some_and(|v| !v.is_empty())
}

fn is_gnome() -> bool {
    env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|d| d.split(':').any(|part| part.eq_ignore_ascii_case("gnome")))
}

impl Backend {
    pub fn detect(self) -> Self {
        if self != Self::Auto {
            return self;
        }
        if env_set("EVENTFX_NO_BACKEND") {
            return Self::None;
        } else if env_set("HYPRLAND_INSTANCE_SIGNATURE") {
            Self::Hyprland
        } else if is_gnome() {
            Self::Gnome
        } else {
            Self::None
        }
    }

    fn connect(self) -> Result<Box<dyn Suppressor>, ConnectError> {
        match self {
            Self::Auto => self.detect().connect(),
            Self::None => Ok(Box::new(NoSuppress)),
            Self::Hyprland => Ok(Box::new(hyprland::Hyprland::connect()?)),
            #[cfg(feature = "gnome")]
            Self::Gnome => Ok(Box::new(gnome::Gnome::connect()?)),
            #[cfg(not(feature = "gnome"))]
            Self::Gnome => Err(ConnectError::Unsupported(
                "GNOME support was not enabled".into(),
            )),
            // _ => Err(ConnectError::Unsupported(format!(
            //     "the {self:?} backend is not implemented yet"
            // ))),
        }
    }
}

pub enum ConnectError {
    Unsupported(String),
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for ConnectError {
    fn from(e: anyhow::Error) -> Self {
        Self::Failed(e)
    }
}

pub fn spawn(choice: Backend, suppressed: Arc<AtomicBool>) {
    let choice = choice.detect();
    info!("supression backend: {choice:?}");

    let result = thread::Builder::new()
        .name("eventsfx-suppress".into())
        .spawn(move || supervise(choice, &suppressed));

    if let Err(e) = result {
        error!("could not start suppression thread: {e}; sounds will always play");
    }
}

fn supervise(choice: Backend, suppressed: &AtomicBool) {
    const MIN: Duration = Duration::from_secs(1);
    const MAX: Duration = Duration::from_secs(30);
    let mut backoff = MIN;

    loop {
        let err = match choice.connect() {
            Ok(mut backend) => {
                debug!("{} backend connected", backend.name());
                backoff = MIN;
                let result = backend.run(suppressed);
                suppressed.store(false, Ordering::Relaxed);
                match result {
                    Ok(()) => return,
                    Err(e) => e,
                }
            }
            Err(ConnectError::Unsupported(why)) => {
                tracing::warn!("{why}; sounds will always play");
                return;
            }
            Err(ConnectError::Failed(e)) => e,
        };

        warn!("suppression backend failed: {err:#}; retrying in {backoff:?}");
        thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX);
    }
}
