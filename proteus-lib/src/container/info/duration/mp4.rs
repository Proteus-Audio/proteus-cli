use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

use symphonia::core::formats::Track;

use super::{single_track_detail, DurationSourceKind};

pub(super) fn probe(file_path: &str, tracks: &[Track]) -> Option<Vec<super::DurationDetail>> {
    let ext = super::extension_lc(file_path)?;
    if !matches!(ext.as_str(), "mp4" | "m4a" | "aac") {
        return None;
    }

    let seconds = probe_seconds(file_path)?;
    single_track_detail(seconds, DurationSourceKind::Structural, true, tracks)
}

pub(super) fn probe_standalone(file_path: &str) -> Option<Vec<super::DurationDetail>> {
    let ext = super::extension_lc(file_path)?;
    if !matches!(ext.as_str(), "mp4" | "m4a" | "aac") {
        return None;
    }
    super::standalone_detail(
        probe_seconds(file_path)?,
        DurationSourceKind::Structural,
        true,
    )
}

fn probe_seconds(file_path: &str) -> Option<f64> {
    let mut file = File::open(file_path).ok()?;
    parse_mp4_duration(&mut file)
}

#[derive(Debug, Clone, Copy)]
struct Atom {
    name: [u8; 4],
    start: u64,
    header_len: u64,
    size: u64,
}

fn parse_mp4_duration<R: Read + Seek>(reader: &mut R) -> Option<f64> {
    let file_len = reader.seek(SeekFrom::End(0)).ok()?;
    reader.seek(SeekFrom::Start(0)).ok()?;
    parse_atoms_for_duration(reader, 0, file_len)
}

fn parse_atoms_for_duration<R: Read + Seek>(reader: &mut R, start: u64, end: u64) -> Option<f64> {
    let mut pos = start;
    let mut best = None;
    while pos + 8 <= end {
        reader.seek(SeekFrom::Start(pos)).ok()?;
        let atom = read_atom(reader, pos, end)?;
        let payload_start = atom.start + atom.header_len;
        let payload_end = atom.start + atom.size;
        if payload_end > end || atom.size < atom.header_len {
            return best;
        }

        match &atom.name {
            b"mdhd" => {
                if let Some(seconds) = parse_mdhd(reader, payload_start, payload_end) {
                    best = Some(seconds);
                }
            }
            b"moov" | b"trak" | b"mdia" => {
                if let Some(seconds) = parse_atoms_for_duration(reader, payload_start, payload_end)
                {
                    best = Some(seconds);
                }
            }
            _ => {}
        }
        pos = payload_end;
    }
    best
}

fn read_atom<R: Read + Seek>(reader: &mut R, start: u64, parent_end: u64) -> Option<Atom> {
    let mut header = [0u8; 8];
    reader.read_exact(&mut header).ok()?;
    let size32 = u32::from_be_bytes(header[0..4].try_into().ok()?) as u64;
    let mut header_len = 8u64;
    let size = if size32 == 1 {
        let mut ext = [0u8; 8];
        reader.read_exact(&mut ext).ok()?;
        header_len = 16;
        u64::from_be_bytes(ext)
    } else if size32 == 0 {
        parent_end.checked_sub(start)?
    } else {
        size32
    };
    Some(Atom {
        name: header[4..8].try_into().ok()?,
        start,
        header_len,
        size,
    })
}

fn parse_mdhd<R: Read + Seek>(reader: &mut R, start: u64, end: u64) -> Option<f64> {
    reader.seek(SeekFrom::Start(start)).ok()?;
    let mut version_flags = [0u8; 4];
    reader.read_exact(&mut version_flags).ok()?;
    let version = version_flags[0];
    if version == 1 {
        if end < start + 32 {
            return None;
        }
        let mut data = [0u8; 28];
        reader.read_exact(&mut data).ok()?;
        let timescale = u32::from_be_bytes(data[16..20].try_into().ok()?);
        let duration = u64::from_be_bytes(data[20..28].try_into().ok()?);
        if timescale == 0 || duration == 0 {
            return None;
        }
        Some(duration as f64 / timescale as f64)
    } else {
        if end < start + 20 {
            return None;
        }
        let mut data = [0u8; 16];
        reader.read_exact(&mut data).ok()?;
        let timescale = u32::from_be_bytes(data[8..12].try_into().ok()?);
        let duration = u32::from_be_bytes(data[12..16].try_into().ok()?);
        if timescale == 0 || duration == 0 {
            return None;
        }
        Some(duration as f64 / timescale as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn parses_nested_mdhd_duration() {
        let mut mdhd = Vec::new();
        mdhd.extend_from_slice(&[0, 0, 0, 0]);
        mdhd.extend_from_slice(&0u32.to_be_bytes());
        mdhd.extend_from_slice(&0u32.to_be_bytes());
        mdhd.extend_from_slice(&44_100u32.to_be_bytes());
        mdhd.extend_from_slice(&88_200u32.to_be_bytes());
        let file = atom(
            b"moov",
            &atom(b"trak", &atom(b"mdia", &atom(b"mdhd", &mdhd))),
        );

        let seconds = parse_mp4_duration(&mut std::io::Cursor::new(file)).expect("duration");

        assert!((seconds - 2.0).abs() < 1e-9);
    }
}
