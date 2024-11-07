#![allow(clippy::bad_bit_mask)]

use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::os::unix::{fs::OpenOptionsExt, io::OwnedFd};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::thread::{sleep, spawn};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dirs::config_dir;
use input::event::{
    keyboard::{KeyState, KeyboardEvent::Key, KeyboardEventTrait},
    pointer::{
        Axis::{Horizontal, Vertical},
        ButtonState,
        PointerEvent::{Button, ScrollContinuous, ScrollWheel},
        PointerScrollEvent,
    },
    Event::{Keyboard, Pointer},
};
use input::{Libinput, LibinputInterface};
use libc::{O_RDONLY, O_RDWR, O_WRONLY};
use rodio::{source::Source, Decoder, OutputStream};

const NAME: Option<&str> = option_env!("CARGO_PKG_NAME");
const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

enum SessionType {
    X11,
    Hyprland,
}

use SessionType::*;

fn active_is_fullscreen(session: SessionType) -> bool {
    let command = match session {
        X11 => "xprop -id `xprop -root _NET_ACTIVE_WINDOW | grep -oP 'window id # \\K.*$'` | grep _NET_WM_STATE_FULLSCREEN >/dev/null",
        Hyprland => r#"[ `hyprctl activewindow -j | jq -r '.size | "\(.[0])x\(.[1])"'` != `hyprctl monitors -j | jq -r ".[] | select(.id == \`hyprctl activewindow -j | jq '.monitor'\`) | \"\(.width)x\(.height)\""` ]"#,
    };

    let status = Command::new("bash")
        .arg("-c")
        .arg(command)
        .status()
        .expect("failed to poll for active window's fullscreen state");

    status.success()
}

struct Interface;

impl LibinputInterface for Interface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        OpenOptions::new()
            .custom_flags(flags)
            .read((flags & O_RDONLY != 0) | (flags & O_RDWR != 0))
            .write((flags & O_WRONLY != 0) | (flags & O_RDWR != 0))
            .open(path)
            .map(|file| file.into())
            .map_err(|err| err.raw_os_error().expect("unable to open input device"))
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        drop(File::from(fd));
    }
}

fn audio_file_reader(file: &dyn AsRef<Path>) -> BufReader<File> {
    let file = file.as_ref();
    let mut path = config_dir().unwrap().join("eventsfx").join(file);

    if !path.exists() {
        path = match std::env::current_exe() {
            Ok(exe) => exe.parent().unwrap().parent().unwrap().join("share/eventsfx/audio").join(file),
            Err(_) => Path::new("audio").join(file),
        }
    }

    if !path.exists() {
        path = Path::new("/usr/share/eventsfx/audio").join(file);
    }

    if !path.exists() {
        panic!("unable to locate audio file: {}", &file.display()) 
    }

    BufReader::new(
        File::open(&path).unwrap_or_else(|_| panic!("unable to open {}", &path.display())),
    )
}

fn timestamp() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
}

fn main() {
    println!(
        "-- running {} {} --",
        NAME.unwrap_or("unknown"),
        VERSION.unwrap_or("unknown")
    );

    let playing = Arc::new(AtomicBool::new(true));
    let playing_clone = Arc::clone(&playing);

    spawn(move || loop {
        let start = timestamp();

        let playing = active_is_fullscreen(Hyprland);
        playing_clone.store(playing, Relaxed);

        let delta = timestamp() - start;
        sleep(Duration::from_millis(500) - delta);
    });

    let modkey_keys = [42, 29, 125, 56, 100, 97, 54, 46];

    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let file_click = audio_file_reader(&"click.flac");
    let file_scroll = audio_file_reader(&"scroll.flac");
    let file_key = audio_file_reader(&"key.flac");
    let file_modkey = audio_file_reader(&"modkey.flac");

    let source_click = Decoder::new(file_click).unwrap().buffered();
    let source_scroll = Decoder::new(file_scroll).unwrap().buffered();
    let source_key = Decoder::new(file_key).unwrap().buffered();
    let source_modkey = Decoder::new(file_modkey).unwrap().buffered();

    let mut input = Libinput::new_with_udev(Interface);
    input.udev_assign_seat("seat0").unwrap();

    let mut scroll_count = 0;

    loop {
        let start = timestamp();
        let is_playing = playing.load(Relaxed);

        input.dispatch().unwrap();
        for event in &mut input {
            if let Keyboard(Key(key)) = &event {
                if let KeyState::Pressed = key.key_state() {
                    println!("playing: {}, key: {}", is_playing, key.key());

                    if is_playing {
                        if modkey_keys.contains(&key.key()) {
                            stream_handle
                                .play_raw(source_modkey.clone().convert_samples())
                                .unwrap();
                        } else {
                            stream_handle
                                .play_raw(source_key.clone().convert_samples())
                                .unwrap();
                        }
                    }
                }
            } else if let Pointer(Button(button)) = &event {
                if let ButtonState::Pressed = button.button_state() {
                    println!("button: {}", button.button());

                    if is_playing {
                        stream_handle
                            .play_raw(source_click.clone().convert_samples())
                            .unwrap();
                    }
                }
            } else if let Pointer(ScrollContinuous(scroll)) = &event {
                scroll_count += 1;

                if scroll_count >= 32 {
                    let mut v = 0.;
                    let mut h = 0.;

                    if scroll.has_axis(Vertical) {
                        v = scroll.scroll_value(Vertical);
                    }

                    if scroll.has_axis(Horizontal) {
                        h = scroll.scroll_value(Horizontal);
                    }

                    println!("scroll: v={}, h={}", v, h);
                    stream_handle
                        .play_raw(source_scroll.clone().convert_samples())
                        .unwrap();

                    scroll_count = 0;
                }
            } else if let Pointer(ScrollWheel(scroll)) = &event {
                println!("scroll: {:#?}", scroll);

                if is_playing {
                    stream_handle
                        .play_raw(source_scroll.clone().convert_samples())
                        .unwrap();
                }
            } else {
                // println!("event: {:?}", event);
            }
        }

        let delta = timestamp() - start;
        sleep(Duration::from_millis(5) - delta);
    }
}
