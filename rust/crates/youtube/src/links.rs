use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

const WATCH_PREFIX: &str = "https://music.youtube.com/watch?v=";
const SEARCH_PREFIX: &str = "https://music.youtube.com/search?q=";
const SONGS_SECTION: &str = "#songs";
const MAX_VIDEO_ID_LENGTH: usize = 64;

pub fn is_video_id(candidate: &str) -> bool {
    is_id_of_at_most(candidate, MAX_VIDEO_ID_LENGTH)
}

pub(crate) fn is_id_of_at_most(candidate: &str, max_length: usize) -> bool {
    !candidate.is_empty()
        && candidate.len() <= max_length
        && candidate
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

pub fn watch_url(video_id: &str) -> String {
    format!("{WATCH_PREFIX}{video_id}")
}

pub fn thumbnail_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
}

pub(crate) fn songs_search_url(query: &str) -> String {
    let encoded = utf8_percent_encode(query.trim(), NON_ALPHANUMERIC);
    format!("{SEARCH_PREFIX}{encoded}{SONGS_SECTION}")
}

const VIDEO_HOSTS: [&str; 4] = [
    "music.youtube.com",
    "www.youtube.com",
    "youtube.com",
    "m.youtube.com",
];
const SHORT_HOST: &str = "youtu.be";
const WATCH_PARAMETER: &str = "v";

pub fn video_id_from_link(link: &str) -> Option<String> {
    let link = link.trim();
    if is_video_id(link) {
        return Some(link.to_owned());
    }
    let without_scheme = link
        .strip_prefix("https://")
        .or_else(|| link.strip_prefix("http://"))
        .unwrap_or(link);
    let (host, rest) = without_scheme.split_once('/')?;
    let candidate = match host {
        SHORT_HOST => rest.split(['?', '#', '/']).next(),
        host if VIDEO_HOSTS.contains(&host) => watch_parameter(rest),
        _ => None,
    }?;
    is_video_id(candidate).then(|| candidate.to_owned())
}

fn watch_parameter(path_and_query: &str) -> Option<&str> {
    query_parameter(path_and_query.strip_prefix("watch?")?, WATCH_PARAMETER)
}

pub(crate) fn query_parameter<'query>(query: &'query str, wanted: &str) -> Option<&'query str> {
    let query = query.split('#').next().unwrap_or_default();
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == wanted)
        .map(|(_, value)| value)
}
