//! Model catalog, downloads and on-disk management.

use futures_util::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub url: Option<String>,
    pub multilingual: bool,
    pub speed: u8,
    pub accuracy: u8,
    pub recommended: bool,
    pub quantized: bool,
    pub installed: bool,
    pub path: Option<String>,
    pub custom: bool,
    pub downloading: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub done: bool,
    pub error: Option<String>,
}

struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    file: &'static str,
    size_bytes: u64,
    multilingual: bool,
    speed: u8,
    accuracy: u8,
    quantized: bool,
}

const HF: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";
const HF_DISTIL: &str = "https://huggingface.co/distil-whisper/distil-large-v3-ggml/resolve/main/";

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry { id: "large-v3-turbo", name: "Whisper Large v3 Turbo", description: "Best overall: near large-v3 accuracy at ~8× the speed. Recommended on Apple Silicon or any GPU.", file: "ggml-large-v3-turbo.bin", size_bytes: 1_624_555_275, multilingual: true, speed: 4, accuracy: 5, quantized: false },
    CatalogEntry { id: "large-v3-turbo-q5_0", name: "Whisper Large v3 Turbo (Q5)", description: "Quantized turbo. Same accuracy class, one third the disk and memory.", file: "ggml-large-v3-turbo-q5_0.bin", size_bytes: 574_041_195, multilingual: true, speed: 4, accuracy: 5, quantized: true },
    CatalogEntry { id: "distil-large-v3", name: "Distil Whisper Large v3", description: "English-only distillation of large-v3. Very fast, excellent accuracy on English speech.", file: "ggml-distil-large-v3.bin", size_bytes: 1_516_000_000, multilingual: false, speed: 4, accuracy: 5, quantized: false },
    CatalogEntry { id: "large-v3", name: "Whisper Large v3", description: "Highest accuracy, slowest. Use when latency doesn't matter.", file: "ggml-large-v3.bin", size_bytes: 3_095_033_483, multilingual: true, speed: 1, accuracy: 5, quantized: false },
    CatalogEntry { id: "medium.en", name: "Whisper Medium (English)", description: "Strong English accuracy, moderate speed.", file: "ggml-medium.en.bin", size_bytes: 1_533_774_781, multilingual: false, speed: 2, accuracy: 4, quantized: false },
    CatalogEntry { id: "medium", name: "Whisper Medium", description: "Multilingual medium model.", file: "ggml-medium.bin", size_bytes: 1_533_763_059, multilingual: true, speed: 2, accuracy: 4, quantized: false },
    CatalogEntry { id: "small.en", name: "Whisper Small (English)", description: "Great balance for laptops without a GPU. Fast and accurate for clear speech.", file: "ggml-small.en.bin", size_bytes: 487_614_201, multilingual: false, speed: 4, accuracy: 3, quantized: false },
    CatalogEntry { id: "small", name: "Whisper Small", description: "Multilingual small model.", file: "ggml-small.bin", size_bytes: 487_601_967, multilingual: true, speed: 4, accuracy: 3, quantized: false },
    CatalogEntry { id: "base.en", name: "Whisper Base (English)", description: "Very fast, good for short commands and quick notes.", file: "ggml-base.en.bin", size_bytes: 147_964_211, multilingual: false, speed: 5, accuracy: 2, quantized: false },
    CatalogEntry { id: "base", name: "Whisper Base", description: "Multilingual base model.", file: "ggml-base.bin", size_bytes: 147_951_465, multilingual: true, speed: 5, accuracy: 2, quantized: false },
    CatalogEntry { id: "tiny.en", name: "Whisper Tiny (English)", description: "Smallest and fastest. Accuracy suffers on long or noisy audio.", file: "ggml-tiny.en.bin", size_bytes: 77_704_715, multilingual: false, speed: 5, accuracy: 1, quantized: false },
    CatalogEntry { id: "tiny", name: "Whisper Tiny", description: "Multilingual tiny model.", file: "ggml-tiny.bin", size_bytes: 77_691_713, multilingual: true, speed: 5, accuracy: 1, quantized: false },
];

fn url_for(entry: &CatalogEntry) -> String {
    if entry.id.starts_with("distil-") {
        format!("{HF_DISTIL}{}", entry.file)
    } else {
        format!("{HF}{}", entry.file)
    }
}

pub struct ModelManager {
    dir: PathBuf,
    downloads: Mutex<HashMap<String, CancellationToken>>,
}

impl ModelManager {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("custom"));
        Self { dir, downloads: Mutex::new(HashMap::new()) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn recommended_id(&self) -> &'static str {
        let mem_gb = {
            let mut s = sysinfo::System::new();
            s.refresh_memory();
            s.total_memory() as f64 / 1_073_741_824.0
        };
        let has_gpu = cfg!(any(target_os = "macos", feature = "cuda", feature = "vulkan"))
            || cfg!(target_os = "windows");
        if has_gpu && mem_gb >= 12.0 {
            "large-v3-turbo"
        } else if mem_gb >= 8.0 {
            "large-v3-turbo-q5_0"
        } else {
            "small.en"
        }
    }

    fn catalog_path(&self, entry: &CatalogEntry) -> PathBuf {
        self.dir.join(entry.file)
    }

    pub fn path_for(&self, id: &str) -> Option<PathBuf> {
        if let Some(entry) = CATALOG.iter().find(|e| e.id == id) {
            let p = self.catalog_path(entry);
            return p.exists().then_some(p);
        }
        if let Some(name) = id.strip_prefix("custom:") {
            let p = self.dir.join("custom").join(name);
            return p.exists().then_some(p);
        }
        None
    }

    pub fn list(&self) -> Vec<ModelInfo> {
        let recommended = self.recommended_id();
        let downloads = self.downloads.lock();
        let mut out: Vec<ModelInfo> = CATALOG
            .iter()
            .map(|e| {
                let path = self.catalog_path(e);
                let installed = path.exists();
                ModelInfo {
                    id: e.id.to_string(),
                    name: e.name.to_string(),
                    description: e.description.to_string(),
                    size_bytes: e.size_bytes,
                    url: Some(url_for(e)),
                    multilingual: e.multilingual,
                    speed: e.speed,
                    accuracy: e.accuracy,
                    recommended: e.id == recommended,
                    quantized: e.quantized,
                    installed,
                    path: installed.then(|| path.to_string_lossy().to_string()),
                    custom: false,
                    downloading: downloads.contains_key(e.id),
                }
            })
            .collect();

        if let Ok(rd) = std::fs::read_dir(self.dir.join("custom")) {
            let mut customs: Vec<ModelInfo> = rd
                .flatten()
                .filter(|d| d.path().is_file())
                .filter(|d| {
                    let n = d.file_name().to_string_lossy().to_lowercase();
                    n.ends_with(".bin") || n.ends_with(".gguf")
                })
                .map(|d| {
                    let name = d.file_name().to_string_lossy().to_string();
                    let size = d.metadata().map(|m| m.len()).unwrap_or(0);
                    ModelInfo {
                        id: format!("custom:{name}"),
                        name: name.trim_end_matches(".bin").trim_end_matches(".gguf").to_string(),
                        description: "Imported GGML model".into(),
                        size_bytes: size,
                        url: None,
                        multilingual: !name.contains(".en"),
                        speed: 3,
                        accuracy: 3,
                        recommended: false,
                        quantized: name.contains("-q"),
                        installed: true,
                        path: Some(d.path().to_string_lossy().to_string()),
                        custom: true,
                        downloading: false,
                    }
                })
                .collect();
            customs.sort_by(|a, b| a.name.cmp(&b.name));
            out.extend(customs);
        }
        out
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let path = self.path_for(id).ok_or_else(|| anyhow::anyhow!("model {id} is not installed"))?;
        std::fs::remove_file(path)?;
        Ok(())
    }

    pub fn import_custom(&self, source: &Path) -> anyhow::Result<ModelInfo> {
        anyhow::ensure!(source.is_file(), "{} is not a file", source.display());
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| anyhow::anyhow!("invalid file name"))?;
        let lower = name.to_lowercase();
        anyhow::ensure!(lower.ends_with(".bin") || lower.ends_with(".gguf"), "expected a .bin or .gguf GGML model");
        // Sanity check: GGML magic
        let mut magic = [0u8; 4];
        {
            use std::io::Read;
            let mut f = std::fs::File::open(source)?;
            f.read_exact(&mut magic)?;
        }
        anyhow::ensure!(
            &magic == b"lmgg" || &magic == b"GGUF" || &magic == b"ggml" || &magic == b"FUGG",
            "file does not look like a GGML/GGUF model"
        );
        let dest = self.dir.join("custom").join(&name);
        if dest != source {
            std::fs::copy(source, &dest)?;
        }
        let id = format!("custom:{name}");
        self.list()
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| anyhow::anyhow!("import failed"))
    }

    pub fn cancel_download(&self, id: &str) {
        if let Some(token) = self.downloads.lock().get(id) {
            token.cancel();
        }
    }

    /// Streams the model file to disk, emitting `model-download-progress`.
    pub async fn download(&self, app: AppHandle, id: String) -> anyhow::Result<()> {
        let entry = CATALOG
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("unknown model {id}"))?;
        let token = CancellationToken::new();
        {
            let mut d = self.downloads.lock();
            if d.contains_key(&id) {
                anyhow::bail!("already downloading");
            }
            d.insert(id.clone(), token.clone());
        }
        let _ = app.emit("models-changed", ());
        let result = self.download_inner(&app, entry, &id, token).await;
        self.downloads.lock().remove(&id);
        let _ = app.emit(
            "model-download-progress",
            DownloadProgress {
                id: id.clone(),
                downloaded: if result.is_ok() { entry.size_bytes } else { 0 },
                total: Some(entry.size_bytes),
                done: result.is_ok(),
                error: result.as_ref().err().map(|e| format!("{e:#}")),
            },
        );
        let _ = app.emit("models-changed", ());
        result
    }

    async fn download_inner(
        &self,
        app: &AppHandle,
        entry: &CatalogEntry,
        id: &str,
        token: CancellationToken,
    ) -> anyhow::Result<()> {
        let final_path = self.catalog_path(entry);
        let part_path = final_path.with_extension("bin.part");
        let client = reqwest::Client::builder()
            .user_agent(concat!("Mynah/", env!("CARGO_PKG_VERSION")))
            .build()?;

        let existing = tokio::fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);
        let mut req = client.get(url_for(entry));
        if existing > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let resp = req.send().await?.error_for_status()?;
        let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut downloaded = if resumed { existing } else { 0 };
        let total = resp
            .content_length()
            .map(|l| l + if resumed { existing } else { 0 })
            .or(Some(entry.size_bytes));

        let mut file = if resumed {
            tokio::fs::OpenOptions::new().append(true).open(&part_path).await?
        } else {
            tokio::fs::File::create(&part_path).await?
        };

        let mut stream = resp.bytes_stream();
        let mut last_emit = std::time::Instant::now();
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    file.flush().await.ok();
                    anyhow::bail!("cancelled");
                }
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            file.write_all(&bytes).await?;
                            downloaded += bytes.len() as u64;
                            if last_emit.elapsed().as_millis() > 120 {
                                last_emit = std::time::Instant::now();
                                let _ = app.emit("model-download-progress", DownloadProgress {
                                    id: id.to_string(), downloaded, total, done: false, error: None,
                                });
                            }
                        }
                        Some(Err(e)) => return Err(e.into()),
                        None => break,
                    }
                }
            }
        }
        file.flush().await?;
        drop(file);
        if let Some(t) = total {
            anyhow::ensure!(downloaded >= t, "download incomplete ({downloaded} of {t} bytes)");
        }
        tokio::fs::rename(&part_path, &final_path).await?;
        Ok(())
    }
}
