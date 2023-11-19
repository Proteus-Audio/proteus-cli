use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, Sink};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::tools::parse_prot;
use crate::{info::Info, player_engine::PlayerEngine};

#[derive(Debug, Clone)]
pub struct Player {
    pub info: Info,
    pub finished_tracks: Arc<Mutex<Vec<i32>>>,
    pub file_path: String,
    pub ts: Arc<Mutex<f64>>,
    playing: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    playback_thread_exists: Arc<AtomicBool>,
    duration: Arc<Mutex<f64>>,
}

impl Player {
    pub fn new(file_path: &String) -> Self {
        let info = Info::new(file_path.clone());
        let prot_info = parse_prot(&file_path, &info);

        let mut this = Self {
            info,
            finished_tracks: Arc::new(Mutex::new(Vec::new())),
            file_path: file_path.clone(),
            playing: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            ts: Arc::new(Mutex::new(0.0)),
            playback_thread_exists: Arc::new(AtomicBool::new(true)),
            duration: Arc::new(Mutex::new(0.0)),
            stop: Arc::new(AtomicBool::new(false)),
        };

        this.initialize_thread();

        this
    }

    fn initialize_thread(&mut self) {
        // ===== Setup ===== //
        let file_path = String::from(self.file_path.clone());
        // Empty finished_tracks
        let mut finished_tracks = self.finished_tracks.lock().unwrap();
        finished_tracks.clear();
        drop(finished_tracks);

        // ===== Set play options ===== //
        println!("Initializing thread");
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(true, Ordering::SeqCst);
        self.playback_thread_exists.store(true, Ordering::SeqCst);

        // ===== Clone variables ===== //
        let paused = self.paused.clone();
        let playback_thread_exists = self.playback_thread_exists.clone();
        let time_passed = self.ts.clone();

        let abort = self.stop.clone();
        let duration = self.duration.clone();

        // ===== Start playback ===== //
        thread::spawn(move || {
            // ===================== //
            // Set playback_thread_exists to true
            // ===================== //
            playback_thread_exists.store(true, Ordering::Relaxed);


            // ===================== //
            // Initialize engine & sink
            // ===================== //
            let mut engine = PlayerEngine::new(&file_path, Some(abort.clone()));
            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            let sink = Sink::try_new(&stream_handle).unwrap();
            sink.play();

            // ===================== //
            // Set duration from engine
            // ===================== //
            let mut duration = duration.lock().unwrap();
            *duration = engine.get_duration();
            drop(duration);

            // ===================== //
            // Initialize chunk_lengths & time_passed
            // ===================== //
            let chunk_lengths = Arc::new(Mutex::new(Vec::new()));
            let mut time_passed_unlocked = time_passed.lock().unwrap();
            *time_passed_unlocked = 0.0;
            drop(time_passed_unlocked);

            // ===================== //
            // Check if the player should be paused or not
            // ===================== //
            let check_details = || {
                if abort.load(Ordering::SeqCst) {
                    sink.clear();
                    return false;
                }

                if paused.load(Ordering::SeqCst) && !sink.is_paused() {
                    sink.pause();
                }
                if !paused.load(Ordering::SeqCst) && sink.is_paused() {
                    sink.play();
                }

                return true;
            };

            // ===================== //
            // Update chunk_lengths / time_passed
            // ===================== //
            let update_chunk_lengths = || {
                let mut chunk_lengths = chunk_lengths.lock().unwrap();
                let mut time_passed = time_passed.lock().unwrap();
                // Check how many chunks have been played (chunk_lengths.len() - sink.len())
                // since the last time this function was called
                // and add that to time_passed
                let chunks_played = chunk_lengths.len() - sink.len();
                for _ in 0..chunks_played {
                    *time_passed += chunk_lengths.remove(0);
                }

                // println!("Time passed: {}", *time_passed);
            };

            // ===================== //
            // Update sink for each chunk received from engine
            // ===================== //
            let update_sink = |(mixer, length_in_seconds): (SamplesBuffer<f32>, f64)| {
                sink.append(mixer);

                let mut chunk_lengths = chunk_lengths.lock().unwrap();
                chunk_lengths.push(length_in_seconds);
                drop(chunk_lengths);

                update_chunk_lengths();
                check_details();
            };

            engine.reception_loop(&update_sink);

            // ===================== //
            // Wait until all tracks are finished playing in sink
            // ===================== //
            loop {
                update_chunk_lengths();
                if !check_details() {
                    break;
                }

                // If all tracks are finished buffering and sink is finished playing, exit the loop
                if sink.empty() && engine.finished_buffering() {
                    break;
                }

                thread::sleep(Duration::from_millis(100));
            }

            // ===================== //
            // Set playback_thread_exists to false
            // ===================== //
            playback_thread_exists.store(false, Ordering::Relaxed);
        });
    }

    pub fn play_at(&mut self, ts: f64) {
        let mut timestamp = self.ts.lock().unwrap();
        *timestamp = ts;
        drop(timestamp);
        self.play();
    }

    pub fn play(&mut self) {
        let thread_exists = self.playback_thread_exists.load(Ordering::SeqCst);
        self.stop.store(false, Ordering::SeqCst);

        if !thread_exists {
            println!("Thread does not exist, initializing thread");
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
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        self.stop.store(true, Ordering::SeqCst);

        while !self.is_finished() {
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn get_time(&self) -> f64 {
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

    pub fn get_duration(&self) -> f64 {
        let duration = self.duration.lock().unwrap();
        *duration
    }
}
