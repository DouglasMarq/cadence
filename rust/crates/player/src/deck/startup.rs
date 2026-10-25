use std::borrow::Cow;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use libmpv2::Mpv;

use super::setting::mpv_flag;
use super::{AUTOMATIC_AUDIO_DEVICE, Deck, MAX_VOLUME};
use crate::PlayerConfig;
use crate::crossfade::OUTPUT_BUFFER_SECONDS;
use crate::error::{PlayerError, Result, error_from_mpv};
use crate::event_loop::{self, EventCallback};
use crate::observed_property::ObservedProperty;
use crate::shared::{PlaylistEntries, Shared};

const STREAM_LAVF_OPTIONS_WITHOUT_RECONNECT_AT_EOF: &str =
    "reconnect=1,reconnect_streamed=1,reconnect_delay_max=5";
const BEST_EFFORT_DISABLED_OPTIONS: [&str; 3] =
    ["osc", "input-default-bindings", "input-vo-keyboard"];
const EVENT_THREAD_NAME: &str = "mpv-events";
const NETWORK_TIMEOUT_SECONDS: &str = "10";

type MpvOption<'value> = (&'static str, Cow<'value, str>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioOutputDriver {
    System,
    Null,
}

impl Deck {
    pub(crate) fn create(
        config: &PlayerConfig,
        driver: AudioOutputDriver,
        on_event: EventCallback,
    ) -> Result<Self> {
        let options = initial_options(config, driver);
        let mpv = initialize_mpv(&options)?;
        observe_properties(&mpv)?;
        let shared = Arc::new(Shared {
            mpv,
            stop_requested: AtomicBool::new(false),
            entries: Mutex::new(PlaylistEntries::default()),
        });
        let event_thread = spawn_event_thread(Arc::clone(&shared), on_event)?;
        Ok(Self {
            shared,
            event_thread: Some(event_thread),
        })
    }
}

fn initial_options(config: &PlayerConfig, driver: AudioOutputDriver) -> Vec<MpvOption<'_>> {
    let mut options = vec![
        ("vo", "null".into()),
        ("vid", "no".into()),
        ("audio-display", "no".into()),
        ("idle", "yes".into()),
        ("load-scripts", "no".into()),
        ("ytdl", "no".into()),
        (
            "gapless-audio",
            gapless_audio_value(config.is_gapless).into(),
        ),
        ("prefetch-playlist", mpv_flag(config.is_gapless).into()),
        ("network-timeout", NETWORK_TIMEOUT_SECONDS.into()),
        (
            "stream-lavf-o",
            STREAM_LAVF_OPTIONS_WITHOUT_RECONNECT_AT_EOF.into(),
        ),
        ("replaygain", config.replay_gain.mpv_value().into()),
        ("replaygain-clip", "no".into()),
        ("volume", config.volume.min(MAX_VOLUME).to_string().into()),
        ("audio-buffer", OUTPUT_BUFFER_SECONDS.to_string().into()),
        ("mute", mpv_flag(config.is_muted).into()),
        (
            "audio-device",
            config
                .audio_device
                .as_deref()
                .unwrap_or(AUTOMATIC_AUDIO_DEVICE)
                .into(),
        ),
    ];
    if driver == AudioOutputDriver::Null {
        options.push(("ao", "null".into()));
    }
    options
}

fn gapless_audio_value(is_gapless: bool) -> &'static str {
    match is_gapless {
        true => "weak",
        false => "no",
    }
}

fn use_c_numeric_locale_required_by_libmpv() {
    unsafe { libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr()) };
}

fn initialize_mpv(options: &[MpvOption<'_>]) -> Result<Mpv> {
    use_c_numeric_locale_required_by_libmpv();
    Mpv::with_initializer(|initializer| {
        for (name, value) in options {
            initializer.set_property(name, value.as_ref())?;
        }
        for name in BEST_EFFORT_DISABLED_OPTIONS {
            if let Err(error) = initializer.set_property(name, "no") {
                tracing::debug!("mpv option {name} could not be disabled: {error}");
            }
        }
        Ok(())
    })
    .map_err(error_from_mpv)
}

fn observe_properties(mpv: &Mpv) -> Result<()> {
    ObservedProperty::ALL.into_iter().try_for_each(|property| {
        mpv.observe_property(property.mpv_name(), property.format(), property.reply_id())
            .map_err(error_from_mpv)
    })
}

fn spawn_event_thread(shared: Arc<Shared>, on_event: EventCallback) -> Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(EVENT_THREAD_NAME.into())
        .spawn(move || event_loop::run(shared, on_event))
        .map_err(|source| PlayerError::Thread {
            name: EVENT_THREAD_NAME,
            source,
        })
}
