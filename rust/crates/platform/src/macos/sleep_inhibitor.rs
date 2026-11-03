use std::sync::{Mutex, PoisonError};

use crate::Result;
use crate::keep_awake::Assertion;

pub struct SleepInhibitor {
    assertion: Mutex<Assertion>,
}

impl SleepInhibitor {
    pub fn new(app_name: &str) -> Result<Self> {
        Ok(Self {
            assertion: Mutex::new(Assertion::new(app_name)),
        })
    }

    pub fn set_inhibited(&self, is_inhibited: bool) -> Result<()> {
        self.assertion
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_held(is_inhibited)
    }
}
