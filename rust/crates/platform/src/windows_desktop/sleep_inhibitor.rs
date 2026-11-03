use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::keep_awake::Assertion;
use crate::{PlatformError, Result};

const THREAD_NAME: &str = "sleep-inhibitor";

pub struct SleepInhibitor {
    requests: Sender<bool>,
}

impl SleepInhibitor {
    pub fn new(app_name: &str) -> Result<Self> {
        let (requests, receiver) = mpsc::channel();
        let assertion = Assertion::new(app_name);
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || hold(assertion, &receiver))
            .map_err(|error| PlatformError::SleepInhibitor(Box::new(error)))?;
        Ok(Self { requests })
    }

    pub fn set_inhibited(&self, is_inhibited: bool) -> Result<()> {
        self.requests
            .send(is_inhibited)
            .map_err(|error| PlatformError::SleepInhibitor(Box::new(error)))
    }
}

fn hold(mut assertion: Assertion, requests: &Receiver<bool>) {
    for is_inhibited in requests {
        if let Err(error) = assertion.set_held(is_inhibited) {
            tracing::warn!("keeping the computer awake failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_an_inhibitor_when_inhibiting_and_releasing_then_its_thread_takes_each_request() {
        let inhibitor = SleepInhibitor::new("cadence-test").unwrap();
        inhibitor.set_inhibited(true).unwrap();
        inhibitor.set_inhibited(true).unwrap();
        inhibitor.set_inhibited(false).unwrap();
    }
}
