fn model_base_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".mofa/models"))
        .unwrap_or_else(|| PathBuf::from("./models"))
}

fn choose_llm_model(base: &Path, choice: LlmModelChoice) -> Option<PathBuf> {
    if let Some(file_name) = choice.file_name() {
        let selected = base.join(file_name);
        if selected.exists() {
            return Some(selected);
        }
    }
    choose_llm_model_auto(base)
}

fn choose_llm_model_auto(base: &Path) -> Option<PathBuf> {
    let mem_gb = total_memory_gb().unwrap_or(32);

    let preferred = if mem_gb <= 8 {
        "qwen2.5-0.5b-q4_k_m.gguf"
    } else if mem_gb <= 16 {
        "qwen2.5-1.5b-q4_k_m.gguf"
    } else if mem_gb <= 24 {
        "qwen2.5-3b-q4_k_m.gguf"
    } else if mem_gb <= 40 {
        "qwen3-4b-q4_k_m.gguf"
    } else if mem_gb <= 64 {
        "qwen2.5-7b-q4_k_m.gguf"
    } else if mem_gb <= 96 {
        "qwen3-8b-q4_k_m.gguf"
    } else if mem_gb <= 128 {
        "qwen3-14b-q4_k_m.gguf"
    } else if mem_gb <= 192 {
        "qwen3-30b-a3b-q4_k_m.gguf"
    } else if mem_gb <= 256 {
        "qwen3-32b-q4_k_m.gguf"
    } else {
        "qwen2.5-72b-q4_k_m.gguf"
    };

    let mut candidates = vec![
        preferred,
        "qwen2.5-1.5b-q4_k_m.gguf",
        "qwen2.5-0.5b-q4_k_m.gguf",
        "qwen2.5-3b-q4_k_m.gguf",
        "qwen3-4b-q4_k_m.gguf",
        "qwen2.5-7b-q4_k_m.gguf",
        "qwen3-8b-q4_k_m.gguf",
        "qwen2.5-14b-q4_k_m.gguf",
        "qwen3-14b-q4_k_m.gguf",
        "qwen3-30b-a3b-q4_k_m.gguf",
        "qwen2.5-32b-q4_k_m.gguf",
        "qwen3-32b-q4_k_m.gguf",
        "qwen2.5-72b-q4_k_m.gguf",
        "qwen2.5-coder-1.5b-q4_k_m.gguf",
        "qwen2.5-coder-0.5b-q4_k_m.gguf",
        "qwen2.5-coder-3b-q4_k_m.gguf",
        "qwen2.5-coder-7b-q4_k_m.gguf",
        "qwen2.5-coder-14b-q4_k_m.gguf",
        "qwen2.5-coder-32b-q4_k_m.gguf",
    ];
    candidates.dedup();

    candidates
        .into_iter()
        .map(|name| base.join(name))
        .find(|p| p.exists())
}

fn choose_asr_model(base: &Path, choice: AsrModelChoice) -> Option<PathBuf> {
    if let Some(file_name) = choice.file_name() {
        let selected = base.join(file_name);
        if selected.exists() {
            return Some(selected);
        }
    }
    choose_asr_model_auto(base)
}

fn choose_asr_model_auto(base: &Path) -> Option<PathBuf> {
    [
        "ggml-small.bin",
        "ggml-base.bin",
        "ggml-tiny.bin",
        "ggml-medium.bin",
    ]
    .into_iter()
    .map(|name| base.join(name))
    .find(|p| p.exists())
}

fn normalize_transcript(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn audio_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples
        .iter()
        .map(|v| {
            let x = *v as f64;
            x * x
        })
        .sum::<f64>()
        / samples.len() as f64;
    mean_square.sqrt() as f32
}

fn build_refine_prompt(raw_text: &str) -> String {
    format!(
        "你是输入法润色器。将 ASR 文本整理为可直接发送的自然表达。\n\
规则：\n\
1) 保留原意与事实，不新增信息；\n\
2) 删除重复、卡顿与明显口吃，但保留自然语气词与语气助词（如“嗯”“啊”“呀”“吧”“哈”）以维持说话感；\n\
3) 专名、数字、代码、URL 原样保留；\n\
4) 若原文含英文/中英混合，尽量保留英文词形、大小写与常见短语，不强制翻译为中文；\n\
5) 若存在明显 ASR 误识（同音误字、语境不通），可基于上下文做最小必要纠正；若不确定，保留原词，不要臆造；\n\
6) 输出风格贴近华人程序员日常输入：中文主句可夹英文技术词（如 API、SDK、bug、PR、merge、deploy、rollback），读起来自然即可；\n\
7) 技术术语优先保留业界常用英文写法，不要生硬翻译；必要时可中英并置，但勿堆砌；\n\
8) 语气上可更口语、更有人味，但勿改变核心事实与意图；\n\
9) 若内容确为空，输出空字符串；\n\
10) 只输出最终文本，不解释、不提问。\n\n{}",
        raw_text
    )
}

fn total_memory_gb() -> Option<u64> {
    let name = CString::new("hw.memsize").ok()?;
    let mut value: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut value as *mut _ as *mut c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret == 0 {
        Some(value / 1024 / 1024 / 1024)
    } else {
        None
    }
}
