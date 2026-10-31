use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use super::locking::lock;

struct GateState {
    free_slots: usize,
    waiting: Vec<oneshot::Sender<DownloadSlot>>,
}

pub(super) struct DownloadGate {
    state: Mutex<GateState>,
}

pub(super) struct DownloadSlot {
    gate: Arc<DownloadGate>,
}

fn pop_newest_waiter(
    waiting: &mut Vec<oneshot::Sender<DownloadSlot>>,
) -> Option<oneshot::Sender<DownloadSlot>> {
    while let Some(waiter) = waiting.pop() {
        if !waiter.is_closed() {
            return Some(waiter);
        }
    }
    None
}

impl DownloadGate {
    pub(super) fn new(slots: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(GateState {
                free_slots: slots,
                waiting: Vec::new(),
            }),
        })
    }

    pub(super) async fn enter(self: &Arc<Self>) -> DownloadSlot {
        let Some(turn) = self.take_slot_or_queue() else {
            return self.slot();
        };
        turn.await.unwrap_or_else(|_closed| self.slot())
    }

    fn take_slot_or_queue(&self) -> Option<oneshot::Receiver<DownloadSlot>> {
        let mut state = lock(&self.state);
        if state.free_slots > 0 {
            state.free_slots -= 1;
            return None;
        }
        let (sender, turn) = oneshot::channel();
        state.waiting.push(sender);
        Some(turn)
    }

    fn slot(self: &Arc<Self>) -> DownloadSlot {
        DownloadSlot {
            gate: Arc::clone(self),
        }
    }

    fn hand_over(self: &Arc<Self>) {
        let next = {
            let mut state = lock(&self.state);
            let next = pop_newest_waiter(&mut state.waiting);
            if next.is_none() {
                state.free_slots += 1;
            }
            next
        };
        if let Some(waiter) = next
            && waiter.send(self.slot()).is_err()
        {
            tracing::debug!("a cover download stopped waiting as its turn came");
        }
    }
}

impl Drop for DownloadSlot {
    fn drop(&mut self) {
        self.gate.hand_over();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use super::*;

    const SETTLE: Duration = Duration::from_millis(20);

    async fn queue_in_order(
        gate: &Arc<DownloadGate>,
        order: &Arc<StdMutex<Vec<&'static str>>>,
        names: &[&'static str],
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut tasks = Vec::new();
        for name in names.iter().copied() {
            let gate = Arc::clone(gate);
            let order = Arc::clone(order);
            tasks.push(tokio::spawn(async move {
                let _slot = gate.enter().await;
                order.lock().unwrap().push(name);
            }));
            tokio::time::sleep(SETTLE).await;
        }
        tasks
    }

    #[tokio::test]
    async fn given_a_full_gate_when_downloads_wait_then_the_newest_goes_first() {
        let gate = DownloadGate::new(1);
        let order = Arc::new(StdMutex::new(Vec::new()));
        let held = gate.enter().await;
        let tasks = queue_in_order(&gate, &order, &["old", "middle", "new"]).await;
        drop(held);
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), ["new", "middle", "old"]);
    }

    #[tokio::test]
    async fn given_a_waiter_that_gave_up_when_a_slot_frees_then_the_slot_is_not_lost() {
        let gate = DownloadGate::new(1);
        let held = gate.enter().await;
        let abandoned = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move {
                let _slot = gate.enter().await;
            }
        });
        tokio::time::sleep(SETTLE).await;
        abandoned.abort();
        drop(held);
        let entered = tokio::time::timeout(Duration::from_secs(1), gate.enter()).await;
        assert!(entered.is_ok());
    }

    #[tokio::test]
    async fn given_free_slots_when_entering_then_no_download_waits() {
        let gate = DownloadGate::new(2);
        let first = gate.enter().await;
        let second = tokio::time::timeout(Duration::from_secs(1), gate.enter()).await;
        assert!(second.is_ok());
        drop(first);
    }
}
