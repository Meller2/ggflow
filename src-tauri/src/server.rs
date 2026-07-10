//! Жизненный цикл llama-server.exe: запуск, остановка, статус, стриминг лога.
//! Флаги — маппинг из llama.bat + дефолты из config::LaunchDefaults.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Сколько ждать готовности сервера, прежде чем сообщить UI об ошибке.
const READY_TIMEOUT: Duration = Duration::from_secs(180);

/// Конфиг запуска, приходит из UI.
#[derive(Debug, Clone, Deserialize)]
pub struct LaunchConfig {
    /// Папка с llama-server.exe.
    pub llama_dir: String,
    /// Полный путь к .gguf.
    pub model_path: String,
    pub ctx: u32,
    /// "f16" | "q8_0" | "q4_0".
    pub kv_quant: String,
    pub threads: u32,
    pub ngl: u32,
    pub port: u16,
    pub tools: bool,
}

/// Статус для UI.
#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub model_name: Option<String>,
}

/// Состояние запущенного сервера (живёт в Tauri-managed state).
/// Child уходит в поток-наблюдатель; здесь храним только pid + метаданные + id поколения.
#[derive(Default)]
pub struct ServerState {
    inner: Mutex<Option<RunningServer>>,
    next_id: AtomicU64,
}

impl ServerState {
    /// Взять lock, устойчиво к «отравлению» (паника другого потока не должна
    /// класть всё приложение — восстанавливаем внутреннее значение).
    fn lock(&self) -> MutexGuard<'_, Option<RunningServer>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct RunningServer {
    pid: u32,
    port: u16,
    model_name: String,
    /// Поколение — чтобы наблюдатель не затирал состояние более нового запуска.
    id: u64,
}

// Windows: не открывать консольное окно у дочернего процесса.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Собрать аргументы llama-server из конфига (портирует build_command + llama.bat).
fn build_args(cfg: &LaunchConfig) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-m".into(),
        cfg.model_path.clone(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        cfg.port.to_string(),
        "-c".into(),
        cfg.ctx.to_string(),
        "--flash-attn".into(),
        "on".into(),
        "-ngl".into(),
        cfg.ngl.to_string(),
        "-t".into(),
        cfg.threads.to_string(),
        "-ub".into(),
        "1024".into(),
        "-b".into(),
        "1024".into(),
        // Чистый лог для парсинга в UI.
        "--log-colors".into(),
        "off".into(),
        "--no-log-prefix".into(),
    ];

    // KV-квант: f16 = без флагов (дефолт llama.cpp). q8_0/q4_0 требуют flash-attn (включён).
    if cfg.kv_quant != "f16" {
        args.push("--cache-type-k".into());
        args.push(cfg.kv_quant.clone());
        args.push("--cache-type-v".into());
        args.push(cfg.kv_quant.clone());
    }

    args.push("--jinja".into());

    if cfg.tools {
        args.push("--tools".into());
        args.push("all".into());
        args.push("--ui-mcp-proxy".into());
    }

    args
}

/// Проверить, свободен ли TCP-порт на 127.0.0.1.
fn port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Убить процесс (и всё его дерево на Windows). Используется и при стопе, и при
/// закрытии приложения, поэтому вынесено отдельно.
fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").arg(pid.to_string()).output();
    }
}

/// Убить сервер, если он ещё запущен. Вызывается при закрытии окна приложения,
/// чтобы llama-server.exe не остался осиротевшим и не держал порт/VRAM.
pub fn shutdown(state: &ServerState) {
    if let Some(s) = state.lock().take() {
        kill_pid(s.pid);
    }
}

/// Помечает готовность: взводит флаг и шлёт `server-ready` ровно один раз.
fn mark_ready(app: &AppHandle, ready: &AtomicBool, port: u16) {
    // swap → true возвращает прежнее значение; шлём событие только на первом переходе.
    if !ready.swap(true, Ordering::SeqCst) {
        let _ = app.emit("server-ready", port);
    }
}

/// Читать поток построчно и слать события `server-log`. Детектить готовность.
fn stream_reader<R: std::io::Read + Send + 'static>(
    app: AppHandle,
    reader: R,
    port: u16,
    ready: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            // Готовность: llama-server печатает "... listening on http://127.0.0.1:PORT".
            let lower = line.to_lowercase();
            if lower.contains("listening") && lower.contains(&port.to_string()) {
                mark_ready(&app, &ready, port);
            }
            let _ = app.emit("server-log", line);
        }
    });
}

/// Страховочный сторож готовности: если по логу готовность так и не поймали,
/// периодически пробуем открыть TCP-соединение к порту. Если за READY_TIMEOUT
/// сервер не поднялся и процесс всё ещё «наш» — сообщаем UI об ошибке.
fn spawn_ready_watchdog(app: AppHandle, port: u16, id: u64, ready: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + READY_TIMEOUT;
        while Instant::now() < deadline {
            if ready.load(Ordering::SeqCst) {
                return; // уже готов (лог поймал раньше)
            }
            // Активная проверка: порт принимает соединения → сервер слушает.
            let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok() {
                mark_ready(&app, &ready, port);
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        // Дедлайн вышел, готовности нет. Сообщаем только если это всё ещё наше поколение.
        if ready.load(Ordering::SeqCst) {
            return;
        }
        let st = app.state::<ServerState>();
        let is_current = st.lock().as_ref().map(|s| s.id == id).unwrap_or(false);
        if is_current {
            let _ = app.emit(
                "server-timeout",
                format!(
                    "Сервер не запустился за {} с. Возможно, не хватает памяти или модель не загрузилась — смотрите лог.",
                    READY_TIMEOUT.as_secs()
                ),
            );
        }
    });
}

// ── Tauri-команды ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn start_server(
    app: AppHandle,
    state: State<ServerState>,
    config: LaunchConfig,
) -> Result<ServerStatus, String> {
    let mut guard = state.lock();
    if guard.is_some() {
        return Err("Сервер уже запущен. Сначала остановите его.".into());
    }

    // Валидация путей и порта (раннее обнаружение проблем).
    // Примечание: проверка порта → spawn = TOCTOU; если между проверкой и bind
    // кто-то займёт порт, llama-server упадёт сам с понятной ошибкой в логе.
    let exe = Path::new(&config.llama_dir).join("llama-server.exe");
    if !exe.is_file() {
        return Err(format!("llama-server.exe не найден в {}", config.llama_dir));
    }
    if !Path::new(&config.model_path).is_file() {
        return Err(format!("Файл модели не найден: {}", config.model_path));
    }
    if !port_available(config.port) {
        return Err(format!(
            "Порт {} уже занят. Закройте другой сервер или смените порт в настройках.",
            config.port
        ));
    }

    let model_name = Path::new(&config.model_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("model")
        .to_string();

    let args = build_args(&config);
    let _ = app.emit("server-log", format!("$ llama-server {}", args.join(" ")));

    let mut cmd = Command::new(&exe);
    cmd.args(&args)
        .current_dir(&config.llama_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child: Child = cmd
        .spawn()
        .map_err(|e| format!("Не удалось запустить llama-server: {e}"))?;

    let pid = child.id();
    let id = state.next_id.fetch_add(1, Ordering::SeqCst);
    let ready = Arc::new(AtomicBool::new(false));

    // Стримим оба потока (забираем пайпы у child).
    if let Some(out) = child.stdout.take() {
        stream_reader(app.clone(), out, config.port, ready.clone());
    }
    if let Some(err) = child.stderr.take() {
        stream_reader(app.clone(), err, config.port, ready.clone());
    }

    // Сторож готовности: таймаут + активная TCP-проверка, если лог не поймал «listening».
    spawn_ready_watchdog(app.clone(), config.port, id, ready.clone());

    // Поток-наблюдатель владеет Child и ждёт его завершения (в т.ч. самопадение/краш).
    {
        let app_mon = app.clone();
        std::thread::spawn(move || {
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            // Снять состояние, только если это всё ещё наше поколение.
            let st = app_mon.state::<ServerState>();
            let mut g = st.lock();
            let is_current = g.as_ref().map(|s| s.id == id).unwrap_or(false);
            if is_current {
                *g = None;
            }
            drop(g);
            let _ = app_mon.emit("server-exit", code);
        });
    }

    *guard = Some(RunningServer {
        pid,
        port: config.port,
        model_name: model_name.clone(),
        id,
    });

    Ok(ServerStatus {
        running: true,
        port: Some(config.port),
        model_name: Some(model_name),
    })
}

#[tauri::command]
pub fn stop_server(app: AppHandle, state: State<ServerState>) -> Result<(), String> {
    let server = {
        let mut guard = state.lock();
        match guard.take() {
            Some(s) => s,
            None => return Ok(()), // уже остановлен
        }
    };

    // Убиваем всё дерево процессов. Наблюдатель, увидев смерть процесса, эмитит server-exit.
    kill_pid(server.pid);

    let _ = app.emit("server-log", "— Остановка сервера —".to_string());
    Ok(())
}

#[tauri::command]
pub fn server_status(state: State<ServerState>) -> ServerStatus {
    let guard = state.lock();
    match guard.as_ref() {
        Some(s) => ServerStatus {
            running: true,
            port: Some(s.port),
            model_name: Some(s.model_name.clone()),
        },
        None => ServerStatus {
            running: false,
            port: None,
            model_name: None,
        },
    }
}
