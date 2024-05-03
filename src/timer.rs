use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Timer {
    pub time: f64,
    running: bool,
    start_time: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            time: 0.0,
            running: false,
            start_time: Instant::now(),
        }
    }

    pub fn start(&mut self) {
        self.start_time = Instant::now();
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.time += self.start_time.elapsed().as_secs_f64();
        self.running = false;
    }

    pub fn get_time(&self) -> f64 {
        if self.running {
            self.time + self.start_time.elapsed().as_secs_f64()
        } else {
            self.time
        }
    }

    pub fn reset(&mut self) {
        self.time = 0.0;
        self.running = false;
    }
}