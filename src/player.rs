use rodio::{buffer::SamplesBuffer, OutputStream, Sink, Source, dynamic_mixer::{self, DynamicMixer}};
use std::{sync::{mpsc, Arc, Mutex}, thread, time::Duration};

use crate::prot::*;
use crate::track::*;
use crate::buffer::*;

// TODO: Refactor to move track index array and audio settings into Player struct
pub struct Player {
    pub finished_tracks: Arc<Mutex<Vec<i32>>>,
    pub file_path: String,
    pub ts: Arc<Mutex<u32>>,
    playing: Arc<Mutex<bool>>,
    paused: Arc<Mutex<bool>>,
    playback_thread_exists: Arc<Mutex<bool>>,
}

impl Player {
    pub fn new(file_path: &String) -> Self {
        Self {
            finished_tracks: Arc::new(Mutex::new(Vec::new())),
            file_path: file_path.clone(),
            playing: Arc::new(Mutex::new(false)),
            paused: Arc::new(Mutex::new(false)),
            ts: Arc::new(Mutex::new(0)),
            playback_thread_exists: Arc::new(Mutex::new(false)),
        }
    }

    fn timestamp_thread(&self) {
        // Initialize timestamp at 0
        let mut ts = self.ts.lock().unwrap();
        *ts = 0;
        drop(ts);

        let ts = self.ts.clone();
        let playing = self.playing.clone();
        let paused = self.paused.clone();
        let finished_tracks = self.finished_tracks.clone();

        thread::spawn(move || {
            loop {
                let playing = playing.lock().unwrap();
                let paused = paused.lock().unwrap();
                let finished_tracks = finished_tracks.lock().unwrap();
                let mut ts = ts.lock().unwrap();

                if !*playing && !*paused {
                    break;
                }

                if *playing && !*paused {
                    *ts += 1;
                }

                drop(playing);
                drop(paused);
                drop(finished_tracks);
                drop(ts);

                thread::sleep(Duration::from_millis(10));
            }
        });
    }

    fn initialize_thread(&self) {
        // ===== Setup ===== //
        let (track_index_array, audio_settings) = parse_prot(&self.file_path);
        let sample_rate = audio_settings.sample_rate;

        let file_path = String::from(self.file_path.clone());
        let finished_tracks = self.finished_tracks.clone();

        // ===== Set play options ===== //
        let mut playing = self.playing.lock().unwrap();
        let mut paused = self.paused.lock().unwrap();
        
        *playing = false;
        *paused = true;
        
        drop(playing);
        drop(paused);

        let playing = self.playing.clone();
        let paused = self.paused.clone();

        let playback_thread_exists = self.playback_thread_exists.clone();

        self.timestamp_thread();

        // ===== Start playback ===== //
        thread::spawn(move || {
            let mut thread_exists = playback_thread_exists.lock().unwrap();
            *thread_exists = true;
            drop(thread_exists);

            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            // let sink_mutex = self.sink.clone();
            let sink = Sink::try_new(&stream_handle).expect("failed to create sink");
    
            let keys: Vec<i32> = track_index_array.iter().enumerate().map(|(i, _v)| i as i32).collect();
    
            let buffer_map: Arc<Mutex<std::collections::HashMap<i32, dasp_ring_buffer::Bounded<Vec<f32>>>>> = init_hash_buffer(&keys, Some(sample_rate as usize));
            let (sender, receiver) = mpsc::sync_channel::<DynamicMixer<f32>>(1);
    
            let enum_track_index_array = track_index_array
                .iter()
                .enumerate()
                // When trying to directly enumerate the track_index_array, the reference dies too early.
                .map(|(i, v)| (i32::from(i as i32), u32::from(*v)));
    
            let playing_map: Arc<Mutex<std::collections::HashMap<i32, Arc<Mutex<bool>>>>> = Arc::new(Mutex::new(std::collections::HashMap::new()));
    
            let file_path_clone = String::from(file_path.clone());
            for (key, track_id) in enum_track_index_array {
                let playing = buffer_track(TrackArgs{
                    file_path: file_path_clone.clone(),
                    track_id,
                    track_key: key,
                    buffer_map: buffer_map.clone(),
                    finished_tracks: finished_tracks.clone(),
                });
    
                let mut playing_map = playing_map.lock().unwrap();
                playing_map.insert(key, playing);
                drop(playing_map);
            }
    
            // let sink_mutex_copy = sink_mutex.clone();
            let finished_tracks_copy = finished_tracks.clone();
    
            thread::spawn(move || {
                let hash_buffer_copy = buffer_map.clone();
    
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
                        // let (controller_b, mixer_b) = dynamic_mixer::mixer::<f32>(2, 44_100);
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
                        
                        sender.send(mixer).unwrap();
                        // sender.send(mixer_b).unwrap();
                        // let sink = sink_mutex_copy.lock().unwrap();
                        // sink.append(mixer);
                        // println!("sink: {:?}", sink.len());
                        // drop(sink);
                    }
                    
                    drop(hash_buffer);
    
                    thread::sleep(Duration::from_millis(100));
                }
            });
    
            // let sink = sink_mutex.lock().unwrap();
            sink.play();

            // TODO: refactor this and the loop that follows it into a single loop
            for received in receiver {
                // let sink = sink_mutex.lock().unwrap();
                sink.append(received);
                // drop(sink);

                let playing = playing.lock().unwrap();
                let paused = paused.lock().unwrap();
                if !*playing && !*paused {
                    break;
                }
                
                if *paused && !sink.is_paused() {
                    sink.pause();
                } 
                if !*paused && sink.is_paused() {
                    sink.play();
                }
                
                drop(playing);
                drop(paused);
            }
            
            loop {
                let playing = playing.lock().unwrap();
                let paused = paused.lock().unwrap();

                if !*playing && !*paused {
                    break;
                }

                if *paused && !sink.is_paused() {
                    sink.pause();
                } 
                if !*paused && sink.is_paused() {
                    sink.play();
                }
                
                drop(playing);
                drop(paused);

                thread::sleep(Duration::from_millis(100));
            }

            let mut thread_exists = playback_thread_exists.lock().unwrap();
            *thread_exists = true;
            drop(thread_exists);
        });
    }

    pub fn play_at(&self, ts: u32) {
        let mut timestamp = self.ts.lock().unwrap();
        *timestamp = ts;
        drop(timestamp);
        self.play();
    }

    pub fn play(&self) {
        let thread_exists = self.playback_thread_exists.lock().unwrap();
        if !*thread_exists {
            self.initialize_thread();
        }
        drop(thread_exists);

        // ===== Set play options ===== //
        let mut playing = self.playing.lock().unwrap();
        let mut paused = self.paused.lock().unwrap();
        
        *playing = true;
        *paused = false;
        
        drop(playing);
        drop(paused);
    }

    pub fn pause(&self) {
        let mut paused = self.paused.lock().unwrap();
        let mut playing = self.playing.lock().unwrap();

        *paused = true;
        *playing = false;
        
        drop(paused);
        drop(playing);
    }

    pub fn resume(&self) {
        let mut playing = self.playing.lock().unwrap();
        let mut paused = self.paused.lock().unwrap();
        
        *playing = true;
        *paused = false;
        
        drop(playing);
        drop(paused);
    }

    pub fn stop(&self) {
        let mut playing = self.playing.lock().unwrap();
        let mut paused = self.paused.lock().unwrap();
        
        *playing = false;
        *paused = false;
        
        drop(playing);
        drop(paused);
    }

    pub fn is_playing(&self) -> bool {
        let playing = self.playing.lock().unwrap();
        // let paused = self.paused.lock().unwrap();
        *playing
    }

    pub fn is_paused(&self) -> bool {
        let paused = self.paused.lock().unwrap();
        *paused
    }

    pub fn get_time(&self) -> u32 {
        let ts = self.ts.lock().unwrap();
        *ts
    }
}