use std::fs::File;

use symphonia::core::formats::Track;

use super::{single_track_detail, DurationSourceKind};

pub(super) fn probe(file_path: &str, tracks: &[Track]) -> Option<Vec<super::DurationDetail>> {
    let ext = super::extension_lc(file_path)?;
    if !matches!(ext.as_str(), "mka" | "mkv" | "webm" | "prot") {
        return None;
    }

    let file = File::open(file_path).ok()?;
    let matroska = matroska::Matroska::open(file).ok()?;
    let duration = matroska.info.duration?;
    let seconds = duration.as_secs_f64();
    single_track_detail(seconds, DurationSourceKind::Structural, true, tracks)
}
