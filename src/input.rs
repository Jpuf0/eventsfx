use std::{
    fs::{File, OpenOptions},
    os::{fd::OwnedFd, unix::fs::OpenOptionsExt},
    path::Path,
};

use anyhow::{anyhow, Context, Result};
use input::{
    event::{
        keyboard::{KeyState, KeyboardEventTrait},
        pointer::ButtonState,
        KeyboardEvent, PointerEvent,
    },
    Event, Libinput, LibinputInterface,
};
use libc::{EIO, O_ACCMODE, O_RDONLY, O_RDWR, O_WRONLY};
use rustix::{
    event::{poll, PollFd, PollFlags},
    io::Errno,
};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Key { modifier: bool },
    Click,
    Scroll,
}

const LEFTCTRL: u32 = 29;
const RIGHTCTRL: u32 = 29;
const LEFTSHIFT: u32 = 42;
const RIGHTSHIFT: u32 = 54;
const LEFTALT: u32 = 56;
const RIGHTALT: u32 = 97;
const LEFTSUPER: u32 = 125;
const RIGHTSUPER: u32 = 126;

const MODIFIERS: [u32; 8] = [
    LEFTCTRL, RIGHTCTRL, LEFTSHIFT, RIGHTSHIFT, LEFTALT, RIGHTALT, LEFTSUPER, RIGHTSUPER,
];

struct UDevInterface;

impl LibinputInterface for UDevInterface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        let flag = flags & O_ACCMODE;

        OpenOptions::new()
            .read(flag == O_RDONLY || flag == O_RDWR)
            .write(flag == O_WRONLY || flag == O_RDWR)
            .custom_flags(flags & !O_ACCMODE)
            .open(path)
            .map(OwnedFd::from)
            .map_err(|err| {
                warn!("cannot open {}: {err}", path.display());
                err.raw_os_error().unwrap_or(EIO)
            })
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        drop(File::from(fd));
    }
}

pub struct InputDevice {
    libinput: Libinput,
}

impl InputDevice {
    pub fn new(seat: &str) -> Result<Self> {
        let mut libinput = Libinput::new_with_udev(UDevInterface);
        libinput
            .udev_assign_seat(seat)
            .map_err(|()| anyhow!("failed to assign seat {seat:?}"))?;

        Ok(Self { libinput })
    }

    pub fn run(&mut self, mut f: impl FnMut(InputEvent)) -> Result<()> {
        loop {
            {
                let mut fds = [PollFd::new(&self.libinput, PollFlags::IN)];
                match poll(&mut fds, None) {
                    Ok(_) => {}
                    Err(Errno::INTR) => continue,
                    Err(e) => return Err(e).context("poll on libinput fd failed"),
                }
            }

            self.libinput
                .dispatch()
                .context("libinput dispatch failed")?;

            while let Some(event) = self.libinput.next() {
                if let Some(ev) = self.translate(event) {
                    f(ev);
                }
            }
        }
    }

    fn translate(&mut self, event: Event) -> Option<InputEvent> {
        match event {
            Event::Keyboard(KeyboardEvent::Key(key)) if key.key_state() == KeyState::Pressed => {
                Some(InputEvent::Key {
                    modifier: MODIFIERS.contains(&key.key()),
                })
            }
            Event::Pointer(PointerEvent::Button(button))
                if button.button_state() == ButtonState::Pressed =>
            {
                Some(InputEvent::Click)
            }
            Event::Pointer(PointerEvent::ScrollWheel(_)) => Some(InputEvent::Scroll),
            _ => None,
        }
    }
}
