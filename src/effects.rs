use std::io::BufReader;
use rustfft::{FftPlanner, num_complex::Complex};

use rodio::Decoder;

pub fn apply_convolution_reverb(input_signal: Vec<f32>) -> Vec<f32> {
    // Load your impulse response (IR) file
    // TODO: This should be done once, not every time the effect is applied
    let ir_signal = load_impulse_response("/Users/innocentsmith/Dev/tauri/proteus-author/dev-assets/Impulse Responses/IR.wav");

    // Perform the convolution here
    // This is a placeholder for actual convolution logic
    let convolved_signal = fft_convolution(&input_signal, &ir_signal);

    convolved_signal

    // input_signal
}

pub fn load_impulse_response(file_path: &str) -> Vec<f32> {
    let file = BufReader::new(std::fs::File::open(file_path).unwrap());
    let mut source = Decoder::new(file).unwrap();

    // println!("Sample rate: {:?}", source.sample_rate());
    // Load the WAV file and return its samples as a Vec<f32>
    // This function needs to be implemented

    let mut samples: Vec<f32> = Vec::new();

    while let Some(sample) = source.next() {
        println!("{:?}", sample);
        samples.push(sample as f32);
    }

    samples
}

pub fn convolution(_input_signal: &[f32], _ir_signal: &[f32]) -> Vec<f32> {
    // Implement the convolution algorithm
    // This could be direct convolution or FFT-based convolution

    Vec::new()
}

fn fft_convolution(input_signal: &[f32], ir_signal: &[f32]) -> Vec<f32> {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(input_signal.len() + ir_signal.len() - 1);
    let ifft = planner.plan_fft_inverse(input_signal.len() + ir_signal.len() - 1);

    let mut input_fft = input_signal.iter().map(|&f| Complex::new(f, 0.0)).collect::<Vec<_>>();
    let mut ir_fft = ir_signal.iter().map(|&f| Complex::new(f, 0.0)).collect::<Vec<_>>();

    // Zero-padding to the same length
    input_fft.resize(input_fft.len() + ir_signal.len() - 1, Complex::new(0.0, 0.0));
    ir_fft.resize(ir_fft.len() + input_signal.len() - 1, Complex::new(0.0, 0.0));

    // Perform FFT
    fft.process(&mut input_fft);
    fft.process(&mut ir_fft);

    // Multiply in frequency domain
    for (input, ir) in input_fft.iter_mut().zip(ir_fft.iter()) {
        *input = *input * *ir;
    }

    // Perform inverse FFT
    ifft.process(&mut input_fft);

    // Normalize and extract real part
    input_fft.clone().into_iter().map(|c| c.re / input_fft.len() as f32).collect()
}

pub fn simple_reverb(samples: Vec<f32>, delay_samples: usize, decay: f32) -> Vec<f32> {
    let mut processed = Vec::with_capacity(samples.len() + delay_samples);
    for i in 0..samples.len() {
        let delayed_index = i.checked_sub(delay_samples);
        let delayed_sample = delayed_index.and_then(|index| samples.get(index)).unwrap_or(&0.0) * decay;
        let current_sample = samples[i] + delayed_sample;
        processed.push(current_sample);
    }
    processed
}