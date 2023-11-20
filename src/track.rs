use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use dasp_ring_buffer::Bounded;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::units::Time;
use std::sync::{Mutex, Arc};
use std::thread;
use symphonia::core::errors::Error;
use symphonia::core::audio::{AudioBufferRef, Signal};
use log::warn;

use crate::buffer::buffer_remaining_space;
use crate::tools::open_file;

pub struct TrackArgs {
    pub file_path: String,
    pub track_id: Option<u32>,
    pub track_key: i32,
    pub buffer_map: Arc<Mutex<HashMap<i32, Bounded<Vec<f32>>>>>,
    pub finished_tracks: Arc<Mutex<Vec<i32>>>,
    pub start_time: f64,
}

pub fn buffer_track(args: TrackArgs, abort: Arc<AtomicBool>) -> Arc<Mutex<bool>> {
    let TrackArgs { file_path, track_id, track_key, buffer_map, finished_tracks, start_time } = args;
    // Create a channel for sending audio chunks from the decoder to the playback system.
    let (mut decoder, mut format) = open_file(&file_path);
    let playing: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));

    thread::spawn(move || {
        // If not explicitly specified, use the first audio track.
        let track_id = track_id.unwrap_or(0);
        // let track_id = track_id.unwrap_or_else(|| {
        //     format.tracks().iter().find(|track| track.codec_params. == symphonia::core::media::Type::Audio)
        //         .expect("no audio track found").id
        // });

        // Get the selected track using the track ID.
        let track = format.tracks().iter().find(|track| track.id == track_id).expect("no track found");

        // Get the selected track's timebase and duration.
        // let tb = track.codec_params.time_base;
        let dur = track.codec_params.n_frames.map(|frames| track.codec_params.start_ts + frames);

        // Start time at given time
        let seconds = start_time.floor() as u64;
        let frac_of_second = start_time.fract();
        let time = Time::new(seconds, frac_of_second);

        let seek_success = format.seek(SeekMode::Coarse, SeekTo::Time { time, track_id: Some(track_id) });

        if seek_success.is_err() {
            mark_track_as_finished(&mut finished_tracks.clone(), track_key);
            return;
        }

        // TODO: Use actual time?
        // let actual_time = track.codec_params.time_base.unwrap().calc_time(seek_success.unwrap().actual_ts);

        let _result: Result<bool, Error> = loop {
            if abort.load(std::sync::atomic::Ordering::Relaxed) {
                break Ok(true);
            }

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

                            add_samples_to_buffer_map(&mut buffer_map.clone(), track_key, stereo_samples);
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

        // If an error occurred, print the error message.
        if let Err(err) = _result {
            warn!("error: {}", err);
        }

        // Mark the track as finished
        mark_track_as_finished(&mut finished_tracks.clone(), track_key);
    });

    return playing;
}

fn add_samples_to_buffer_map(buffer_map: &mut Arc<Mutex<HashMap<i32, Bounded<Vec<f32>>>>>, track_key: i32, samples: Vec<f32>) {    
    while buffer_remaining_space(buffer_map, track_key) < samples.len() {
        thread::sleep(Duration::from_millis(100));
    }


    let mut hash_buffer = buffer_map.lock().unwrap();

    for sample in samples {
        hash_buffer.get_mut(&track_key).unwrap().push(sample);
    }

    drop(hash_buffer);
}

fn mark_track_as_finished(finished_tracks: &mut Arc<Mutex<Vec<i32>>>, track_key: i32) {
    let mut finished_tracks_copy = finished_tracks.lock().unwrap();
    finished_tracks_copy.push(track_key);
    drop(finished_tracks_copy);
}