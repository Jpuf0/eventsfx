use std::{fs::File, io::BufReader, path::Path};

use anyhow::{anyhow, Context, Ok, Result};
use rodio::{source::Buffered, Decoder, DeviceSinkBuilder, MixerDeviceSink, Source};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Key,
    ModKey,
    Click,
    Scroll,
}

type SoundClip = Buffered<Decoder<BufReader<File>>>;

pub struct Audio {
    sink: MixerDeviceSink,
    key: Option<SoundClip>,
    modkey: Option<SoundClip>,
    click: Option<SoundClip>,
    scroll: Option<SoundClip>,
}

impl Audio {
    pub fn new(dir: &Path) -> Result<Self> {
        let mut sink = DeviceSinkBuilder::open_default_sink()
            .map_err(|e| anyhow!("cannot open audio output: {e}"))?;
        sink.log_on_drop(false);

        let load = |name: &str| {
            let path = dir.join(name);
            if !path.exists() {
                info!("{} not found", path.display());
                return None;
            }
            load(&path).inspect_err(|e| warn!("{e:#}")).ok()
        };

        let audio = Self {
            sink,
            key: load("key.flac"),
            modkey: load("modkey.flac"),
            click: load("click.flac"),
            scroll: load("scroll.flac"),
        };

        if [&audio.key, &audio.modkey, &audio.click, &audio.scroll]
            .iter()
            .all(|c| c.is_none())
        {
            anyhow::bail!("no sounds found in {}", dir.display());
        }

        Ok(audio)
    }

    pub fn play(&self, sound: Sound) {
        let soundclip = match sound {
            Sound::Key => self.key.as_ref(),
            Sound::ModKey => self.modkey.as_ref().or(self.key.as_ref()),
            Sound::Click => self.click.as_ref(),
            Sound::Scroll => self.scroll.as_ref(),
        };

        if let Some(soundclip) = soundclip {
            self.sink.mixer().add(soundclip.clone());
        }
    }
}

fn load(path: &Path) -> Result<SoundClip> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let decoder = Decoder::try_from(BufReader::new(file))
        .with_context(|| format!("cannot decode {}", path.display()))?;

    Ok(decoder.buffered())
}
