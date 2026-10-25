use std::sync::Arc;

use cadence_api::MusicServer;
use slint::ComponentHandle;

use super::results::{Results, with_results};
use crate::ui::{AppWindow, Context, on_ui};

#[derive(Debug)]
pub struct NotConnected;

pub struct ServerCall<Request, Done> {
    request: Request,
    done: Done,
}

impl<Request, Done> ServerCall<Request, Done> {
    pub fn new<Value, Pending>(request: Request, done: Done) -> Self
    where
        Request: FnOnce(Arc<dyn MusicServer>) -> Pending,
        Done: FnOnce(&AppWindow, &Results, Value),
        Pending: Future<Output = Value>,
    {
        Self { request, done }
    }
}

pub fn fetch<Value, Pending, Request, Done>(
    window: &AppWindow,
    context: &Context,
    call: ServerCall<Request, Done>,
) -> Result<(), NotConnected>
where
    Request: FnOnce(Arc<dyn MusicServer>) -> Pending + Send + 'static,
    Done: FnOnce(&AppWindow, &Results, Value) + Send + 'static,
    Pending: Future<Output = Value> + Send + 'static,
    Value: Send + 'static,
{
    let server = context.session.get().ok_or(NotConnected)?;
    let weak = window.as_weak();
    let ServerCall { request, done } = call;
    context.runtime.spawn(async move {
        let result = request(server).await;
        on_ui(&weak, move |window| {
            with_results(window, |results| done(window, results, result));
        });
    });
    Ok(())
}
