mod audio;
mod input;
mod supress;

use std::{
    env,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::{Context, Result};
use tracing::{info, trace};
use tracing_subscriber::EnvFilter;

use crate::{
    audio::{Audio, Sound},
    input::{InputDevice, InputEvent},
};

fn audio_dir() -> Result<PathBuf> {
    env::var_os("EVENTSFX_AUDIO_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .context("EVENTSFX_AUDIO_DIR not set")
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "eventsfx=trace".into()),
        )
        .init();

    info!("-- running v{} --", env!("CARGO_PKG_VERSION"));

    let audio_dir = audio_dir()?;
    let audio = Audio::new(&audio_dir)?;

    let suppressed = Arc::new(AtomicBool::new(false));
    supress::spawn(supress::Backend::Auto, Arc::clone(&suppressed));

    let mut input = InputDevice::new("seat0").context("failed to setup libinput.")?;

    input.run(|event| {
        if suppressed.load(Ordering::Relaxed) {
            return;
        }

        trace!(?event, "input");

        let sound = match event {
            InputEvent::Key { modifier: false } => Sound::Key,
            InputEvent::Key { modifier: true } => Sound::ModKey,
            InputEvent::Click => Sound::Click,
            InputEvent::Scroll => Sound::Scroll,
        };
        audio.play(sound);
    })
}
