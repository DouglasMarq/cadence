use keepawake::KeepAwake;

use crate::{PlatformError, Result};

const INHIBIT_REASON: &str = "Playing audio";
const REVERSE_DOMAIN: &str = "io.github.douglasmarq.cadence";

pub(crate) struct Assertion {
    app_name: String,
    held: Option<KeepAwake>,
}

impl Assertion {
    pub(crate) fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_owned(),
            held: None,
        }
    }

    pub(crate) fn set_held(&mut self, is_held: bool) -> Result<()> {
        match (is_held, self.held.is_some()) {
            (true, false) => self.held = Some(self.create()?),
            (false, true) => self.held = None,
            _ => {}
        }
        Ok(())
    }

    fn create(&self) -> Result<KeepAwake> {
        keepawake::Builder::default()
            .display(true)
            .idle(true)
            .reason(INHIBIT_REASON)
            .app_name(self.app_name.as_str())
            .app_reverse_domain(REVERSE_DOMAIN)
            .create()
            .map_err(inhibitor_error)
    }
}

fn inhibitor_error(error: keepawake::Error) -> PlatformError {
    PlatformError::SleepInhibitor(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inhibiting_twice_keeps_one_assertion_and_releasing_drops_it() {
        let mut assertion = Assertion::new("cadence-test");
        assertion.set_held(true).unwrap();
        assertion.set_held(true).unwrap();
        assert!(assertion.held.is_some());
        assertion.set_held(false).unwrap();
        assert!(assertion.held.is_none());
    }
}
