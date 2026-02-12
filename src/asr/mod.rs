// ASR engines: FunASR, Whisper

pub trait AsrEngine {
    fn transcribe(&mut self, audio: &[f32], sample_rate: u32) -> anyhow::Result<String>;
}

// TODO: Implement FunASR
// TODO: Implement Whisper (small, medium)
