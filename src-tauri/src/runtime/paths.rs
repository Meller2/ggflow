//! Portable-first app data paths for managed runtime / settings / models.

use std::path::{Path, PathBuf};

pub(crate) const SERVER_EXE: &str = "llama-server.exe";

/// Имя папки данных — совпадает с `tauri.conf.json` → `identifier`.
pub const DATA_DIR_NAME: &str = "com.ggflow.app";
/// Прежние identifier: ищем runtime/settings, но больше не пишем сюда.
pub const LEGACY_DATA_DIR_NAMES: &[&str] =
    &["com.llamalauncher.app", "com.ilzat.llama-launcher"];

/// Каталог, где лежит exe приложения. В dev — `target/debug`, в release — папка программы.
pub fn exe_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Не удалось определить путь к программе: {e}"))?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "Некорректный путь к программе".into())
}

/// Можно ли писать в каталог (portable-проверка для Program Files и т.п.).
fn dir_is_writable(dir: &Path) -> bool {
    if !dir.is_dir() {
        // Попробуем создать — если нельзя, не writable.
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    let probe = dir.join(".ll-write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// `%LOCALAPPDATA%/<name>` — fallback, когда рядом с exe писать нельзя (Program Files).
pub(crate) fn local_data_dir(name: &str) -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Не удалось определить LOCALAPPDATA".to_string())?;
    Ok(base.join(name))
}

/// Корень данных приложения:
/// 1) рядом с exe, если туда можно писать (portable / dev);
/// 2) иначе `%LOCALAPPDATA%/com.ggflow.app` (NSIS в Program Files).
pub fn app_dir() -> Result<PathBuf, String> {
    let beside = exe_dir()?;
    if dir_is_writable(&beside) {
        return Ok(beside);
    }
    // Fallback: локальные данные пользователя (не требует админа).
    let dir = local_data_dir(DATA_DIR_NAME)?;
    ensure_dir(&dir)?;
    Ok(dir)
}

pub fn runtime_root() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("runtime"))
}

pub fn default_models_dir() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("models"))
}

pub(crate) fn ensure_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|e| format!("Не удалось создать «{}»: {e}", path.display()))
}

/// Найти llama-server.exe в дереве (после распаковки zip может быть вложенная папка).
pub(crate) fn find_server_exe(root: &Path) -> Option<PathBuf> {
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

pub(crate) fn is_installed_at(dir: &Path) -> bool {
    dir.join(SERVER_EXE).is_file()
}

/// Свободное место на томе path (для проверок перед скачиванием/установкой).
pub(crate) fn free_space_bytes(path: &Path) -> Option<u64> {
    // На Windows берём корень диска path.
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
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
            GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_bytes, &mut total, &mut total_free)
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
