use std::{
    pin::pin,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{anyhow, bail, Context, Result};
use futures_lite::{stream, StreamExt};
use tracing::{info, warn};
use zbus::{fdo::DBusProxy, names::BusName, Connection};

use crate::supress::{ConnectError, Suppressor};

const SERVICE: &str = "dev.eventsfx.Shell";

#[zbus::proxy(
    interface = "dev.eventsfx.Shell1",
    default_service = "dev.eventsfx.Shell",
    default_path = "/dev/eventsfx/Shell",
    gen_blocking = false
)]
trait Shell {
    #[zbus(property)]
    fn fullscreen(&self) -> zbus::Result<bool>;
}

pub struct Gnome {
    proxy: ShellProxy<'static>,
}

impl Gnome {
    pub fn connect() -> Result<Self, ConnectError> {
        zbus::block_on(async {
            let conn = Connection::session()
                .await
                .context("cannot connect to the session bus")?;

            let dbus = DBusProxy::new(&conn)
                .await
                .context("cannot create org.freedesktop.DBus proxy")?;
            let name = BusName::try_from(SERVICE).expect("valid bus name");
            let owned = dbus
                .name_has_owner(name)
                .await
                .context("cannot query the session bus")?;
            if !owned {
                return Err(ConnectError::Failed(anyhow!(
                    "{SERVICE} is not on the bus."
                )));
            }

            let proxy = ShellProxy::new(&conn)
                .await
                .context("cannot create DBus proxy")?;

            Ok(Self { proxy })
        })
    }
}

enum Update {
    Fullscreen(zbus::Result<bool>),
    Owner(bool),
}

impl Suppressor for Gnome {
    fn name(&self) -> &'static str {
        "gnome"
    }

    fn run(&mut self, suppressed: &AtomicBool) -> Result<()> {
        zbus::block_on(async {
            let mut last = None;
            let mut set = |now: bool| {
                if last != Some(now) {
                    info!(suppressed = now, "fullscreen state changed");
                    suppressed.store(now, Ordering::Relaxed);
                    last = Some(now);
                }
            };

            let props = self.proxy.receive_fullscreen_changed().await;
            let owners = self
                .proxy
                .inner()
                .receive_owner_changed()
                .await
                .context("cannot watch the extensions bus name")?;

            set(self
                .proxy
                .fullscreen()
                .await
                .context("reading fullscreen")?);

            let updates = stream::race(
                props.then(|c| async move { Update::Fullscreen(c.get().await) }),
                owners.map(|owner| Update::Owner(owner.is_some())),
            );
            let mut updates = pin!(updates);

            while let Some(update) = updates.next().await {
                match update {
                    Update::Fullscreen(v) => set(v.context("reading fullscreen")?),
                    Update::Owner(false) => {
                        warn!("{SERVICE} left the bus");
                        set(false);
                    }
                    Update::Owner(true) => {
                        let v: bool = self
                            .proxy
                            .inner()
                            .get_property("Fullscreen")
                            .await
                            .context("reading fullscreen from new owner")?;
                        set(v);
                    }
                }
            }

            bail!("session bus closed");
        })
    }
}
