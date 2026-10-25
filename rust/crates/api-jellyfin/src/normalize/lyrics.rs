use cadence_core::lrc::lrc_timestamp;

use super::ticks_to_ms;
use crate::dto;

pub fn lyrics(lyrics: dto::Lyrics) -> Option<String> {
    let lines = lyrics.lyrics;
    let render_line: fn(dto::LyricLine) -> String = match lines.first()?.start {
        Some(_synced_start) => lrc_line,
        None => plain_line,
    };
    let rendered_lines: Vec<String> = lines.into_iter().map(render_line).collect();
    Some(rendered_lines.join("\n"))
}

fn plain_line(line: dto::LyricLine) -> String {
    line.text
}

fn lrc_line(line: dto::LyricLine) -> String {
    format!("{}{}", lrc_timestamp(ticks_to_ms(line.start)), line.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_lyrics_become_lrc() {
        let synced = dto::Lyrics {
            lyrics: vec![
                dto::LyricLine {
                    start: Some(0),
                    text: "one".into(),
                },
                dto::LyricLine {
                    start: Some(615_300_000),
                    text: "two".into(),
                },
            ],
        };
        assert_eq!(lyrics(synced).unwrap(), "[00:00.00]one\n[01:01.53]two");
    }

    #[test]
    fn unsynced_lyrics_are_plain() {
        let unsynced = dto::Lyrics {
            lyrics: vec![
                dto::LyricLine {
                    start: None,
                    text: "a".into(),
                },
                dto::LyricLine {
                    start: None,
                    text: "b".into(),
                },
            ],
        };
        assert_eq!(lyrics(unsynced).unwrap(), "a\nb");
        assert!(lyrics(dto::Lyrics::default()).is_none());
    }
}
