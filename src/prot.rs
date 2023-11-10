use matroska::{Matroska, Settings, Audio};
use rand::Rng;
use serde_json;

pub fn parse_prot(file_path: &String) -> (Vec<u32>, Audio) {
    let file = std::fs::File::open(file_path).unwrap();
    let mka: Matroska = Matroska::open(file).expect("Could not open file");

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

    let first_audio_settings = mka.tracks.iter().find_map(|track| {
        if let Settings::Audio(audio_settings) = &track.settings {
            Some(audio_settings.clone()) // assuming you want to keep the settings, and they are cloneable
        } else {
            None
        }
    }).expect("Could not find audio settings");

    return (track_index_array, first_audio_settings);
}

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn open_mka(file_path: &String) -> (Box<dyn Decoder>, Box<dyn FormatReader>) {
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
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .expect("no supported audio tracks");

    // Use the default options for the decoder.
    let dec_opts: DecoderOptions = Default::default();

    // Create a decoder for the track.
    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .expect("unsupported codec");

    (decoder, format)
}

use std::{sync::mpsc, thread};
use symphonia::core::errors::Error;
use log::warn;

pub fn buffer_mka(file_path: &String, track_id: u32) -> mpsc::Receiver<(u32, Vec<f32>)> {
    // Create a channel for sending audio chunks from the decoder to the playback system.
    let (sender, receiver) = mpsc::sync_channel::<(u32, Vec<f32>)>(1);
    let (mut decoder, mut format) = open_mka(file_path);

    thread::spawn(move || {
        // Get the selected track using the track ID.
        let track = format.tracks().iter().find(|track| track.id == track_id).expect("no track found");

        // Get the selected track's timebase and duration.
        // let tb = track.codec_params.time_base;
        let dur = track.codec_params.n_frames.map(|frames| track.codec_params.start_ts + frames);


        let _result: Result<bool, Error> = loop {
            // Get the next packet from the format reader.
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(err) => break Err(err),
            };

            if packet.track_id() != track_id {
                continue;
            }

            // If playback is finished, break out of the loop.
            if packet.ts() >= dur.unwrap_or(0) {
                sender.send((track_id, Vec::new())).unwrap();
                break Ok(true);
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    match decoded {
                        AudioBufferRef::F32(buf) => {
                            // Convert the interleaved samples to a vector of stereo samples.
                            // TODO: Support other channel layouts.
                            let stereo_samples: Vec<f32> = buf.chan(0).to_vec().into_iter().zip(buf.chan(1).to_vec().into_iter())
                                .flat_map(|(left, right)| vec![left, right])
                                .collect();

                            if stereo_samples.len() == 0 {
                                continue;
                            }

                            match sender.send((packet.track_id(), stereo_samples)) {
                                Ok(_) => {}
                                Err(_) => {
                                    println!("Error sending buffer");
                                }
                            }
                        }
                        _ => {
                            // Repeat for the different sample formats.
                            unimplemented!()
                        }
                    }
                }
                Err(Error::DecodeError(err)) => {
                    // Decode errors are not fatal. Print the error message and try to decode the next
                    // packet as usual.
                    warn!("decode error: {}", err);
                }
                Err(err) => break Err(err),
            }
        };
    });

    receiver
}