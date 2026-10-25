use cadence_core::queue::Advance;

use super::Controller;
use crate::sleep_timer::SleepMode;

impl Controller {
    pub(super) fn set_sleep_timer(&mut self, mode: Option<SleepMode>) {
        match mode {
            Some(mode) => self.sleep_timer.start(mode, &self.deps.commands),
            None => self.sleep_timer.cancel(),
        }
        self.deps.ui.sleep_timer(self.sleep_timer.status());
        self.refresh_preload();
    }

    pub(super) fn sleep_tick(&mut self, elapsed_ms: u64) {
        if self.is_playing {
            self.count_down_sleep(elapsed_ms);
        }
        self.deps.ui.sleep_timer(self.sleep_timer.status());
    }

    fn count_down_sleep(&mut self, elapsed_ms: u64) {
        let is_ending = self.sleep_timer.ends_within(elapsed_ms);
        self.sleep_timer.tick(elapsed_ms);
        if is_ending {
            self.toggle_pause();
        }
    }

    pub(super) fn sleep_at_track_end(&mut self) {
        self.sleep_timer.cancel();
        self.deps.ui.sleep_timer(None);
        self.queue.next(Advance::Auto);
        self.stop();
        self.publish_now_playing();
        self.publish_queue_current();
        self.schedule_queue_sync();
    }
}
