//! Managed runtime llama.cpp: portable-установка рядом с exe приложения.
//!
//! Скачивает официальные бинарники с GitHub Releases (ggml-org/llama.cpp),
//! распаковывает в `{exe_dir}/runtime/{tag}/{backend}/`, пишет `llama_dir`.
//!
//! Выбор backend (Windows x64):
//!   NVIDIA → CUDA 12.4 (+ cudart DLLs)
//!   иначе GPU → Vulkan
//!   без GPU → CPU
//!
//! Portable: всё лежит рядом с программой — можно носить на флешке.

use crate::hardware::detect_hardware;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use tokio::fs::File as TokioFile;
use tokio::io::AsyncWriteExt;

const GH: &str = "https://github.com/ggml-org/llama.cpp";
const GH_API: &str = "https://api.github.com/repos/ggml-org/llama.cpp";
const UA: &str = concat!("LlamaLauncher/", env!("CARGO_PKG_VERSION"));
const EMIT_STEP: u64 = 2_000_000;
const SERVER_EXE: &str = "llama-server.exe";

// ── Типы ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeBackend {
    Cpu,
    Vulkan,
    Cuda12,
}

impl RuntimeBackend {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Vulkan => "vulkan",
            Self::Cuda12 => "cuda-12.4",
        }
    }

    pub fn label_ru(&self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Vulkan => "Vulkan (GPU)",
            Self::Cuda12 => "CUDA 12.4 (NVIDIA)",
        }
    }

    fn from_id(s: &str) -> Option<Self> {
        match s {
            "cpu" => Some(Self::Cpu),
            "vulkan" => Some(Self::Vulkan),
            "cuda-12.4" | "cuda12" | "cuda" => Some(Self::Cuda12),
            _ => None,
        }
    }
}

/// Статус managed-runtime для UI.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    /// Папка с llama-server.exe (если уже стоит).
    pub llama_dir: Option<String>,
    pub installed: bool,
    pub tag: Option<String>,
    pub backend: Option<String>,
    pub backend_label: Option<String>,
    /// Рекомендуемый backend под текущее железо.
    pub recommended_backend: String,
    pub recommended_label: String,
    /// Portable-корень приложения (рядом с exe).
    pub app_dir: String,
    /// Дефолтная папка моделей `{app_dir}/models`.
    pub default_models_dir: String,
    /// Корень runtime `{app_dir}/runtime`.
    pub runtime_root: String,
}

/// Событие `runtime-progress`.
#[derive(Debug, Clone, Serialize)]
struct Progress {
    /// Человекочитаемый этап: «Скачиваю…», «Распаковываю…».
    stage: String,
    file: String,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
    canceled: bool,
}

#[derive(Default)]
pub struct RuntimeInstallState {
    cancel: AtomicBool,
    active: Mutex<bool>,
}

impl RuntimeInstallState {
    fn lock_active(&self) -> std::sync::MutexGuard<'_, bool> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }
}

enum DlErr {
    Canceled,
    Failed(String),
}

// ── Пути (portable) ──────────────────────────────────────────────────────────

/// Каталог, где лежит exe приложения. В dev — `target/debug`, в release — папка программы.
pub fn app_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Не удалось определить путь к программе: {e}"))?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "Некорректный путь к программе".into())
}

pub fn runtime_root() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("runtime"))
}

pub fn default_models_dir() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("models"))
}

/// `{runtime}/{tag}/{backend_id}/`
fn backend_dir(tag: &str, backend: &RuntimeBackend) -> Result<PathBuf, String> {
    Ok(runtime_root()?.join(tag).join(backend.id()))
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("Не удалось создать «{}»: {e}", path.display()))
}

/// Найти llama-server.exe в дереве (после распаковки zip может быть вложенная папка).
fn find_server_exe(root: &Path) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    let direct = root.join(SERVER_EXE);
    if direct.is_file() {
        return Some(direct);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for ent in entries.flatten() {
            let p = ent.path();
            if p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(SERVER_EXE))
            {
                return Some(p);
            }
            if p.is_dir() {
                stack.push(p);
            }
        }
    }
    None
}

fn is_installed_at(dir: &Path) -> bool {
    dir.join(SERVER_EXE).is_file()
}

// ── Выбор backend ────────────────────────────────────────────────────────────

fn recommend_backend() -> RuntimeBackend {
    let hw = detect_hardware();
    match hw.gpu {
        Some(g) => {
            let name = g.name.to_lowercase();
            if name.contains("nvidia") || name.contains("geforce") || name.contains("rtx") || name.contains("gtx") || name.contains("quadro") || name.contains("tesla") {
                RuntimeBackend::Cuda12
            } else if g.vram_bytes > 0 {
                // AMD / Intel / прочие с VRAM — Vulkan.
                RuntimeBackend::Vulkan
            } else {
                RuntimeBackend::Cpu
            }
        }
        None => RuntimeBackend::Cpu,
    }
}

// ── GitHub release ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(UA)
        // GitHub иногда долго отдаёт редиректы на objects.githubusercontent.com.
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("Не удалось создать HTTP-клиент: {e}"))
}

async fn fetch_latest_release() -> Result<GhRelease, String> {
    let url = format!("{GH_API}/releases/latest");
    let resp = client()?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Не удалось связаться с GitHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "GitHub вернул {} при запросе релизов llama.cpp",
            resp.status()
        ));
    }
    resp.json::<GhRelease>()
        .await
        .map_err(|e| format!("Не удалось разобрать ответ GitHub: {e}"))
}

/// Имена asset'ов для backend (основной zip + опциональный cudart).
fn asset_names(tag: &str, backend: &RuntimeBackend) -> (String, Option<String>) {
    match backend {
        RuntimeBackend::Cpu => (format!("llama-{tag}-bin-win-cpu-x64.zip"), None),
        RuntimeBackend::Vulkan => (format!("llama-{tag}-bin-win-vulkan-x64.zip"), None),
        RuntimeBackend::Cuda12 => (
            format!("llama-{tag}-bin-win-cuda-12.4-x64.zip"),
            Some("cudart-llama-bin-win-cuda-12.4-x64.zip".into()),
        ),
    }
}

fn find_asset<'a>(release: &'a GhRelease, name: &str) -> Result<&'a GhAsset, String> {
    release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| {
            format!(
                "В релизе {} нет файла «{name}». Попробуйте другой backend или позже.",
                release.tag_name
            )
        })
}

// ── Прогресс / скачивание ────────────────────────────────────────────────────

fn emit(
    app: &AppHandle,
    stage: &str,
    file: &str,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
    canceled: bool,
) {
    let _ = app.emit(
        "runtime-progress",
        Progress {
            stage: stage.to_string(),
            file: file.to_string(),
            downloaded,
            total,
            done,
            error,
            canceled,
        },
    );
}

async fn stream_to_file(
    app: &AppHandle,
    state: &RuntimeInstallState,
    url: &str,
    dest: &Path,
    stage: &str,
    label: &str,
) -> Result<u64, DlErr> {
    if let Some(parent) = dest.parent() {
        ensure_dir(parent).map_err(DlErr::Failed)?;
    }

    let mut resp = client()
        .map_err(DlErr::Failed)?
        .get(url)
        .send()
        .await
        .map_err(|e| DlErr::Failed(format!("Сеть недоступна: {e}")))?;

    if !resp.status().is_success() {
        return Err(DlErr::Failed(format!(
            "Сервер вернул {} при скачивании {label}",
            resp.status()
        )));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut out = TokioFile::create(dest)
        .await
        .map_err(|e| DlErr::Failed(format!("Не удалось создать файл: {e}")))?;

    let mut downloaded = 0u64;
    let mut last_emit = 0u64;
    emit(app, stage, label, 0, total, false, None, false);

    loop {
        if state.cancel.load(Ordering::SeqCst) {
            let _ = out.flush().await;
            let _ = tokio::fs::remove_file(dest).await;
            return Err(DlErr::Canceled);
        }
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| DlErr::Failed(format!("Ошибка при скачивании: {e}")))?;
        let Some(chunk) = chunk else { break };
        out.write_all(&chunk)
            .await
            .map_err(|e| DlErr::Failed(format!("Ошибка записи: {e}")))?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit >= EMIT_STEP {
            last_emit = downloaded;
            emit(app, stage, label, downloaded, total, false, None, false);
        }
    }
    out.flush()
        .await
        .map_err(|e| DlErr::Failed(format!("Ошибка сброса на диск: {e}")))?;

    if total > 0 && downloaded != total {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(DlErr::Failed(format!(
            "Файл скачан не полностью: {downloaded} из {total} байт"
        )));
    }
    emit(app, stage, label, downloaded, downloaded.max(total), false, None, false);
    Ok(downloaded)
}

// ── Распаковка ───────────────────────────────────────────────────────────────

/// Распаковать zip в dest, схлопывая путь до каталога, где лежит llama-server.exe
/// (или просто все файлы, если exe нет — как у cudart).
fn extract_zip(zip_path: &Path, dest: &Path, merge: bool) -> Result<(), String> {
    if !merge && dest.exists() {
        std::fs::remove_dir_all(dest)
            .map_err(|e| format!("Не удалось очистить «{}»: {e}", dest.display()))?;
    }
    ensure_dir(dest)?;

    let file = File::open(zip_path).map_err(|e| format!("Не удалось открыть zip: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Повреждённый zip-архив: {e}"))?;

    // Сначала во временную папку рядом.
    let tmp = dest.with_extension("extracting");
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    ensure_dir(&tmp)?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Ошибка чтения zip: {e}"))?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| "Небезопасный путь внутри zip".to_string())?
            .to_path_buf();

        let out_path = tmp.join(&name);
        if entry.is_dir() {
            ensure_dir(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            ensure_dir(parent)?;
        }
        let mut outfile =
            File::create(&out_path).map_err(|e| format!("Не удалось создать файл: {e}"))?;
        io::copy(&mut entry, &mut outfile).map_err(|e| format!("Ошибка распаковки: {e}"))?;
    }

    // Если есть llama-server.exe — копируем его каталог; иначе всё содержимое tmp.
    let source = if let Some(exe) = find_server_exe(&tmp) {
        exe.parent()
            .ok_or_else(|| "Некорректный путь к llama-server.exe".to_string())?
            .to_path_buf()
    } else {
        // cudart: файлы могут быть в корне или в одной вложенной папке.
        flatten_single_subdir(&tmp)
    };

    copy_dir_contents(&source, dest)?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

fn flatten_single_subdir(root: &Path) -> PathBuf {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    // Один подкаталог и ничего больше на верхнем уровне → спуститься.
    if entries.len() == 1 && entries[0].is_dir() {
        return entries.swap_remove(0);
    }
    root.to_path_buf()
}

fn copy_dir_contents(src: &Path, dest: &Path) -> Result<(), String> {
    ensure_dir(dest)?;
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for ent in std::fs::read_dir(&dir).map_err(|e| format!("read_dir: {e}"))? {
            let ent = ent.map_err(|e| format!("read_dir entry: {e}"))?;
            let p = ent.path();
            let rel = p.strip_prefix(src).unwrap_or(&p);
            let target = dest.join(rel);
            if p.is_dir() {
                ensure_dir(&target)?;
                stack.push(p);
            } else {
                if let Some(parent) = target.parent() {
                    ensure_dir(parent)?;
                }
                std::fs::copy(&p, &target)
                    .map_err(|e| format!("copy {} → {}: {e}", p.display(), target.display()))?;
            }
        }
    }
    Ok(())
}

// ── Статус / установка ───────────────────────────────────────────────────────

/// Прочитать managed-метаданные, если есть.
fn read_meta(dir: &Path) -> Option<(String, String)> {
    let meta_path = dir.join("ll-runtime.json");
    let s = std::fs::read_to_string(meta_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    Some((
        v.get("tag")?.as_str()?.to_string(),
        v.get("backend")?.as_str()?.to_string(),
    ))
}

fn write_meta(dir: &Path, tag: &str, backend: &RuntimeBackend) -> Result<(), String> {
    let meta = serde_json::json!({
        "tag": tag,
        "backend": backend.id(),
        "source": GH,
    });
    std::fs::write(
        dir.join("ll-runtime.json"),
        serde_json::to_string_pretty(&meta).unwrap_or_default(),
    )
    .map_err(|e| format!("Не удалось записать метаданные runtime: {e}"))
}

/// Найти уже установленный managed runtime (обход runtime/*/*).
fn find_existing_install() -> Option<(PathBuf, String, String)> {
    let root = runtime_root().ok()?;
    if !root.is_dir() {
        return None;
    }
    // Предпочитаем самый свежий по mtime каталог с exe.
    let mut best: Option<(PathBuf, String, String, std::time::SystemTime)> = None;
    let tags = std::fs::read_dir(&root).ok()?;
    for tag_ent in tags.flatten() {
        if !tag_ent.path().is_dir() {
            continue;
        }
        let tag_name = tag_ent.file_name().to_string_lossy().to_string();
        let backends = std::fs::read_dir(tag_ent.path()).ok();
        let Some(backends) = backends else { continue };
        for be in backends.flatten() {
            let dir = be.path();
            if !is_installed_at(&dir) {
                continue;
            }
            let backend_id = if let Some((_, b)) = read_meta(&dir) {
                b
            } else {
                be.file_name().to_string_lossy().to_string()
            };
            let mtime = dir
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let cand = (dir, tag_name.clone(), backend_id, mtime);
            best = match best {
                Some(b) if b.3 >= cand.3 => Some(b),
                _ => Some(cand),
            };
        }
    }
    best.map(|(p, t, b, _)| (p, t, b))
}

fn build_status() -> Result<RuntimeStatus, String> {
    let app = app_dir()?;
    let models = default_models_dir()?;
    let rt = runtime_root()?;
    let recommended = recommend_backend();

    let (llama_dir, installed, tag, backend, backend_label) =
        if let Some((dir, t, b)) = find_existing_install() {
            let label = RuntimeBackend::from_id(&b)
                .map(|x| x.label_ru().to_string())
                .unwrap_or_else(|| b.clone());
            (
                Some(dir.to_string_lossy().to_string()),
                true,
                Some(t),
                Some(b),
                Some(label),
            )
        } else {
            (None, false, None, None, None)
        };

    Ok(RuntimeStatus {
        llama_dir,
        installed,
        tag,
        backend,
        backend_label,
        recommended_backend: recommended.id().to_string(),
        recommended_label: recommended.label_ru().to_string(),
        app_dir: app.to_string_lossy().to_string(),
        default_models_dir: models.to_string_lossy().to_string(),
        runtime_root: rt.to_string_lossy().to_string(),
    })
}

async fn install_impl(
    app: &AppHandle,
    state: &RuntimeInstallState,
    backend_override: Option<RuntimeBackend>,
) -> Result<RuntimeStatus, DlErr> {
    let backend = backend_override.unwrap_or_else(recommend_backend);

    emit(
        app,
        "Готовлю установку",
        backend.label_ru(),
        0,
        0,
        false,
        None,
        false,
    );

    // Создаём portable-папки.
    let models = default_models_dir().map_err(DlErr::Failed)?;
    ensure_dir(&models).map_err(DlErr::Failed)?;
    let cache = runtime_root()
        .map_err(DlErr::Failed)?
        .join(".cache");
    ensure_dir(&cache).map_err(DlErr::Failed)?;

    let release = fetch_latest_release().await.map_err(DlErr::Failed)?;
    let tag = release.tag_name.clone();
    let (main_name, cudart_name) = asset_names(&tag, &backend);
    let main_asset = find_asset(&release, &main_name).map_err(DlErr::Failed)?;
    let cudart_asset = match &cudart_name {
        Some(n) => Some(find_asset(&release, n).map_err(DlErr::Failed)?),
        None => None,
    };

    // Оценка места: zip'ы * 2 (распаковка) + запас.
    let need = main_asset.size
        + cudart_asset.map(|a| a.size).unwrap_or(0);
    let need = need.saturating_mul(3); // zip + extract + margin
    if let Some(free) = free_space_bytes(&runtime_root().map_err(DlErr::Failed)?) {
        if free < need {
            return Err(DlErr::Failed(format!(
                "Недостаточно места на диске: нужно ~{}, свободно ~{}",
                fmt_mb(need),
                fmt_mb(free)
            )));
        }
    }

    let dest = backend_dir(&tag, &backend).map_err(DlErr::Failed)?;

    // Скачать основной zip.
    let main_zip = cache.join(&main_name);
    stream_to_file(
        app,
        state,
        &main_asset.browser_download_url,
        &main_zip,
        &format!("Скачиваю {}…", backend.label_ru()),
        &main_name,
    )
    .await?;

    // Скачать cudart при необходимости.
    let cudart_zip = if let Some(asset) = cudart_asset {
        let p = cache.join(&asset.name);
        stream_to_file(
            app,
            state,
            &asset.browser_download_url,
            &p,
            "Скачиваю CUDA Runtime…",
            &asset.name,
        )
        .await?;
        Some(p)
    } else {
        None
    };

    if state.cancel.load(Ordering::SeqCst) {
        return Err(DlErr::Canceled);
    }

    emit(
        app,
        "Распаковываю движок…",
        &main_name,
        0,
        0,
        false,
        None,
        false,
    );

    // extract_zip синхронный и CPU-bound — в blocking-пул.
    let dest_c = dest.clone();
    let main_zip_c = main_zip.clone();
    tokio::task::spawn_blocking(move || extract_zip(&main_zip_c, &dest_c, false))
        .await
        .map_err(|e| DlErr::Failed(format!("Ошибка задачи распаковки: {e}")))?
        .map_err(DlErr::Failed)?;

    if let Some(cz) = cudart_zip {
        emit(
            app,
            "Распаковываю CUDA Runtime…",
            cz.file_name().and_then(|n| n.to_str()).unwrap_or("cudart"),
            0,
            0,
            false,
            None,
            false,
        );
        let dest_c = dest.clone();
        tokio::task::spawn_blocking(move || extract_zip(&cz, &dest_c, true))
            .await
            .map_err(|e| DlErr::Failed(format!("Ошибка задачи распаковки: {e}")))?
            .map_err(DlErr::Failed)?;
    }

    if !is_installed_at(&dest) {
        return Err(DlErr::Failed(
            "После распаковки llama-server.exe не найден. Архив релиза изменился?".into(),
        ));
    }

    write_meta(&dest, &tag, &backend).map_err(DlErr::Failed)?;

    // Чистим zip-кэш (можно оставить — но экономим место).
    let _ = std::fs::remove_file(&main_zip);
    if let Some(n) = cudart_name {
        let _ = std::fs::remove_file(cache.join(n));
    }

    emit(
        app,
        "Готово",
        &format!("{} · {}", tag, backend.label_ru()),
        1,
        1,
        true,
        None,
        false,
    );

    build_status().map_err(DlErr::Failed)
}

fn fmt_mb(bytes: u64) -> String {
    format!("{:.0} МБ", bytes as f64 / (1024.0 * 1024.0))
}

/// Свободное место на томе, где лежит path (Windows: GetDiskFreeSpaceEx через std нестандартно —
/// используем простой fallback: None если не смогли).
fn free_space_bytes(path: &Path) -> Option<u64> {
    // На Windows берём корень диска path.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::ffi::OsStr;
        // Пробуем через `fs2`-подобный хак: GetDiskFreeSpaceExW через windows crate
        // не подключали — используем PowerShell-free подход через kernel32.
        // Минимально: если path не существует, create parent.
        let probe = if path.exists() {
            path.to_path_buf()
        } else {
            path.parent()?.to_path_buf()
        };
        let wide: Vec<u16> = OsStr::new(&probe)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free_bytes: u64 = 0;
        let mut total: u64 = 0;
        let mut total_free: u64 = 0;
        // windows crate feature Win32_Storage_FileSystem — может не быть. Используем raw.
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes,
                &mut total,
                &mut total_free,
            )
        };
        if ok != 0 {
            return Some(free_bytes);
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

// ── Tauri-команды ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn runtime_status() -> Result<RuntimeStatus, String> {
    build_status()
}

/// Установить (или переустановить) managed runtime.
/// `backend`: None / "auto" → по железу; иначе "cpu" | "vulkan" | "cuda-12.4".
#[tauri::command]
pub async fn runtime_install(
    app: AppHandle,
    state: State<'_, RuntimeInstallState>,
    backend: Option<String>,
) -> Result<RuntimeStatus, String> {
    {
        let mut active = state.lock_active();
        if *active {
            return Err("Установка движка уже идёт.".into());
        }
        *active = true;
    }
    state.cancel.store(false, Ordering::SeqCst);

    let backend = match backend.as_deref().map(str::trim).filter(|s| !s.is_empty() && *s != "auto")
    {
        Some(id) => Some(
            RuntimeBackend::from_id(id)
                .ok_or_else(|| format!("Неизвестный backend: {id}"))?,
        ),
        None => None,
    };

    let result = install_impl(&app, &state, backend).await;
    *state.lock_active() = false;

    match result {
        Ok(st) => Ok(st),
        Err(DlErr::Canceled) => {
            emit(&app, "Отменено", "", 0, 0, false, None, true);
            Err("Установка отменена.".into())
        }
        Err(DlErr::Failed(msg)) => {
            emit(&app, "Ошибка", "", 0, 0, false, Some(msg.clone()), false);
            Err(msg)
        }
    }
}

#[tauri::command]
pub fn runtime_cancel_install(state: State<RuntimeInstallState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

/// Гарантировать, что `{app_dir}/models` существует; вернуть путь.
#[tauri::command]
pub fn ensure_default_models_dir() -> Result<String, String> {
    let dir = default_models_dir()?;
    ensure_dir(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}
