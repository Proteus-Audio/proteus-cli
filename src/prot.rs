use matroska::{Audio, Matroska, Settings};
use rand::Rng;
use serde_json;

pub struct ProtInfo {
    pub track_index_array: Vec<u32>,
    pub audio_settings: Audio,
    pub duration: u64,
}

pub fn parse_prot(file_path: &String) -> ProtInfo {
    let file = std::fs::File::open(file_path).unwrap();
    let mka: Matroska = Matroska::open(file).expect("Could not open file");

    let reader = get_reader(file_path);

    let first_track = reader.tracks().first().unwrap();

    let tb = first_track.codec_params.time_base.unwrap();
    let dur = first_track.codec_params.n_frames.map(|frames| first_track.codec_params.start_ts + frames).unwrap();
    tb.calc_time(dur);
    // let dur_in_seconds = dur / track.codec_params.sample_rate.unwrap() as u64;
    // println!("Track duration: {:?}", );

    let mut track_index_array: Vec<u32> = Vec::new();
    mka.attachments.iter().for_each(|attachment| {
        // Only print if name is "play_settings.json"
        if attachment.name == "play_settings.json" {
            // read json data from attachment.data to object
            let json_data: serde_json::Value = serde_json::from_slice(&attachment.data).unwrap();

            let encoder_version = json_data["encoder_version"].as_f64();

            // For each track in json_data, print the track number
            json_data["play_settings"]["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .for_each(|track| {
                    if let Some(_version) = encoder_version {
                        let indexes = track["ids"].as_array().unwrap();
                        let random_number = rand::thread_rng().gen_range(0..indexes.len());
                        let index = indexes[random_number].to_string().parse::<u32>().unwrap();
                        track_index_array.push(index);
                    } else {
                        let starting_index =
                            track["startingIndex"].to_string().parse::<u32>().unwrap() + 1;
                        let length = track["length"].to_string().parse::<u32>().unwrap();

                        // Get random number between starting_index and starting_index + length
                        let random_number =
                            rand::thread_rng().gen_range(starting_index..(starting_index + length));
                        track_index_array.push(random_number);
                    }
                });
        }
    });

    let first_audio_settings = mka
        .tracks
        .iter()
        .find_map(|track| {
            if let Settings::Audio(audio_settings) = &track.settings {
                Some(audio_settings.clone()) // assuming you want to keep the settings, and they are cloneable
            } else {
                None
            }
        })
        .expect("Could not find audio settings");

    ProtInfo {
        track_index_array,
        audio_settings: first_audio_settings,
        duration: tb.calc_time(dur).seconds
    }
}

use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn open_file(file_path: &String) -> (Box<dyn Decoder>, Box<dyn FormatReader>) {
    let format = get_reader(file_path);
    let decoder = get_decoder(&format);

    (decoder, format)
}

pub fn get_reader(file_path: &String) -> Box<dyn FormatReader> {
    // Open the media source.
    let src = std::fs::File::open(file_path).expect("failed to open media");

    // Create the media source stream.
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    // Create a probe hint using the file's extension. [Optional]
    let mut hint = Hint::new();
    hint.with_extension("mka");

    // Use the default options for metadata and format readers.
    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    // Probe the media source.
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .expect("unsupported format");

    // Get the instantiated format reader.
    let format = probed.format;

    // Find the first audio track with a known (decodeable) codec.
    format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .expect("no supported audio tracks");

    format
}

pub fn get_decoder(format: &Box<dyn FormatReader>) -> Box<dyn Decoder> {
    // Use the default options for the decoder.
    let dec_opts: DecoderOptions = Default::default();

    // Create a decoder for the track.
    let decoder = symphonia::default::get_codecs()
        .make(&format.tracks()[0].codec_params, &dec_opts)
        .expect("unsupported codec");

    decoder
}