use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_family = "unix")]
use std::os::unix::fs as unix_fs;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let demo_cpp = manifest_dir.join("../demo-app/cpp");
    let local_cpp = manifest_dir.join("cpp");

    println!("cargo:rerun-if-changed=build.rs");

    if !demo_cpp.exists() {
        panic!("demo-app/cpp 不存在: {}", demo_cpp.display());
    }

    ensure_dir_link(&local_cpp, &demo_cpp);

    // Make runtime dylibs visible for `cargo run` binaries under target/{profile}
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let target_bin_dir = manifest_dir.join("target").join(profile);
    let _ = fs::create_dir_all(&target_bin_dir);

    let dylibs = [
        demo_cpp.join("build/llama.cpp/src/libllama.dylib"),
        demo_cpp.join("build/llama.cpp/ggml/src/libggml.dylib"),
        demo_cpp.join("build/llama.cpp/ggml/src/libggml-base.dylib"),
        demo_cpp.join("build/llama.cpp/ggml/src/libggml-cpu.dylib"),
        demo_cpp.join("build/llama.cpp/ggml/src/ggml-metal/libggml-metal.dylib"),
        demo_cpp.join("build/llama.cpp/ggml/src/ggml-blas/libggml-blas.dylib"),
    ];

    for src in dylibs {
        if src.exists() {
            let dst = target_bin_dir.join(src.file_name().unwrap());
            ensure_file_link(&dst, &src);
        }
    }
}

fn ensure_dir_link(link: &Path, target: &Path) {
    if link.exists() {
        return;
    }

    #[cfg(target_family = "unix")]
    {
        if let Err(e) = unix_fs::symlink(target, link) {
            panic!(
                "创建目录软链失败: {} -> {} ({e})",
                link.display(),
                target.display()
            );
        }
    }

    #[cfg(not(target_family = "unix"))]
    {
        let _ = (link, target);
    }
}

fn ensure_file_link(link: &Path, target: &Path) {
    if link.exists() {
        return;
    }

    #[cfg(target_family = "unix")]
    {
        if let Err(e) = unix_fs::symlink(target, link) {
            panic!(
                "创建文件软链失败: {} -> {} ({e})",
                link.display(),
                target.display()
            );
        }
    }

    #[cfg(not(target_family = "unix"))]
    {
        let _ = (link, target);
    }
}
