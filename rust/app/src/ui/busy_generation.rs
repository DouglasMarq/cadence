#[derive(Debug, Default, Clone, Copy)]
pub struct BusyGeneration {
    generation: u64,
    is_busy: bool,
}

impl BusyGeneration {
    pub fn current(&self) -> u64 {
        self.generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub fn restart(&mut self) -> u64 {
        self.generation += 1;
        self.is_busy = false;
        self.generation
    }

    pub fn claim(&mut self) -> bool {
        if self.is_busy {
            return false;
        }
        self.is_busy = true;
        true
    }

    pub fn finish(&mut self, generation: u64) {
        if self.is_current(generation) {
            self.is_busy = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_claimed_generation_when_claimed_again_then_it_is_refused_until_finished() {
        let mut work = BusyGeneration::default();
        let generation = work.restart();
        assert!(work.claim());
        assert!(!work.claim());
        work.finish(generation + 1);
        assert!(!work.claim());
        work.finish(generation);
        assert!(work.claim());
    }

    #[test]
    fn given_a_busy_generation_when_restarted_then_the_old_one_is_stale_and_free() {
        let mut work = BusyGeneration::default();
        let old = work.restart();
        assert!(work.claim());
        let new = work.restart();
        assert!(!work.is_current(old));
        assert!(work.is_current(new));
        assert_eq!(work.current(), new);
        assert!(work.claim());
    }
}
