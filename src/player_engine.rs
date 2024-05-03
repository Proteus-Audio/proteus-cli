use dasp_ring_buffer::Bounded;
use rodio::{
    buffer::SamplesBuffer,
    dynamic_mixer::{self, DynamicMixer},
    Source,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use std::{collections::HashMap, sync::mpsc::Receiver, thread};

use crate::{buffer::*, prot::Prot};
// use crate::effects::*;
use crate::track::*;

#[derive(Debug, Clone)]
pub struct PlayerEngine {
    pub finished_tracks: Arc<Mutex<Vec<i32>>>,
    start_time: f64,
    abort: Arc<AtomicBool>,
    buffer_map: Arc<Mutex<HashMap<i32, Bounded<Vec<f32>>>>>,
    effects_buffer: Arc<Mutex<Bounded<Vec<f32>>>>,
    prot: Arc<Mutex<Prot>>,
}

impl PlayerEngine {
    pub fn new(prot: Arc<Mutex<Prot>>, abort_option: Option<Arc<AtomicBool>>, start_time: f64) -> Self {
        let buffer_map = init_buffer_map();
        let finished_tracks: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));
        let abort = if abort_option.is_some() {
            abort_option.unwrap()
        } else {
            Arc::new(AtomicBool::new(false))
        };

        let prot_unlocked = prot.lock().unwrap();
        let buffer_size = prot_unlocked.info.sample_rate as usize * 10; // Ten seconds of audio at the sample rate
        let effects_buffer = Arc::new(Mutex::new(Bounded::from(vec![0.0; buffer_size])));
        drop(prot_unlocked);

        let this = Self {
            finished_tracks,
            start_time,
            buffer_map,
            effects_buffer,
            abort,
            prot
        };

        this
    }

    pub fn reception_loop(&mut self, f: &dyn Fn((SamplesBuffer<f32>, f64))) {
        let prot = self.prot.lock().unwrap();
        let keys = prot.get_keys();
        drop(prot);
        self.ready_buffer_map(&keys);
        let receiver = self.get_receiver();

        for (mixer, length_in_seconds) in receiver {
            f((mixer, length_in_seconds));
        }
    }

    fn get_receiver(&self) -> Receiver<(SamplesBuffer<f32>, f64)> {
        // let (sender, receiver) = mpsc::sync_channel::<(DynamicMixer<f32>, f64)>(1);
        let (sender, receiver) = mpsc::sync_channel::<(SamplesBuffer<f32>, f64)>(1);

        let prot = self.prot.lock().unwrap();
        let audio_info = prot.info.clone();
        drop(prot);
        let buffer_map = self.buffer_map.clone();
        let abort = self.abort.clone();

        let playing_map: Arc<Mutex<std::collections::HashMap<i32, Arc<Mutex<bool>>>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));

        let finished_tracks = self.finished_tracks.clone();
        let effects_buffer = self.effects_buffer.clone();
        let prot_locked = self.prot.clone();
        let start_time = self.start_time;

        thread::spawn(move || {
            let prot = prot_locked.lock().unwrap();
            let enumerated_list = prot.enumerated_list();
            drop(prot);

            for (key, file_path, track_id) in enumerated_list {
                let playing = buffer_track(
                    TrackArgs {
                        file_path: file_path.clone(),
                        track_id,
                        track_key: key,
                        buffer_map: buffer_map.clone(),
                        finished_tracks: finished_tracks.clone(),
                        start_time
                    },
                    abort.clone(),
                );

                let mut playing_map = playing_map.lock().unwrap();
                playing_map.insert(key, playing);
                drop(playing_map);
            }

            // let sink_mutex_copy = sink_mutex.clone();
            let hash_buffer_copy = buffer_map.clone();

            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }

                let mut hash_buffer = hash_buffer_copy.lock().unwrap();

                let mut removable_tracks: Vec<i32> = Vec::new();

                // if all buffers are not empty, add samples from each buffer to the mixer
                // until at least one buffer is empty
                let mut all_buffers_full = true;
                for (track_key, buffer) in hash_buffer.iter() {
                    if buffer.len() == 0 {
                        let finished = finished_tracks.lock().unwrap();
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
                if hash_buffer.len() == 0 && effects_buffer.lock().unwrap().len() == 0 {
                    break;
                }

                if all_buffers_full || (effects_buffer.lock().unwrap().len() > 0 && hash_buffer.len() == 0) {
                    let (controller, mixer) = dynamic_mixer::mixer::<f32>(2, 44_100);
                    
                    // Hash buffer plus effects buffer
                    let mut effects_buffer_unlocked = effects_buffer.lock().unwrap();
                    let mut combined_buffer: HashMap<i32, Bounded<Vec<f32>>> = HashMap::new();
                    for (track_key, buffer) in hash_buffer.iter() {
                        combined_buffer.insert(*track_key, buffer.clone());
                    }


                    // combined_buffer.append(&mut effects_buffer.lock().unwrap());
                    // combined_buffer.insert(*track_key, combined_buffer);

                    
                    let length_of_smallest_buffer = hash_buffer
                        .iter()
                        .map(|(_, buffer)| buffer.len())
                        .min()
                        .unwrap();
                    for (_, buffer) in hash_buffer.iter_mut() {
                        let mut samples: Vec<f32> = Vec::new();
                        for _ in 0..length_of_smallest_buffer {
                            samples.push(buffer.pop().unwrap());
                        }

                        let source =
                            SamplesBuffer::new(2, audio_info.sample_rate as u32, samples);

                        controller.add(source.convert_samples().amplify(0.2));
                    }

                    // Add effects buffer to mixer
                    let num_effects_samples = if effects_buffer_unlocked.len() < length_of_smallest_buffer {
                        effects_buffer_unlocked.len()
                    } else {
                        length_of_smallest_buffer
                    };
                    
                    {
                        let mut samples: Vec<f32> = Vec::new();
                        for _ in 0..num_effects_samples {
                            samples.push(effects_buffer_unlocked.pop().unwrap());
                        }

                        let source =
                            SamplesBuffer::new(2, audio_info.sample_rate as u32, samples);

                        controller.add(source.convert_samples().amplify(0.2));
                    }

                    drop(effects_buffer_unlocked);

                    // let buffer = mixer.buffered().reverb(Duration::from_millis(100), 0.5).buffered();

                    let samples_buffer =
                        PlayerEngine::process_effects(mixer, effects_buffer.clone());

                    // while let Some(sample) = mixer.next() {
                    //     // mixer.sample_rate();
                    //     // mixer.channels();
                    //     // Process each sample as needed
                    //     // println!("Sample: {:?}", sample);
                    // }

                    // println!("Mixer size: {:?}", mixer.total_duration());
                    // println!("Smallest buffer size: {:?}", length_of_smallest_buffer);

                    let length_in_seconds = length_of_smallest_buffer as f64 / audio_info.sample_rate as f64 / audio_info.channels as f64;

                    sender.send((samples_buffer, length_in_seconds)).unwrap();
                }

                drop(hash_buffer);

                thread::sleep(Duration::from_millis(100));
            }
        });

        // Arc::new(receiver)
        receiver
    }

    pub fn process_effects(
        mixer: DynamicMixer<f32>,
        _effects_buffer: Arc<Mutex<Bounded<Vec<f32>>>>,
    ) -> SamplesBuffer<f32> {
        // TODO: Implement effects
        let sample_rate = mixer.sample_rate();
        let mixer_buffered = mixer.buffered();
        // let starting_length = mixer_buffered.clone().into_iter().count();
        // // let samples: Vec<f32> = mixer.buffered().take(length_of_smallest_buffer).collect();
        // let mut samples: Vec<f32> = Vec::new();
        // let mut left_over_samples: Vec<f32> = Vec::new();

        let vector_samples = mixer_buffered.clone().into_iter().collect::<Vec<f32>>();

        // let max = mixer_buffered.clone().max_by(|x, y| x.abs().total_cmp(&y.abs()));
        // println!("Max peak: {:?}", max);
        
        // let samples_with_reverb = apply_convolution_reverb(vector_samples);
        // let samples_with_reverb = simple_reverb(vector_samples, 22050, 0.5);

        // let mut index = 0;
        // for sample in vector_samples {
        //     index += 1;

        //     if index <= starting_length {
        //         samples.push(sample);
        //     } else {
        //         left_over_samples.push(sample);
        //     }
        // }

        // let mut effects_buffer_unlocked = effects_buffer.lock().unwrap();
        // for sample in left_over_samples {
        //     effects_buffer_unlocked.push(sample);
        // }
        // drop(effects_buffer_unlocked);

        SamplesBuffer::new(mixer_buffered.channels(), sample_rate, vector_samples)
    }

    pub fn get_duration(&self) -> f64 {
        let prot = self.prot.lock().unwrap();
        *prot.get_duration()
    }

    fn ready_buffer_map(&mut self, keys: &Vec<u32>) {
        self.buffer_map = init_buffer_map();

        let prot = self.prot.lock().unwrap();
        let sample_rate = prot.info.sample_rate;
        drop(prot);
        let buffer_size = sample_rate as usize * 1; // Ten seconds of audio at the sample rate

        for key in keys {
            let ring_buffer = Bounded::from(vec![0.0; buffer_size]);
            self.buffer_map
                .lock()
                .unwrap()
                .insert(*key as i32, ring_buffer);
        }
    }

    // pub fn abort(&self) {
    //     self.abort.store(true, Ordering::SeqCst);
    // }

    pub fn finished_buffering(&self) -> bool {
        let finished_tracks = self.finished_tracks.lock().unwrap();
        let prot = self.prot.lock().unwrap();
        let keys = prot.get_keys();
        drop(prot);

        for key in keys {
            if !finished_tracks.contains(&(key as i32)) {
                return false;
            }
        }

        true
    }

    // pub fn get_length(&self) -> usize {
    //     let prot = self.prot.lock().unwrap();
    //     prot.get_length()
    // }
}
