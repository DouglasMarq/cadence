use super::cards::CardKind;
use super::covers::CoverKey;
use super::fetch::{ServerCall, fetch};
use super::results::with_results;
use crate::ui::images::{CoverSource, to_image};
use crate::ui::{AppWindow, Context, SearchKind};

const SEARCH_COVER_SIZE: u32 = 192;

pub struct CoverSlot {
    pub kind: SearchKind,
    pub index: i32,
}

pub fn load_cover(window: &AppWindow, context: &Context, slot: CoverSlot) {
    let kind = CardKind::from(slot.kind);
    let Ok(index) = usize::try_from(slot.index) else {
        return;
    };
    let claimed = with_results(window, |results| results.claim_cover(kind, index)).flatten();
    let Some((id, cover_id)) = claimed else {
        return;
    };
    let covers = context.view_covers();
    let key = CoverKey::new(kind, id);
    let shown_key = key.clone();
    let sent = fetch(
        window,
        context,
        ServerCall::new(
            move |server| async move {
                let source = CoverSource::Server { server, cover_id };
                source.fetch(covers.as_ref(), SEARCH_COVER_SIZE).await
            },
            move |_, results, cover| {
                let image = cover.and_then(|cover| to_image(&cover));
                results.set_cover(shown_key, index, image);
            },
        ),
    );
    if sent.is_err() {
        with_results(window, |results| results.covers.borrow_mut().finish(&key));
    }
}
