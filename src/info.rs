use std::{path::Path, fs::File, collections::HashMap};

use symphonia::core::{
    codecs::CodecParameters,
    probe::{
        Hint,
        ProbeResult
    },
    io::{
        ReadOnlySource,
        MediaSource,
        MediaSourceStream
    },
    formats::FormatOptions,
    meta::MetadataOptions
};

pub fn get_time_from_frames(codec_params: &CodecParameters) -> f64 {
    let tb = codec_params.time_base.unwrap();
    let dur = codec_params.n_frames.map(|frames| codec_params.start_ts + frames).unwrap();
    let time = tb.calc_time(dur);

    time.seconds as f64 + time.frac
}

fn get_probe_result_from_string(file_path: &String) -> ProbeResult {
    let path_str = file_path.as_str();
    // Create a hint to help the format registry guess what format reader is appropriate.
    let mut hint = Hint::new();

    // If the path string is '-' then read from standard input.
    let source = if path_str == "-" {
        Box::new(ReadOnlySource::new(std::io::stdin())) as Box<dyn MediaSource>
    } else {
        // Othwerise, get a Path from the path string.
        let path = Path::new(path_str);

        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            if let Some(extension_str) = extension.to_str() {
                hint.with_extension(extension_str);
            }
        }

        Box::new(File::open(path).expect("failed to open media file")) as Box<dyn MediaSource>
    };

    // Create the media source stream using the boxed media source from above.
    let mss = MediaSourceStream::new(source, Default::default());

    // Use the default options for format readers other than for gapless playback.
    let format_opts = FormatOptions {
        // enable_gapless: !args.is_present("no-gapless"),
        ..Default::default()
    };

    // Use the default options for metadata readers.
    let metadata_opts: MetadataOptions = Default::default();

    // Get the value of the track option, if provided.
    // let track = match args.value_of("track") {
    //     Some(track_str) => track_str.parse::<usize>().ok(),
    //     _ => None,
    // };

    symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts).unwrap()
}

fn get_durations(file_path: &String) -> HashMap<u32, f64> {
    let mut probed = get_probe_result_from_string(file_path);

    let mut durations: Vec<f64> = Vec::new();

    if let Some(metadata_rev) = probed.format.metadata().current() {
        metadata_rev.tags().iter().for_each(|tag| {
            if tag.key == "DURATION" {
                // Convert duration of type 01:12:37.227000000 to 4337.227
                let duration = tag.value.to_string().clone();
                let duration_parts = duration.split(":").collect::<Vec<&str>>();
                let hours = duration_parts[0].parse::<f64>().unwrap();
                let minutes = duration_parts[1].parse::<f64>().unwrap();
                let seconds = duration_parts[2].parse::<f64>().unwrap();
                // let milliseconds = duration_parts[3].parse::<f64>().unwrap();
                let duration_in_seconds = (hours * 3600.0) + (minutes * 60.0) + seconds;

                println!("Duration: {}", duration_in_seconds);

                durations.push(duration_in_seconds);
            }
        });
    }

    // Convert durations to HashMap with key as index and value as duration
    let mut duration_map: HashMap<u32, f64> = HashMap::new();

    for (index, track) in probed.format.tracks().iter().enumerate() {
        println!("Track: {:?}", track.id);
        if let Some(real_duration) = durations.get(index) {
            duration_map.insert(track.id, *real_duration);
            continue;
        }

        let codec_params = &track.codec_params;
        let duration = get_time_from_frames(codec_params);
        duration_map.insert(track.id, duration);
    }

    duration_map
}

#[derive(Debug, Clone)]
pub struct Info {
    pub file_path: String,
    duration_map: HashMap<u32, f64>,
}

impl Info {
    pub fn new(file_path: String) -> Self {
        Self {
            duration_map: get_durations(&file_path),
            file_path,
        }
    }
    
    pub fn get_duration(&self, index: u32) -> Option<f64> {
        match self.duration_map.get(&index) {
            Some(duration) => Some(*duration),
            None => None,
        }
    }
}
