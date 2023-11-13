use dasp_ring_buffer::Bounded;
use matroska::Audio;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink, Source, dynamic_mixer::{self, DynamicMixer}};
use std::{thread, collections::HashMap};
use std::time::Duration;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};


use crate::prot::*;
use crate::track::*;
use crate::buffer::*;

#[derive(Debug, Clone)]
pub struct Player {
    pub finished_tracks: Arc<Mutex<Vec<i32>>>,
    pub file_path: String,
    pub ts: Arc<Mutex<u32>>,
    playing: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    playback_thread_exists: Arc<AtomicBool>,
    duration: Arc<Mutex<u64>>,
    track_index_array: Arc<Mutex<Vec<u32>>>,
    audio_settings: Audio,
    buffer_map: Arc<Mutex<HashMap<i32, Bounded<Vec<f32>>>>>,
}

impl Player {
    pub fn new(file_path: &String) -> Self {
        let ProtInfo { 
            track_index_array,
            audio_settings ,
            duration
        } = parse_prot(&file_path);

        let buffer_map = init_buffer_map();

        let mut this = Self {
            finished_tracks: Arc::new(Mutex::new(Vec::new())),
            file_path: file_path.clone(),
            playing: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            ts: Arc::new(Mutex::new(0)),
            playback_thread_exists: Arc::new(AtomicBool::new(true)),
            duration: Arc::new(Mutex::new(duration)),
            track_index_array: Arc::new(Mutex::new(track_index_array)),
            audio_settings,
            buffer_map
        };

        this.initialize_thread();

        this
    }

    fn timestamp_thread(&self) {
        // TODO: find a more accurate way to count time. This method is not guaranteed to be accurate.
        let mut ts = self.ts.lock().unwrap();
        *ts = 0;
        drop(ts);

        let ts = self.ts.clone();
        let playing = self.playing.clone();
        let paused = self.paused.clone();
        let playback_thread_exists = self.playback_thread_exists.clone();

        thread::spawn(move || {
            loop {
                // let mut playing = playing.lock().unwrap();
                // let paused = paused.lock().unwrap();
                let mut ts = ts.lock().unwrap();
                let play_value = playing.load(Ordering::SeqCst);
                let pause_value = paused.load(Ordering::SeqCst);

                if !play_value && !pause_value {
                    break;
                }

                if play_value && !pause_value {
                    *ts += 1;
                }

                let thread_exists = playback_thread_exists.load(Ordering::SeqCst);
                if !thread_exists {
                    playing.store(false, Ordering::SeqCst);
                    break;
                }

                drop(ts);

                thread::sleep(Duration::from_millis(10));
            }
        });
    }

    fn initialize_thread(&mut self) {
        self.shuffle();
        // ===== Setup ===== //
        let sample_rate = self.audio_settings.sample_rate;

        let file_path = String::from(self.file_path.clone());
        let finished_tracks = self.finished_tracks.clone();

        // ===== Set play options ===== //
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(true, Ordering::SeqCst);

        let playing = self.playing.clone();
        let paused = self.paused.clone();

        let playback_thread_exists = self.playback_thread_exists.clone();

        self.timestamp_thread();
    
        let index_array = self.track_index_array.lock().unwrap();
        let length = index_array.len();
        let keys: Vec<i32> = index_array.iter().enumerate().map(|(i, _v)| i as i32).collect();
        let enum_track_index_array: Vec<(i32, u32)> = index_array
            .iter()
            .enumerate()
            // When trying to directly enumerate the track_index_array, the reference dies too early.
            .map(|(i, v)| (i32::from(i as i32), u32::from(*v))).collect();
        drop(index_array);

        self.ready_buffer_map(&keys);
        let buffer_map = self.buffer_map.clone();

        let abort = Arc::new(AtomicBool::new(false));

        // ===== Start playback ===== //
        thread::spawn(move || {
            playback_thread_exists.store(true, Ordering::Relaxed);

            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            let sink = Sink::try_new(&stream_handle).expect("failed to create sink");
    
            let (sender, receiver) = mpsc::sync_channel::<DynamicMixer<f32>>(1);
    
            let playing_map: Arc<Mutex<std::collections::HashMap<i32, Arc<Mutex<bool>>>>> = Arc::new(Mutex::new(std::collections::HashMap::new()));
    
            let file_path_clone = String::from(file_path.clone());
            for (key, track_id) in enum_track_index_array {
                let playing = buffer_track(TrackArgs{
                    file_path: file_path_clone.clone(),
                    track_id,
                    track_key: key,
                    buffer_map: buffer_map.clone(),
                    finished_tracks: finished_tracks.clone(),
                }, abort.clone());
    
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
    
                            
                            let source = SamplesBuffer::new(2, sample_rate as u32, samples);
                            controller.add(source.convert_samples().amplify(0.2));
                        }
                        
                        sender.send(mixer);
                    }
                    
                    drop(hash_buffer);
    
                    thread::sleep(Duration::from_millis(100));
                }
            });
    
            // let sink = sink_mutex.lock().unwrap();
            sink.play();

            // TODO: refactor this and the loop that follows it into a single loop
            for received in receiver {
                sink.append(received);

                if !playing.load(Ordering::SeqCst) && !paused.load(Ordering::SeqCst) {
                    abort.store(true, Ordering::Relaxed);
                    sink.clear();
                    break;
                }
                
                if paused.load(Ordering::SeqCst) && !sink.is_paused() {
                    sink.pause();
                } 
                if !paused.load(Ordering::SeqCst) && sink.is_paused() {
                    sink.play();
                }
            }
            
            loop {
                if !playing.load(Ordering::SeqCst) && !paused.load(Ordering::SeqCst) {
                    abort.store(true, Ordering::Relaxed);
                    sink.clear();
                    break;
                }

                if paused.load(Ordering::SeqCst) && !sink.is_paused() {
                    sink.pause();
                } 
                if !paused.load(Ordering::SeqCst) && sink.is_paused() {
                    sink.play();
                }


                // If all tracks are finished buffering and sink is finished playing, exit the loop
                let finished_tracks = finished_tracks.lock().unwrap();
                if sink.empty() && finished_tracks.len() == length {
                    break;
                }
                drop(finished_tracks);

                thread::sleep(Duration::from_millis(100));
            }
            
            playback_thread_exists.store(false, Ordering::Relaxed);
        });
    }

    pub fn play_at(&mut self, ts: u32) {
        let mut timestamp = self.ts.lock().unwrap();
        *timestamp = ts;
        drop(timestamp);
        self.play();
    }

    pub fn play(&mut self) {
        let thread_exists = self.playback_thread_exists.load(Ordering::SeqCst);

        if !thread_exists {
            self.initialize_thread();
        }

        self.resume();
    }

    pub fn pause(&self) {
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.playing.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        let mut buffer_map = self.buffer_map.lock().unwrap();
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        buffer_map.clear();

        drop (buffer_map);

        while !self.is_finished() {
            thread::sleep(Duration::from_millis(10));
        }

        self.reset();
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn get_time(&self) -> u32 {
        let ts = self.ts.lock().unwrap();
        *ts
    }

    pub fn is_finished(&self) -> bool {
        let playback_thread_exists = self.playback_thread_exists.load(Ordering::SeqCst);
        !playback_thread_exists
    }

    pub fn sleep_until_end(&self) {
        loop {
            if self.is_finished() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn get_duration(&self) -> u64 {
        let duration = self.duration.lock().unwrap();
        *duration
    }

    fn ready_buffer_map(&mut self, keys: &Vec<i32>) {
        self.buffer_map = init_buffer_map();

        let sample_rate = self.audio_settings.sample_rate;
        let buffer_size = sample_rate as usize * 1; // Ten seconds of audio at the sample rate

        for key in keys {
            let ring_buffer = Bounded::from(vec![0.0; buffer_size]);
            self.buffer_map.lock().unwrap().insert(*key, ring_buffer);
        }
    }

    pub fn shuffle(&self) {
        let ProtInfo { 
            track_index_array,
            audio_settings ,
            duration
        } = parse_prot(&self.file_path);

        let mut self_track_index_array = self.track_index_array.lock().unwrap();
        self_track_index_array.clone_from(&track_index_array);
        drop(self_track_index_array);

        // let keys = self.track_index_array.lock().unwrap().iter().enumerate().map(|(i, _v)| i as i32).collect();
        // self.ready_buffer_map(&keys);
    }

    fn reset (&self) {
        let ProtInfo { 
            track_index_array,
            audio_settings ,
            duration
        } = parse_prot(&self.file_path);

        let mut self_track_index_array = self.track_index_array.lock().unwrap();
        self_track_index_array.clone_from(&track_index_array);
        drop(self_track_index_array);

        // let keys = self.track_index_array.lock().unwrap().iter().enumerate().map(|(i, _v)| i as i32).collect();
        // self.ready_buffer_map(&keys);
    }
}