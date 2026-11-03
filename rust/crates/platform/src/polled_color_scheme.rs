use std::time::Duration;

use crate::{Result, SystemColorScheme};

pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) async fn watch_by_polling(
    read_color_scheme: impl Fn() -> SystemColorScheme,
    mut on_change: impl FnMut(SystemColorScheme),
) -> Result<()> {
    let mut shown = read_color_scheme();
    on_change(shown);
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        let current = read_color_scheme();
        if current != shown {
            shown = current;
            on_change(current);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const WATCHED_POLLS: u32 = 4;

    #[test]
    fn given_the_scheme_changes_when_polling_then_each_change_is_reported_once() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();
        let schemes = [
            SystemColorScheme::Light,
            SystemColorScheme::Light,
            SystemColorScheme::Dark,
            SystemColorScheme::Dark,
            SystemColorScheme::Light,
        ];
        let reads = Cell::new(0);
        let read = || {
            let index = reads.get();
            reads.set(index + 1);
            schemes
                .get(index)
                .copied()
                .unwrap_or(SystemColorScheme::Light)
        };
        let mut reported = Vec::new();
        let watching = runtime.block_on(async {
            tokio::time::timeout(
                POLL_INTERVAL * WATCHED_POLLS + POLL_INTERVAL / 2,
                watch_by_polling(read, |scheme| reported.push(scheme)),
            )
            .await
        });
        assert!(watching.is_err());
        assert_eq!(
            reported,
            [
                SystemColorScheme::Light,
                SystemColorScheme::Dark,
                SystemColorScheme::Light
            ]
        );
    }
}
