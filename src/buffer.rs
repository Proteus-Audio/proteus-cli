
use dasp_ring_buffer::Bounded;
use std::{collections::HashMap, sync::{Arc, Mutex}};
use crate::constants;

pub fn init_hash_buffer(keys: &Vec<u32>, sample_rate: Option<usize>) -> Arc<Mutex<HashMap<u32, Bounded<Vec<f32>>>>> {
    // // Named object for storing decoded audio samples for each of the tracks in the array.
    let track_buffers: Arc<Mutex<HashMap<u32, Bounded<Vec<f32>>>>> = Arc::new(Mutex::new(HashMap::new()));

    let sample_rate = sample_rate.unwrap_or(constants::SAMPLE_RATE);
    let buffer_size = sample_rate * 1; // Ten seconds of audio at the sample rate

    for key in keys {
        let ring_buffer = Bounded::from(vec![0.0; buffer_size]);
        track_buffers.lock().unwrap().insert(*key, ring_buffer);
    }

    track_buffers
}

pub fn buffer_remaining_space(track_buffers: &Arc<Mutex<HashMap<u32, Bounded<Vec<f32>>>>>, track_id: u32) -> usize {
    let track_buffers = track_buffers.lock().unwrap();
    let track_buffer = track_buffers.get(&track_id).unwrap();
    let remaining_space = track_buffer.max_len() - track_buffer.len();
    drop(track_buffers);
    remaining_space
}