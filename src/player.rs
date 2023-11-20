use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, Sink};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::prot::Prot;
use crate::{info::Info, player_engine::PlayerEngine};

#[derive(Clone)]
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
    prot: Arc<Mutex<Prot>>,
    audio_heard: Arc<AtomicBool>,
    volume: Arc<Mutex<f32>>,
    sink: Arc<Mutex<Sink>>,
}

impl Player {
    pub fn new(file_path: &String) -> Self {
        let info = Info::new(file_path.clone());
        let prot = Arc::new(Mutex::new(Prot::new(file_path)));

        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink: Arc<Mutex<Sink>> = Arc::new(Mutex::new(Sink::try_new(&stream_handle).unwrap()));

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
            audio_heard: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(Mutex::new(0.8)),
            sink,
            prot,
        };

        this.initialize_thread(None);

        this
    }

    fn initialize_thread(&mut self, ts: Option<f64>) {
        // Empty finished_tracks
        let mut finished_tracks = self.finished_tracks.lock().unwrap();
        finished_tracks.clear();
        drop(finished_tracks);

        // ===== Set play options ===== //
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(true, Ordering::SeqCst);
        self.playback_thread_exists.store(true, Ordering::SeqCst);

        // ===== Clone variables ===== //
        let paused = self.paused.clone();
        let playback_thread_exists = self.playback_thread_exists.clone();
        let time_passed = self.ts.clone();

        let abort = self.stop.clone();
        let duration = self.duration.clone();
        let prot = self.prot.clone();

        let audio_heard = self.audio_heard.clone();
        let volume = self.volume.clone();
        let sink_mutex = self.sink.clone();

        audio_heard.store(false, Ordering::Relaxed);

        // ===== Start playback ===== //
        thread::spawn(move || {
            // ===================== //
            // Set playback_thread_exists to true
            // ===================== //
            playback_thread_exists.store(true, Ordering::Relaxed);


            // ===================== //
            // Initialize engine & sink
            // ===================== //
            let start_time = match ts {
                Some(ts) => ts,
                None => 0.0,
            };
            let mut engine = PlayerEngine::new(prot, Some(abort.clone()), start_time);
            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            // let sink_mutex = Arc::new(Mutex::new(Sink::try_new(&stream_handle).unwrap()));

            let mut sink = sink_mutex.lock().unwrap();
            *sink = Sink::try_new(&stream_handle).unwrap();
            sink.set_volume(*volume.lock().unwrap());
            sink.play();
            drop(sink);

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
            *time_passed_unlocked = start_time;
            drop(time_passed_unlocked);

            let pause_sink = |sink: &Sink, fade_length_in_seconds: f32| {
                let timestamp = *time_passed.lock().unwrap();

                let fade_increments = sink.volume() / (fade_length_in_seconds * 100.0);
                // Fade out and pause sink
                while sink.volume() > 0.0 && timestamp != start_time {
                    sink.set_volume(sink.volume() - fade_increments);
                    thread::sleep(Duration::from_millis(10));
                }
                sink.pause();
            };

            let resume_sink = |sink: &Sink, fade_length_in_seconds: f32| {
                let volume = *volume.lock().unwrap();
                let fade_increments = (volume - sink.volume()) / (fade_length_in_seconds * 100.0);
                // Fade in and play sink
                sink.play();
                while sink.volume() < volume {
                    sink.set_volume(sink.volume() + fade_increments);
                    thread::sleep(Duration::from_millis(5));
                }
            };

            // ===================== //
            // Check if the player should be paused or not
            // ===================== //
            let check_details = || {
                if abort.load(Ordering::SeqCst) {
                    let sink = sink_mutex.lock().unwrap();
                    pause_sink(&sink, 0.1);
                    sink.clear();
                    drop(sink);
                    
                    return false;
                }
                
                let sink = sink_mutex.lock().unwrap();
                if paused.load(Ordering::SeqCst) && !sink.is_paused() {
                    pause_sink(&sink, 0.1);
                }
                if !paused.load(Ordering::SeqCst) && sink.is_paused() {
                    resume_sink(&sink, 0.1);
                }
                drop(sink);
                
                return true;
            };

            // ===================== //
            // Update chunk_lengths / time_passed
            // ===================== //
            let update_chunk_lengths = || {
                if abort.load(Ordering::SeqCst) {
                    return;
                }
                
                let mut chunk_lengths = chunk_lengths.lock().unwrap();
                let mut time_passed_unlocked = time_passed.lock().unwrap();
                // Check how many chunks have been played (chunk_lengths.len() - sink.len())
                // since the last time this function was called
                // and add that to time_passed
                let sink = sink_mutex.lock().unwrap();
                let chunks_played = chunk_lengths.len() - sink.len();
                drop(sink);
                
                for _ in 0..chunks_played {
                    *time_passed_unlocked += chunk_lengths.remove(0);
                }

                drop(chunk_lengths);
                drop(time_passed_unlocked);
            };

            // ===================== //
            // Update sink for each chunk received from engine
            // ===================== //
            let update_sink = |(mixer, length_in_seconds): (SamplesBuffer<f32>, f64)| {
                audio_heard.store(true, Ordering::Relaxed);

                let sink = sink_mutex.lock().unwrap();
                sink.append(mixer);
                drop(sink);

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
                
                let sink = sink_mutex.lock().unwrap();
                let sink_empty = sink.empty();
                drop(sink);
                // If all tracks are finished buffering and sink is finished playing, exit the loop
                if sink_empty && engine.finished_buffering() {
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

        self.kill_current();
        self.stop.store(false, Ordering::SeqCst);
        self.initialize_thread(Some(ts));

        self.resume();

        // Wait until audio is heard
        while !self.audio_heard.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn play(&mut self) {
        let thread_exists = self.playback_thread_exists.load(Ordering::SeqCst);
        self.stop.store(false, Ordering::SeqCst);

        if !thread_exists {
            self.initialize_thread(None);
        }

        self.resume();

        // Wait until audio is heard
        while !self.audio_heard.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn pause(&self) {
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.playing.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn kill_current(&self) {
        self.playing.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);

        self.stop.store(true, Ordering::SeqCst);

        while !self.is_finished() {
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn stop(&self) {
        self.kill_current();
        self.ts.lock().unwrap().clone_from(&0.0);
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

    pub fn seek(&mut self, ts: f64) {
        let mut timestamp = self.ts.lock().unwrap();
        *timestamp = ts;
        drop(timestamp);

        let playing = self.is_playing();
        let paused = self.is_paused();
        let stopped = self.stop.load(Ordering::SeqCst);

        self.kill_current();
        self.initialize_thread(Some(ts));

        self.stop.store(stopped, Ordering::SeqCst);
        self.playing.store(playing, Ordering::SeqCst);
        self.paused.store(paused, Ordering::SeqCst);

        if !paused {
            self.resume();
        }
    }

    pub fn refresh_tracks(&mut self) {
        let mut prot = self.prot.lock().unwrap();
        prot.refresh_tracks();
        drop(prot);

        // If stopped, return
        if self.is_finished() {
            return;
        }

        // Kill current thread and start 
        // new thread at the current timestamp
        let ts = self.get_time();
        self.seek(ts);
        
        // If previously playing, resume
        if self.is_playing() {
            self.resume();
        }

        // Wait until audio is heard
        while !self.audio_heard.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn shuffle(&mut self) {
        self.refresh_tracks();
    }

    pub fn set_volume(&mut self, new_volume: f32) {
        let sink = self.sink.lock().unwrap();
        sink.set_volume(new_volume);
        drop(sink);
        
        let mut volume = self.volume.lock().unwrap();
        *volume = new_volume;
        drop(volume);
    }

    pub fn get_volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }
}
