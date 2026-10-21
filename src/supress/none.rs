use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::supress::Suppressor;

pub struct NoSuppress;

impl Suppressor for NoSuppress {
    fn name(&self) -> &'static str {
        "none"
    }

    fn run(&mut self, suppressed: &AtomicBool) -> Result<()> {
        suppressed.store(false, Ordering::Relaxed);
        Ok(())
    }
}
