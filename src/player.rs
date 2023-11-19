use rodio::buffer::SamplesBuffer;
use rodio::{
    dynamic_mixer::DynamicMixer,
    OutputStream, Sink,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

use crate::{info::Info, prot::ProtInstance};

#[derive(Debug, Clone)]
pub struct Player {
    pub info: Info,
    pub finished_tracks: Arc<Mutex<Vec<i32>>>,
    pub file_path: String,
    pub ts: Arc<Mutex<u32>>,
    playing: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    playback_thread_exists: Arc<AtomicBool>,
    duration: Arc<Mutex<f64>>,
}

impl Player {
    pub fn new(file_path: &String) -> Self {
        let info = Info::new(file_path.clone());

        let mut this = Self {
            info,
            finished_tracks: Arc::new(Mutex::new(Vec::new())),
            file_path: file_path.clone(),
            playing: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            ts: Arc::new(Mutex::new(0)),
            playback_thread_exists: Arc::new(AtomicBool::new(true)),
            duration: Arc::new(Mutex::new(0.0)),
            stop: Arc::new(AtomicBool::new(false)),
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

        let abort = self.stop.clone();

        thread::spawn(move || {
            loop {
                // let mut playing = playing.lock().unwrap();
                // let paused = paused.lock().unwrap();
                let mut ts = ts.lock().unwrap();
                let play_value = playing.load(Ordering::SeqCst);
                let pause_value = paused.load(Ordering::SeqCst);
                let stop_value = abort.load(Ordering::SeqCst);

                if stop_value {
                    *ts = 0;
                    break;
                }

                if play_value && !pause_value {
                    *ts += 1;
                }

                let thread_exists = playback_thread_exists.load(Ordering::SeqCst);
                if !thread_exists {
                    *ts = 0;
                    playing.store(false, Ordering::SeqCst);
                    break;
                }

                drop(ts);

                thread::sleep(Duration::from_millis(10));
            }
        });
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

        self.timestamp_thread();

        let abort = self.stop.clone();
        let duration = self.duration.clone();

        // ===== Start playback ===== //
        thread::spawn(move || {
            playback_thread_exists.store(true, Ordering::Relaxed);
            
            let mut prot = ProtInstance::new(&file_path, Some(abort.clone()));

            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            let sink = Sink::try_new(&stream_handle).unwrap();

            // let sink = sink_mutex.lock().unwrap();
            sink.play();


            let mut duration = duration.lock().unwrap();
            let prot_duration = prot.get_duration();
            if prot_duration > *duration {
                *duration = prot_duration;
            }
            drop(duration);

            let update_sink = |mixer: SamplesBuffer<f32>| {
                sink.append(mixer);

                if abort.load(Ordering::SeqCst) {
                    sink.clear();
                }

                if paused.load(Ordering::SeqCst) && !sink.is_paused() {
                    sink.pause();
                }
                if !paused.load(Ordering::SeqCst) && sink.is_paused() {
                    sink.play();
                }
            };

            prot.reception_loop(&update_sink);

            loop {
                if abort.load(Ordering::SeqCst) {
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
                // println!("sink.empty(): {}", sink.empty());
                // println!("prot.finished_buffering(): {}", prot.finished_buffering());
                if sink.empty() && prot.finished_buffering() {
                    break;
                }

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

    pub fn get_duration(&self) -> f64 {
        let duration = self.duration.lock().unwrap();
        *duration
    }
}
