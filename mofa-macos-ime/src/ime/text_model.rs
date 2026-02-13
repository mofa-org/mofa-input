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
    } else {
        "qwen2.5-3b-q4_k_m.gguf"
    };

    let mut candidates = vec![
        preferred,
        "qwen2.5-1.5b-q4_k_m.gguf",
        "qwen2.5-0.5b-q4_k_m.gguf",
        "qwen2.5-3b-q4_k_m.gguf",
        "qwen2.5-7b-q4_k_m.gguf",
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

fn compact_for_filter(text: &str) -> String {
    text.chars()
        .filter(|c| {
            if c.is_whitespace() {
                return false;
            }
            !matches!(
                c,
                '，' | '。'
                    | '！'
                    | '？'
                    | '；'
                    | '：'
                    | '、'
                    | '（'
                    | '）'
                    | '【'
                    | '】'
                    | '.'
                    | ','
                    | '!'
                    | '?'
                    | ';'
                    | ':'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '"'
                    | '\''
                    | '“'
                    | '”'
                    | '…'
            )
        })
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_template_noise_text(text: &str) -> bool {
    let compact = compact_for_filter(text);
    if compact.is_empty() {
        return true;
    }
    const PATTERNS: [&str; 11] = [
        "好的请提供需要转写和润色的语音内容",
        "请提供需要转写和润色的语音内容",
        "请提供需要转写的语音内容",
        "请提供语音内容",
        "未检测到有效语音",
        "未识别到有效语音",
        "未识别到语音",
        "pleaseprovidevoiceinput",
        "pleaseprovidetheaudiocontent",
        "pleaseprovidevoicetotranscribe",
        "novalidaudio",
    ];
    PATTERNS.iter().any(|p| compact.contains(p))
}

fn should_drop_transcript(text: &str) -> bool {
    let normalized = normalize_transcript(text);
    if normalized.is_empty() {
        return true;
    }
    if is_template_noise_text(&normalized) {
        return true;
    }
    let compact = compact_for_filter(&normalized);
    compact.chars().count() <= 1
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

fn english_char_ratio(text: &str) -> f32 {
    let mut latin = 0usize;
    let mut total = 0usize;
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            latin += 1;
            total += 1;
        } else if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            total += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        latin as f32 / total as f32
    }
}

fn build_refine_prompt(raw_text: &str) -> String {
    if english_char_ratio(raw_text) >= 0.7 {
        format!(
            "You are an input-method text polisher. Rewrite the ASR text into natural, concise English ready to send.\n\
Rules:\n\
1) Keep the original meaning and key facts.\n\
2) Remove fillers, stutters, and duplicate fragments.\n\
3) Keep names, numbers, code terms, and URLs unchanged.\n\
4) Do not translate into Chinese.\n\
5) Do not explain, do not add commentary, and do not ask follow-up questions.\n\
6) If content is empty/invalid, output an empty string.\n\
Output only the final text.\n\n{}",
            raw_text
        )
    } else {
        format!(
            "你是输入法润色器。将 ASR 文本整理为可直接发送的自然中文。\n\
规则：\n\
1) 保留原意与事实，不新增信息；\n\
2) 删除口癖、重复、卡顿；\n\
3) 专名、数字、代码、URL 原样保留；\n\
4) 不解释、不寒暄、不提问；\n\
5) 若内容为空或无效，仅输出空字符串；\n\
6) 只输出最终文本。\n\n{}",
            raw_text
        )
    }
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
