//! Minimal logger: stderr plus a per-run file in the app data dir, so problems in a
//! bundle launched from Finder can still be read afterwards.

use parking_lot::Mutex;
use std::fs::File;
use std::io::Write;
use std::path::Path;

struct AppLogger {
    file: Mutex<Option<File>>,
}

static LOGGER: AppLogger = AppLogger { file: Mutex::new(None) };

fn max_level_for(target: &str) -> log::LevelFilter {
    if target.starts_with("whisper_rs") || target.starts_with("tao") || target.starts_with("wry") || target.starts_with("hyper") || target.starts_with("reqwest") {
        log::LevelFilter::Warn
    } else {
        log::LevelFilter::Info
    }
}

impl log::Log for AppLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= max_level_for(m.target())
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}] {}\n",
            chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
            record.level(),
            record.target(),
            record.args()
        );
        let _ = std::io::stderr().write_all(line.as_bytes());
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.flush();
        }
    }
}

pub fn init() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
}

/// Start writing to `<dir>/mynah.log`, keeping the previous run as `mynah.log.1`.
pub fn attach_file(dir: &Path) {
    let path = dir.join("mynah.log");
    let _ = std::fs::rename(&path, dir.join("mynah.log.1"));
    match File::create(&path) {
        Ok(f) => *LOGGER.file.lock() = Some(f),
        Err(e) => log::warn!("could not open log file {}: {e}", path.display()),
    }
}
