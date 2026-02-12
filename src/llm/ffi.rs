use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString};
use std::path::Path;
use std::sync::Arc;

pub struct LlmEngine {
    ctx: *mut c_void,
}

// SAFETY: llama.cpp is thread-safe for single context
unsafe impl Send for LlmEngine {}
unsafe impl Sync for LlmEngine {}

#[link(name = "llm_server", kind = "static")]
extern "C" {
    fn llm_init(model_path: *const c_char) -> *mut c_void;
    fn llm_free(ctx: *mut c_void);

    fn llm_generate(ctx: *mut c_void, prompt: *const c_char, max_tokens: c_int, temperature: c_float) -> *mut c_char;
    fn llm_generate_stream(ctx: *mut c_void, prompt: *const c_char, max_tokens: c_int, temperature: c_float,
                           callback: extern "C" fn(*const c_char, *mut c_void), user_data: *mut c_void);
    fn llm_free_string(s: *mut c_char);

    fn llm_kv_clear(ctx: *mut c_void);
    fn llm_kv_count(ctx: *mut c_void) -> c_int;

    fn llm_chat_add_user(ctx: *mut c_void, message: *const c_char);
    fn llm_chat_respond(ctx: *mut c_void, max_tokens: c_int, temperature: c_float) -> *mut c_char;
    fn llm_chat_respond_stream(ctx: *mut c_void, max_tokens: c_int, temperature: c_float,
                                callback: extern "C" fn(*const c_char, *mut c_void), user_data: *mut c_void);
    fn llm_chat_clear(ctx: *mut c_void);
}

extern "C" fn token_callback(token: *const c_char, user_data: *mut c_void) {
    unsafe {
        let callback = &*(user_data as *mut Box<dyn Fn(&str) + Send>);
        let s = CStr::from_ptr(token).to_string_lossy();
        callback(&s);
    }
}

impl LlmEngine {
    pub fn new(model_path: &Path) -> anyhow::Result<Self> {
        let path_str = CString::new(model_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?)?;
        let ctx = unsafe { llm_init(path_str.as_ptr()) };
        if ctx.is_null() {
            return Err(anyhow::anyhow!("Failed to initialize LLM"));
        }
        Ok(Self { ctx })
    }

    pub fn generate(&self, prompt: &str, max_tokens: i32, temperature: f32) -> anyhow::Result<String> {
        let c_prompt = CString::new(prompt)?;
        let result = unsafe { llm_generate(self.ctx, c_prompt.as_ptr(), max_tokens, temperature) };
        if result.is_null() {
            return Err(anyhow::anyhow!("Generation failed"));
        }
        let s = unsafe { CStr::from_ptr(result).to_string_lossy().into_owned() };
        unsafe { llm_free_string(result) };
        Ok(s)
    }

    pub fn generate_stream<F>(&self, prompt: &str, max_tokens: i32, temperature: f32, callback: F)
    where
        F: Fn(&str) + Send + 'static,
    {
        let c_prompt = CString::new(prompt).unwrap();
        let mut cb: Box<dyn Fn(&str) + Send> = Box::new(callback);
        unsafe {
            llm_generate_stream(
                self.ctx,
                c_prompt.as_ptr(),
                max_tokens,
                temperature,
                token_callback,
                &mut cb as *mut _ as *mut c_void,
            );
        }
    }

    // ===== Multi-turn chat =====

    pub fn chat_add_user(&self, message: &str) -> anyhow::Result<()> {
        let c_msg = CString::new(message)?;
        unsafe { llm_chat_add_user(self.ctx, c_msg.as_ptr()) };
        Ok(())
    }

    pub fn chat_respond(&self, max_tokens: i32, temperature: f32) -> anyhow::Result<String> {
        let result = unsafe { llm_chat_respond(self.ctx, max_tokens, temperature) };
        if result.is_null() {
            return Err(anyhow::anyhow!("Chat response failed"));
        }
        let s = unsafe { CStr::from_ptr(result).to_string_lossy().into_owned() };
        unsafe { llm_free_string(result) };
        Ok(s)
    }

    pub fn chat_respond_stream<F>(&self, max_tokens: i32, temperature: f32, callback: F)
    where
        F: Fn(&str) + Send + 'static,
    {
        let mut cb: Box<dyn Fn(&str) + Send> = Box::new(callback);
        unsafe {
            llm_chat_respond_stream(
                self.ctx,
                max_tokens,
                temperature,
                token_callback,
                &mut cb as *mut _ as *mut c_void,
            );
        }
    }

    pub fn chat_clear(&self) {
        unsafe { llm_chat_clear(self.ctx) };
    }

    pub fn kv_count(&self) -> i32 {
        unsafe { llm_kv_count(self.ctx) }
    }
}

impl Drop for LlmEngine {
    fn drop(&mut self) {
        unsafe { llm_free(self.ctx) };
    }
}
