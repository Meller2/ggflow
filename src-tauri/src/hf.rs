//! Каталог Hugging Face (Фаза 4): поиск GGUF-репозиториев, список файлов, скачивание.
//! HTTP делаем на Rust (reqwest): стриминг прямо в файл + события прогресса без фронт-плагинов.
//!
//! Эндпоинты HF:
//!   поиск   GET /api/models?search=&filter=gguf&sort=downloads&direction=-1&limit=40
//!   файлы   GET /api/models/{repo}/tree/main?recursive=true
//!   файл    GET /{repo}/resolve/main/{path}   (LFS-редирект reqwest проходит сам)

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

const HF: &str = "https://huggingface.co";
const UA: &str = concat!("LlamaLauncher/", env!("CARGO_PKG_VERSION"));
/// Не чаще одного события прогресса на ~2 МБ — чтобы не заваливать фронт.
const EMIT_STEP: u64 = 2_000_000;

// ── Модели данных ─────────────────────────────────────────────────────────────

/// Репозиторий в результатах поиска (для UI).
#[derive(Debug, Clone, Serialize)]
pub struct HfModel {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub last_modified: Option<String>,
}

/// Сырой ответ HF search (поля в camelCase).
#[derive(Debug, Deserialize)]
struct HfModelRaw {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(rename = "lastModified", default)]
    last_modified: Option<String>,
}

/// GGUF-файл в репозитории (для UI).
#[derive(Debug, Clone, Serialize)]
pub struct HfFile {
    pub path: String,
    pub size: u64,
}

/// Элемент дерева репо. У LFS-файлов реальный размер лежит в `lfs.size`,
/// а `size` — это размер указателя (мелкий), поэтому предпочитаем lfs.size.
#[derive(Debug, Deserialize)]
struct TreeEntry {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    lfs: Option<Lfs>,
}

#[derive(Debug, Deserialize)]
struct Lfs {
    #[serde(default)]
    size: u64,
}

/// Событие `download-progress` для фронта.
#[derive(Debug, Clone, Serialize)]
struct Progress {
    file: String,
    downloaded: u64,
    total: u64, // 0 = размер неизвестен
    done: bool,
    error: Option<String>,
    canceled: bool,
}

/// Состояние текущей загрузки — для отмены и защиты от параллельных скачиваний.
#[derive(Default)]
pub struct DownloadState {
    cancel: AtomicBool,
    /// Имя файла активной загрузки (None = свободно).
    active: Mutex<Option<String>>,
}

impl DownloadState {
    /// Poison-устойчивый lock (паника чужого потока не должна класть загрузки).
    fn active(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Внутренняя ошибка скачивания: отмена vs реальный сбой.
enum DlErr {
    Canceled,
    Failed(String),
}

// ── Вспомогательное ───────────────────────────────────────────────────────────

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(UA)
        .build()
        .map_err(|e| format!("Не удалось создать HTTP-клиент: {e}"))
}

/// Минимальное percent-кодирование строки поиска (RFC 3986 unreserved остаётся как есть).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ── Tauri-команды ─────────────────────────────────────────────────────────────

/// Поиск GGUF-репозиториев по подстроке, отсортированных по числу загрузок.
/// `limit` (None → 40) даёт фронту «показать ещё»: перезапрос с бо́льшим лимитом.
#[tauri::command]
pub async fn hf_search(query: String, limit: Option<u32>) -> Result<Vec<HfModel>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // HF отдаёт максимум ~100 за запрос; ограничим разумным диапазоном.
    let limit = limit.unwrap_or(40).clamp(1, 100);
    let url = format!(
        "{HF}/api/models?search={}&filter=gguf&sort=downloads&direction=-1&limit={limit}",
        urlencode(q)
    );
    let resp = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Сеть недоступна: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Hugging Face вернул {}", resp.status()));
    }
    let raw: Vec<HfModelRaw> = resp
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать ответ поиска: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|m| HfModel {
            id: m.id,
            downloads: m.downloads,
            likes: m.likes,
            last_modified: m.last_modified,
        })
        .collect())
}

/// Список .gguf-файлов репозитория с размерами (учитывая LFS).
#[tauri::command]
pub async fn hf_list_files(repo: String) -> Result<Vec<HfFile>, String> {
    let url = format!("{HF}/api/models/{repo}/tree/main?recursive=true");
    let resp = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Сеть недоступна: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Hugging Face вернул {} для {repo}", resp.status()));
    }
    let entries: Vec<TreeEntry> = resp
        .json()
        .await
        .map_err(|e| format!("Не удалось разобрать список файлов: {e}"))?;

    let mut files: Vec<HfFile> = entries
        .into_iter()
        .filter(|e| e.kind == "file" && e.path.to_lowercase().ends_with(".gguf"))
        .map(|e| {
            let size = e
                .lfs
                .as_ref()
                .map(|l| l.size)
                .filter(|&s| s > 0)
                .unwrap_or(e.size);
            HfFile { path: e.path, size }
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Скачать файл репо в папку назначения. Стримит в `<файл>.part`, затем rename.
/// Шлёт события `download-progress`. Возвращает итоговый путь.
#[tauri::command]
pub async fn hf_download(
    app: AppHandle,
    state: State<'_, DownloadState>,
    repo: String,
    file: String,
    dest_dir: String,
) -> Result<String, String> {
    // В репо путь может быть с подпапками — на диск кладём по базовому имени.
    let filename = Path::new(&file)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Некорректное имя файла".to_string())?
        .to_string();

    let dir = PathBuf::from(&dest_dir);
    if !dir.is_dir() {
        return Err(format!("Папка назначения не найдена: {dest_dir}"));
    }
    let final_path = dir.join(&filename);
    if final_path.exists() {
        return Err(format!("Файл «{filename}» уже есть в папке."));
    }
    let part_path = dir.join(format!("{filename}.part"));

    // Докачка: если .part уже есть, продолжим с его размера (HTTP Range).
    // 0 = качаем с нуля.
    let resume_from = tokio::fs::metadata(&part_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    // Занять единственный слот загрузки.
    {
        let mut active = state.active();
        if active.is_some() {
            return Err("Уже идёт другая загрузка — дождитесь её завершения.".into());
        }
        *active = Some(filename.clone());
    }
    state.cancel.store(false, Ordering::SeqCst);

    let url = format!("{HF}/{repo}/resolve/main/{file}");
    let result = stream_to_file(&app, &state, &url, &part_path, &filename, resume_from).await;

    // Освободить слот в любом исходе.
    *state.active() = None;

    match result {
        Ok(total) => {
            std::fs::rename(&part_path, &final_path)
                .map_err(|e| format!("Не удалось сохранить файл: {e}"))?;
            emit(&app, &filename, total, total, true, None, false);
            Ok(final_path.to_string_lossy().to_string())
        }
        Err(DlErr::Canceled) => {
            // .part НЕ удаляем — оставляем для последующей докачки.
            emit(&app, &filename, 0, 0, false, None, true);
            Err("Загрузка отменена.".into())
        }
        Err(DlErr::Failed(msg)) => {
            // .part НЕ удаляем — при сетевом сбое даём шанс докачать позже.
            emit(&app, &filename, 0, 0, false, Some(msg.clone()), false);
            Err(msg)
        }
    }
}

/// Отменить текущую загрузку (флаг подхватится в цикле стриминга).
#[tauri::command]
pub fn hf_cancel_download(state: State<DownloadState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

// ── Реализация стриминга ──────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn emit(
    app: &AppHandle,
    file: &str,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
    canceled: bool,
) {
    let _ = app.emit(
        "download-progress",
        Progress {
            file: file.to_string(),
            downloaded,
            total,
            done,
            error,
            canceled,
        },
    );
}

/// Качает `url` в `part_path` кусками, эмитит прогресс, проверяет флаг отмены.
/// При `resume_from > 0` продолжает существующий `.part` через HTTP Range.
/// Возвращает итоговый размер при успехе.
async fn stream_to_file(
    app: &AppHandle,
    state: &State<'_, DownloadState>,
    url: &str,
    part_path: &Path,
    filename: &str,
    resume_from: u64,
) -> Result<u64, DlErr> {
    let mut req = client().map_err(DlErr::Failed)?.get(url);
    if resume_from > 0 {
        // Просим сервер отдать хвост файла начиная с уже скачанного смещения.
        req = req.header("Range", format!("bytes={resume_from}-"));
    }
    let mut resp = req
        .send()
        .await
        .map_err(|e| DlErr::Failed(format!("Сеть недоступна: {e}")))?;

    // 206 Partial Content → докачка принята. 200 → сервер отдаёт файл целиком
    // (Range проигнорирован), поэтому начинаем с нуля и перезаписываем .part.
    let status = resp.status();
    let resuming = status == reqwest::StatusCode::PARTIAL_CONTENT && resume_from > 0;
    if !status.is_success() {
        return Err(DlErr::Failed(format!(
            "Hugging Face вернул {status} при скачивании"
        )));
    }

    let already: u64 = if resuming { resume_from } else { 0 };
    // content_length — это длина ТЕЛА ответа; при докачке прибавляем уже скачанное.
    let body_len = resp.content_length().unwrap_or(0);
    let total = if body_len > 0 { already + body_len } else { 0 };

    // Открываем .part на дозапись (докачка) или создаём заново (с нуля).
    let mut out = if resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(part_path)
            .await
    } else {
        File::create(part_path).await
    }
    .map_err(|e| DlErr::Failed(format!("Не удалось открыть файл: {e}")))?;

    let mut downloaded = already;
    let mut last_emit = already;
    // Начальное событие, чтобы UI сразу показал полосу (и точку старта при докачке).
    emit(app, filename, downloaded, total, false, None, false);

    loop {
        if state.cancel.load(Ordering::SeqCst) {
            let _ = out.flush().await;
            return Err(DlErr::Canceled);
        }
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| DlErr::Failed(format!("Ошибка при скачивании: {e}")))?;
        let chunk = match chunk {
            Some(c) => c,
            None => break, // конец потока
        };
        out.write_all(&chunk)
            .await
            .map_err(|e| DlErr::Failed(format!("Ошибка записи на диск: {e}")))?;
        downloaded += chunk.len() as u64;

        if downloaded - last_emit >= EMIT_STEP {
            last_emit = downloaded;
            emit(app, filename, downloaded, total, false, None, false);
        }
    }

    out.flush()
        .await
        .map_err(|e| DlErr::Failed(format!("Ошибка сброса на диск: {e}")))?;

    // Проверка целостности: если сервер сообщил размер — он должен совпасть.
    // Иначе оборванный поток с кодом 200 молча сохранился бы как «валидный» файл.
    if total > 0 && downloaded != total {
        return Err(DlErr::Failed(format!(
            "Файл скачан не полностью: {downloaded} из {total} байт. Попробуйте докачать."
        )));
    }

    Ok(if total > 0 { total } else { downloaded })
}
