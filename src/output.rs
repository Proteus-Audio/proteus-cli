use rodio::{buffer::SamplesBuffer, OutputStream, Sink, Source, dynamic_mixer::{self, DynamicMixer}};
use std::{sync::{mpsc, Arc, Mutex}, thread, time::Duration};

use crate::prot::*;
use crate::buffer::*;

pub fn play(file_path: &String) {
    let finished_tracks = Arc::new(Mutex::new(Vec::new()));

    let (track_index_array, audio_settings) = parse_prot(file_path);
    let sample_rate = audio_settings.sample_rate;

    // TODO: Support other channel layouts.
    // let channels = audio_settings.channels;

    let keys: Vec<i32> = track_index_array.iter().enumerate().map(|(i, _v)| i as i32).collect();

    let hash_buffer = init_hash_buffer(&keys, Some(sample_rate as usize));
    let (sender, receiver) = mpsc::sync_channel::<DynamicMixer<f32>>(1);

    let enum_track_index_array = track_index_array
        .iter()
        .enumerate()
        // When trying to directly enumerate the track_index_array, the reference dies too early.
        .map(|(i, v)| (i32::from(i as i32), u32::from(*v)));

    for (key, track_id) in enum_track_index_array {
        let buffer = buffer_mka(file_path, track_id, key);
        let hash_buffer_copy = hash_buffer.clone();
        let finished_tracks_copy = finished_tracks.clone();
        // let source = SamplesBuffer::new(2, 44100, Vec::<f32>::new());
        // source.
        thread::spawn(move || {
            loop {
                // println!("track_index: {}", this_index);
                let buffer_receiver = buffer.recv();
                if buffer_receiver.is_err() {
                    // Channel hung up, so add track_id to finished_tracks
                    finished_tracks_copy.lock().unwrap().push(key);
                    break;
                }
                
                let (track_key, samples) = buffer_receiver.unwrap();

                while buffer_remaining_space(&hash_buffer_copy, track_key) < samples.len() {
                    thread::sleep(Duration::from_millis(100));
                }


                let mut hash_buffer = hash_buffer_copy.lock().unwrap();

                for sample in samples {
                    hash_buffer.get_mut(&track_key).unwrap().push(sample);
                }

                drop(hash_buffer);
            }
        });
    }

    thread::spawn(move || {
        let hash_buffer_copy = hash_buffer.clone();
        let finished_tracks_copy = finished_tracks.clone();

        loop {
            let mut hash_buffer = hash_buffer_copy.lock().unwrap();

            let mut removable_tracks: Vec<i32> = Vec::new();
            
            // if all buffers are not empty, add samples from each buffer to the mixer
            // until at least one buffer is empty
            let mut all_buffers_full = true;
            for (track_key, buffer) in hash_buffer.iter() {
                // println!("Buffer length: {}", buffer.len());
                if buffer.len() == 0 {
                    let finished = finished_tracks_copy.lock().unwrap();
                    if finished.contains(&track_key) {
                        removable_tracks.push(*track_key);
                        continue;
                    }
                    all_buffers_full = false;
                }
            }

            for track_id in removable_tracks {
                hash_buffer.remove(&track_id);
            }

            // If hash_buffer contains no tracks, exit the loop
            if hash_buffer.len() == 0 {
                break;
            }

            
            if all_buffers_full {
                let (controller, mixer) = dynamic_mixer::mixer::<f32>(2, 44_100);
                let length_of_smallest_buffer = hash_buffer.iter().map(|(_, buffer)| buffer.len()).min().unwrap();
                for (_, buffer) in hash_buffer.iter_mut() {
                    let mut samples: Vec<f32> = Vec::new();
                    for _ in 0..length_of_smallest_buffer {
                        samples.push(buffer.pop().unwrap());
                    }

                    
                    // println!("samples: {:?}", samples);
                    let source = SamplesBuffer::new(2, sample_rate as u32, samples);
                    controller.add(source.convert_samples().amplify(0.2));
                }
                
                sender.clone().send(mixer).unwrap();
            }
            
            drop(hash_buffer);

            thread::sleep(Duration::from_millis(100));
        }
    });
    
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).expect("failed to create sink");

    for received in receiver {
        sink.append(received);
    }

    thread::sleep(Duration::from_secs(1));

    // Sleep the thread until sink is empty.
    sink.sleep_until_end();

}
