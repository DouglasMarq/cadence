use cadence_core::model::ReplayGain;

pub struct ReplayGainValues {
    pub album_gain: Option<f32>,
    pub album_peak: Option<f32>,
    pub track_gain: Option<f32>,
    pub track_peak: Option<f32>,
}

fn is_set(value: Option<f32>) -> bool {
    value.is_some_and(|number| number != 0.0)
}

pub fn replay_gain(values: ReplayGainValues) -> Option<ReplayGain> {
    let has_gain = is_set(values.album_gain) || is_set(values.track_gain);
    let has_peak = is_set(values.album_peak) || is_set(values.track_peak);
    if !has_gain && !has_peak {
        return None;
    }
    Some(ReplayGain {
        track_gain_db: values.track_gain.filter(|_| has_gain),
        track_peak: values.track_peak.filter(|_| has_peak),
        album_gain_db: values.album_gain.filter(|_| has_gain),
        album_peak: values.album_peak.filter(|_| has_peak),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_only_zero_values_when_normalizing_then_there_is_no_replay_gain() {
        let zero_track_gain = ReplayGainValues {
            album_gain: None,
            album_peak: None,
            track_gain: Some(0.0),
            track_peak: Some(0.0),
        };
        assert_eq!(replay_gain(zero_track_gain), None);
    }

    #[test]
    fn given_gains_without_peaks_when_normalizing_then_the_zero_peaks_are_dropped() {
        let gains_only = ReplayGainValues {
            album_gain: Some(-6.0),
            album_peak: Some(0.0),
            track_gain: Some(0.0),
            track_peak: None,
        };
        assert_eq!(
            replay_gain(gains_only),
            Some(ReplayGain {
                track_gain_db: Some(0.0),
                track_peak: None,
                album_gain_db: Some(-6.0),
                album_peak: None,
            })
        );
    }

    #[test]
    fn given_a_gain_and_a_peak_when_normalizing_then_both_groups_are_kept() {
        let album_gain_and_track_peak = ReplayGainValues {
            album_gain: Some(-6.0),
            album_peak: None,
            track_gain: None,
            track_peak: Some(0.9),
        };
        assert_eq!(
            replay_gain(album_gain_and_track_peak),
            Some(ReplayGain {
                track_gain_db: None,
                track_peak: Some(0.9),
                album_gain_db: Some(-6.0),
                album_peak: None,
            })
        );
    }
}
