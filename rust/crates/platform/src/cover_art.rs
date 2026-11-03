use std::path::Path;

use crate::NowPlaying;
use crate::image_url::is_free_of_credentials;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoverArt<'now_playing> {
    Url(&'now_playing str),
    File(&'now_playing Path),
}

pub(crate) fn cover_art(now_playing: &NowPlaying) -> Option<CoverArt<'_>> {
    now_playing
        .cover_url
        .as_deref()
        .filter(|url| !url.is_empty() && is_free_of_credentials(url))
        .map(CoverArt::Url)
        .or_else(|| now_playing.cover_file.as_deref().map(CoverArt::File))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREDENTIAL_URL: &str =
        "http://navidrome.example/rest/getCoverArt.view?id=1&u=cadence&t=token&s=salt";
    const PUBLIC_URL: &str = "https://jellyfin.example/Items/1/Images/Primary";

    fn playing(cover_url: Option<&str>, cover_file: Option<&str>) -> NowPlaying {
        NowPlaying {
            cover_url: cover_url.map(str::to_owned),
            cover_file: cover_file.map(Into::into),
            ..NowPlaying::default()
        }
    }

    #[test]
    fn given_a_url_without_credentials_when_choosing_the_cover_then_it_wins_over_the_file() {
        let now_playing = playing(Some(PUBLIC_URL), Some("covers/abc"));
        assert_eq!(cover_art(&now_playing), Some(CoverArt::Url(PUBLIC_URL)));
    }

    #[test]
    fn given_a_url_with_credentials_when_choosing_the_cover_then_the_file_is_used() {
        let now_playing = playing(Some(CREDENTIAL_URL), Some("covers/abc"));
        assert_eq!(
            cover_art(&now_playing),
            Some(CoverArt::File(Path::new("covers/abc")))
        );
    }

    #[test]
    fn given_a_url_with_credentials_and_no_file_when_choosing_the_cover_then_there_is_none() {
        assert_eq!(cover_art(&playing(Some(CREDENTIAL_URL), None)), None);
        assert_eq!(cover_art(&playing(Some(""), None)), None);
    }
}
