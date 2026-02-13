enum DownloadEvent {
    Progress {
        id: String,
        progress: f32,
        downloaded_mb: f64,
    },
    Done {
        id: String,
    },
    Error {
        id: String,
        message: String,
    },
}

fn download_url_candidates(primary: &str) -> Vec<String> {
    let mut urls = vec![primary.to_string()];
    let hf_prefix = "https://huggingface.co/";
    if let Some(rest) = primary.strip_prefix(hf_prefix) {
        if let Ok(custom_mirror) = std::env::var("MOFA_HF_MIRROR") {
            let mirror = custom_mirror.trim().trim_end_matches('/');
            if !mirror.is_empty() {
                urls.push(format!("{mirror}/{rest}"));
            }
        }
        urls.push(format!("https://hf-mirror.com/{rest}"));
    }

    let mut deduped = Vec::new();
    for url in urls {
        if !deduped.contains(&url) {
            deduped.push(url);
        }
    }
    deduped
}

fn do_download(entry: &ModelEntry, model_dir: &Path, tx: &Sender<DownloadEvent>) -> Result<()> {
    fs::create_dir_all(model_dir).context("创建模型目录失败")?;

    let path = entry.path(model_dir);
    let tmp_path = path.with_extension(format!("{}.part", entry.file_name));

    if tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("mofa-macos-ime/0.1")
        .build()
        .context("初始化下载客户端失败")?;

    let mut last_err: Option<anyhow::Error> = None;
    for url in download_url_candidates(entry.url) {
        if tmp_path.exists() {
            let _ = fs::remove_file(&tmp_path);
        }

        let mut resp = match client
            .get(&url)
            .send()
            .with_context(|| format!("请求失败: {url}"))
        {
            Ok(resp) => resp,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };

        if !resp.status().is_success() {
            last_err = Some(anyhow::anyhow!("HTTP {}: {url}", resp.status()));
            continue;
        }

        let total = resp
            .content_length()
            .unwrap_or(entry.size_mb * 1024 * 1024)
            .max(1);

        let mut out = match File::create(&tmp_path)
            .with_context(|| format!("创建文件失败: {}", tmp_path.display()))
        {
            Ok(out) => out,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };

        let mut downloaded: u64 = 0;
        let mut buf = [0u8; 64 * 1024];
        let mut stream_error = None;

        loop {
            let n = match resp.read(&mut buf).context("下载流读取失败") {
                Ok(n) => n,
                Err(e) => {
                    stream_error = Some(e);
                    break;
                }
            };
            if n == 0 {
                break;
            }

            if let Err(e) = out.write_all(&buf[..n]).context("写入模型文件失败") {
                stream_error = Some(e);
                break;
            }
            downloaded += n as u64;

            let percent = ((downloaded as f64 / total as f64) * 100.0).min(100.0) as f32;
            let downloaded_mb = downloaded as f64 / 1024.0 / 1024.0;

            let _ = tx.send(DownloadEvent::Progress {
                id: entry.id.to_string(),
                progress: percent,
                downloaded_mb,
            });
        }

        if let Some(e) = stream_error {
            last_err = Some(e.context(format!("下载失败: {url}")));
            continue;
        }

        if let Err(e) = out.flush().context("刷新模型文件失败") {
            last_err = Some(e);
            continue;
        }

        fs::rename(&tmp_path, &path).with_context(|| {
            format!(
                "重命名临时文件失败: {} -> {}",
                tmp_path.display(),
                path.display()
            )
        })?;

        let _ = tx.send(DownloadEvent::Done {
            id: entry.id.to_string(),
        });
        return Ok(());
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("下载失败: 未找到可用下载源")))
}
