// Audio recording and processing

pub fn list_devices() -> Vec<String> {
    vec![]
}

pub struct Recorder;

impl Recorder {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&mut self) {
        // TODO
    }

    pub fn stop(&mut self) -> Vec<f32> {
        vec![]
    }
}
