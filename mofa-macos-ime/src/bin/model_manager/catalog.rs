#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LlmModel {
    Qwen05,
    Qwen15,
    Qwen3,
    Qwen4,
    Qwen7,
    Qwen8,
    Qwen14,
    Qwen14Q3,
    Qwen30A3B,
    Qwen32,
    Qwen32Q3,
    Qwen72,
    QwenCoder05,
    QwenCoder15,
    QwenCoder3,
    QwenCoder7,
    QwenCoder14,
    QwenCoder32,
}

impl LlmModel {
    fn all() -> [Self; 18] {
        [
            Self::Qwen05,
            Self::Qwen15,
            Self::Qwen3,
            Self::Qwen4,
            Self::Qwen7,
            Self::Qwen8,
            Self::Qwen14,
            Self::Qwen14Q3,
            Self::Qwen30A3B,
            Self::Qwen32,
            Self::Qwen32Q3,
            Self::Qwen72,
            Self::QwenCoder05,
            Self::QwenCoder15,
            Self::QwenCoder3,
            Self::QwenCoder7,
            Self::QwenCoder14,
            Self::QwenCoder32,
        ]
    }

    fn id(self) -> &'static str {
        match self {
            Self::Qwen05 => "llm:qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "llm:qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "llm:qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen4 => "llm:qwen3-4b-q4_k_m.gguf",
            Self::Qwen7 => "llm:qwen2.5-7b-q4_k_m.gguf",
            Self::Qwen8 => "llm:qwen3-8b-q4_k_m.gguf",
            Self::Qwen14 => "llm:qwen2.5-14b-q4_k_m.gguf",
            Self::Qwen14Q3 => "llm:qwen3-14b-q4_k_m.gguf",
            Self::Qwen30A3B => "llm:qwen3-30b-a3b-q4_k_m.gguf",
            Self::Qwen32 => "llm:qwen2.5-32b-q4_k_m.gguf",
            Self::Qwen32Q3 => "llm:qwen3-32b-q4_k_m.gguf",
            Self::Qwen72 => "llm:qwen2.5-72b-q4_k_m.gguf",
            Self::QwenCoder05 => "llm:qwen2.5-coder-0.5b-q4_k_m.gguf",
            Self::QwenCoder15 => "llm:qwen2.5-coder-1.5b-q4_k_m.gguf",
            Self::QwenCoder3 => "llm:qwen2.5-coder-3b-q4_k_m.gguf",
            Self::QwenCoder7 => "llm:qwen2.5-coder-7b-q4_k_m.gguf",
            Self::QwenCoder14 => "llm:qwen2.5-coder-14b-q4_k_m.gguf",
            Self::QwenCoder32 => "llm:qwen2.5-coder-32b-q4_k_m.gguf",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::Qwen05 => "qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen4 => "qwen3-4b-q4_k_m.gguf",
            Self::Qwen7 => "qwen2.5-7b-q4_k_m.gguf",
            Self::Qwen8 => "qwen3-8b-q4_k_m.gguf",
            Self::Qwen14 => "qwen2.5-14b-q4_k_m.gguf",
            Self::Qwen14Q3 => "qwen3-14b-q4_k_m.gguf",
            Self::Qwen30A3B => "qwen3-30b-a3b-q4_k_m.gguf",
            Self::Qwen32 => "qwen2.5-32b-q4_k_m.gguf",
            Self::Qwen32Q3 => "qwen3-32b-q4_k_m.gguf",
            Self::Qwen72 => "qwen2.5-72b-q4_k_m.gguf",
            Self::QwenCoder05 => "qwen2.5-coder-0.5b-q4_k_m.gguf",
            Self::QwenCoder15 => "qwen2.5-coder-1.5b-q4_k_m.gguf",
            Self::QwenCoder3 => "qwen2.5-coder-3b-q4_k_m.gguf",
            Self::QwenCoder7 => "qwen2.5-coder-7b-q4_k_m.gguf",
            Self::QwenCoder14 => "qwen2.5-coder-14b-q4_k_m.gguf",
            Self::QwenCoder32 => "qwen2.5-coder-32b-q4_k_m.gguf",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Qwen05 => "Qwen2.5 0.5B",
            Self::Qwen15 => "Qwen2.5 1.5B",
            Self::Qwen3 => "Qwen2.5 3B",
            Self::Qwen4 => "Qwen3 4B",
            Self::Qwen7 => "Qwen2.5 7B",
            Self::Qwen8 => "Qwen3 8B",
            Self::Qwen14 => "Qwen2.5 14B",
            Self::Qwen14Q3 => "Qwen3 14B",
            Self::Qwen30A3B => "Qwen3 30B-A3B",
            Self::Qwen32 => "Qwen2.5 32B",
            Self::Qwen32Q3 => "Qwen3 32B",
            Self::Qwen72 => "Qwen2.5 72B",
            Self::QwenCoder05 => "Qwen2.5-Coder 0.5B",
            Self::QwenCoder15 => "Qwen2.5-Coder 1.5B",
            Self::QwenCoder3 => "Qwen2.5-Coder 3B",
            Self::QwenCoder7 => "Qwen2.5-Coder 7B",
            Self::QwenCoder14 => "Qwen2.5-Coder 14B",
            Self::QwenCoder32 => "Qwen2.5-Coder 32B",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Self::Qwen05 => "极省内存，低负载设备",
            Self::Qwen15 => "16GB 设备推荐档",
            Self::Qwen3 => "默认档，质量与速度平衡",
            Self::Qwen4 => "Qwen3 轻量档，通用对话",
            Self::Qwen7 => "质量更高，需更大内存",
            Self::Qwen8 => "Qwen3 进阶档，质量更佳",
            Self::Qwen14 => "高质量档，内存需求高",
            Self::Qwen14Q3 => "Qwen3 高质量档",
            Self::Qwen30A3B => "MoE 档，效果强但更重",
            Self::Qwen32 => "高质量大模型，资源占用高",
            Self::Qwen32Q3 => "Qwen3 大模型，高质量",
            Self::Qwen72 => "超大模型，仅高配设备",
            Self::QwenCoder05 => "代码向轻量档",
            Self::QwenCoder15 => "代码向平衡档",
            Self::QwenCoder3 => "代码向默认档",
            Self::QwenCoder7 => "代码向进阶档",
            Self::QwenCoder14 => "代码向高质量档",
            Self::QwenCoder32 => "代码向大模型",
        }
    }

    fn size_mb(self) -> u64 {
        match self {
            Self::Qwen05 => 400,
            Self::Qwen15 => 900,
            Self::Qwen3 => 1900,
            Self::Qwen4 => 2500,
            Self::Qwen7 => 4400,
            Self::Qwen8 => 5030,
            Self::Qwen14 => 8990,
            Self::Qwen14Q3 => 9000,
            Self::Qwen30A3B => 18600,
            Self::Qwen32 => 19900,
            Self::Qwen32Q3 => 19800,
            Self::Qwen72 => 44000,
            Self::QwenCoder05 => 400,
            Self::QwenCoder15 => 900,
            Self::QwenCoder3 => 1900,
            Self::QwenCoder7 => 4400,
            Self::QwenCoder14 => 9000,
            Self::QwenCoder32 => 19900,
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::Qwen05 => "https://huggingface.co/lmstudio-community/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf",
            Self::Qwen15 => "https://huggingface.co/lmstudio-community/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
            Self::Qwen3 => "https://huggingface.co/lmstudio-community/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf",
            Self::Qwen4 => "https://huggingface.co/lmstudio-community/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
            Self::Qwen7 => "https://huggingface.co/lmstudio-community/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
            Self::Qwen8 => "https://huggingface.co/lmstudio-community/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
            Self::Qwen14 => "https://huggingface.co/lmstudio-community/Qwen2.5-14B-Instruct-GGUF/resolve/main/Qwen2.5-14B-Instruct-Q4_K_M.gguf",
            Self::Qwen14Q3 => "https://huggingface.co/lmstudio-community/Qwen3-14B-GGUF/resolve/main/Qwen3-14B-Q4_K_M.gguf",
            Self::Qwen30A3B => "https://huggingface.co/lmstudio-community/Qwen3-30B-A3B-GGUF/resolve/main/Qwen3-30B-A3B-Q4_K_M.gguf",
            Self::Qwen32 => "https://huggingface.co/lmstudio-community/Qwen2.5-32B-Instruct-GGUF/resolve/main/Qwen2.5-32B-Instruct-Q4_K_M.gguf",
            Self::Qwen32Q3 => "https://huggingface.co/lmstudio-community/Qwen3-32B-GGUF/resolve/main/Qwen3-32B-Q4_K_M.gguf",
            Self::Qwen72 => "https://huggingface.co/lmstudio-community/Qwen2.5-72B-Instruct-GGUF/resolve/main/Qwen2.5-72B-Instruct-Q4_K_M.gguf",
            Self::QwenCoder05 => "https://huggingface.co/lmstudio-community/Qwen2.5-Coder-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-Coder-0.5B-Instruct-Q4_K_M.gguf",
            Self::QwenCoder15 => "https://huggingface.co/lmstudio-community/Qwen2.5-Coder-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-Coder-1.5B-Instruct-Q4_K_M.gguf",
            Self::QwenCoder3 => "https://huggingface.co/lmstudio-community/Qwen2.5-Coder-3B-Instruct-GGUF/resolve/main/Qwen2.5-Coder-3B-Instruct-Q4_K_M.gguf",
            Self::QwenCoder7 => "https://huggingface.co/lmstudio-community/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf",
            Self::QwenCoder14 => "https://huggingface.co/lmstudio-community/Qwen2.5-Coder-14B-Instruct-GGUF/resolve/main/Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf",
            Self::QwenCoder32 => "https://huggingface.co/lmstudio-community/Qwen2.5-Coder-32B-Instruct-GGUF/resolve/main/Qwen2.5-Coder-32B-Instruct-Q4_K_M.gguf",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AsrModel {
    WhisperSmall,
    WhisperBase,
    WhisperTiny,
    WhisperMedium,
}

impl AsrModel {
    fn all() -> [Self; 4] {
        [
            Self::WhisperSmall,
            Self::WhisperBase,
            Self::WhisperTiny,
            Self::WhisperMedium,
        ]
    }

    fn id(self) -> &'static str {
        match self {
            Self::WhisperTiny => "asr:ggml-tiny.bin",
            Self::WhisperBase => "asr:ggml-base.bin",
            Self::WhisperSmall => "asr:ggml-small.bin",
            Self::WhisperMedium => "asr:ggml-medium.bin",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::WhisperTiny => "ggml-tiny.bin",
            Self::WhisperBase => "ggml-base.bin",
            Self::WhisperSmall => "ggml-small.bin",
            Self::WhisperMedium => "ggml-medium.bin",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::WhisperTiny => "Whisper Tiny",
            Self::WhisperBase => "Whisper Base",
            Self::WhisperSmall => "Whisper Small",
            Self::WhisperMedium => "Whisper Medium",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Self::WhisperTiny => "最快，精度较低",
            Self::WhisperBase => "速度与精度平衡",
            Self::WhisperSmall => "当前主流程默认",
            Self::WhisperMedium => "精度更高，体积大",
        }
    }

    fn size_mb(self) -> u64 {
        match self {
            Self::WhisperTiny => 72,
            Self::WhisperBase => 142,
            Self::WhisperSmall => 466,
            Self::WhisperMedium => 1500,
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::WhisperTiny => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
            Self::WhisperBase => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            Self::WhisperSmall => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
            Self::WhisperMedium => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        }
    }
}

#[derive(Debug, Clone)]
struct ModelEntry {
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    file_name: &'static str,
    url: &'static str,
    size_mb: u64,
}

impl ModelEntry {
    fn path(&self, base: &Path) -> PathBuf {
        base.join(self.file_name)
    }
}

fn llm_entries() -> Vec<ModelEntry> {
    LlmModel::all()
        .into_iter()
        .map(|m| ModelEntry {
            id: m.id(),
            name: m.name(),
            desc: m.desc(),
            file_name: m.file_name(),
            url: m.url(),
            size_mb: m.size_mb(),
        })
        .collect()
}

fn asr_entries() -> Vec<ModelEntry> {
    AsrModel::all()
        .into_iter()
        .map(|m| ModelEntry {
            id: m.id(),
            name: m.name(),
            desc: m.desc(),
            file_name: m.file_name(),
            url: m.url(),
            size_mb: m.size_mb(),
        })
        .collect()
}
