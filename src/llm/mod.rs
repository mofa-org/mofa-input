pub mod ffi;

use std::path::Path;
use std::sync::{Arc, Mutex};

/// Thread-safe wrapper for multi-turn conversations
#[derive(Clone)]
pub struct ChatSession {
    engine: Arc<Mutex<ffi::LlmEngine>>,
}

impl ChatSession {
    pub fn new(model_path: &Path) -> anyhow::Result<Self> {
        let engine = ffi::LlmEngine::new(model_path)?;
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
        })
    }

    /// Send message and get complete response
    pub fn send(&self, message: &str, max_tokens: i32, temperature: f32) -> anyhow::Result<String> {
        let engine = self.engine.lock().unwrap();
        engine.chat_add_user(message)?;
        engine.chat_respond(max_tokens, temperature)
    }

    /// Send message with streaming response
    pub fn send_stream<F>(&self, message: &str, max_tokens: i32, temperature: f32, callback: F)
    where
        F: Fn(&str) + Send + 'static,
    {
        let engine = self.engine.lock().unwrap();
        engine.chat_add_user(message).unwrap();
        engine.chat_respond_stream(max_tokens, temperature, callback);
    }

    /// Clear conversation history
    pub fn clear(&self) {
        let engine = self.engine.lock().unwrap();
        engine.chat_clear();
    }

    /// Get token count in KV cache
    pub fn token_count(&self) -> i32 {
        let engine = self.engine.lock().unwrap();
        engine.kv_count()
    }
}
