#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LlmModel {
    Qwen05,
    Qwen15,
    Qwen3,
    Qwen7,
}

impl LlmModel {
    fn all() -> [Self; 4] {
        [Self::Qwen05, Self::Qwen15, Self::Qwen3, Self::Qwen7]
    }

    fn id(self) -> &'static str {
        match self {
            Self::Qwen05 => "llm:qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "llm:qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "llm:qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen7 => "llm:qwen2.5-7b-q4_k_m.gguf",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::Qwen05 => "qwen2.5-0.5b-q4_k_m.gguf",
            Self::Qwen15 => "qwen2.5-1.5b-q4_k_m.gguf",
            Self::Qwen3 => "qwen2.5-3b-q4_k_m.gguf",
            Self::Qwen7 => "qwen2.5-7b-q4_k_m.gguf",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Qwen05 => "Qwen2.5 0.5B",
            Self::Qwen15 => "Qwen2.5 1.5B",
            Self::Qwen3 => "Qwen2.5 3B",
            Self::Qwen7 => "Qwen2.5 7B",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Self::Qwen05 => "极省内存，低负载设备",
            Self::Qwen15 => "16GB 设备推荐档",
            Self::Qwen3 => "默认档，质量与速度平衡",
            Self::Qwen7 => "质量更高，需更大内存",
        }
    }

    fn size_mb(self) -> u64 {
        match self {
            Self::Qwen05 => 400,
            Self::Qwen15 => 900,
            Self::Qwen3 => 1900,
            Self::Qwen7 => 4400,
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::Qwen05 => "https://huggingface.co/lmstudio-community/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf",
            Self::Qwen15 => "https://huggingface.co/lmstudio-community/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf",
            Self::Qwen3 => "https://huggingface.co/lmstudio-community/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf",
            Self::Qwen7 => "https://huggingface.co/lmstudio-community/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
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
