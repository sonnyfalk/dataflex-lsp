use std::path::PathBuf;
use std::sync::OnceLock;

use flexi_logger::{FileSpec, LogSpecification, Logger, LoggerHandle};

static LOGGER_HANDLE: OnceLock<LoggerHandle> = OnceLock::new();

pub fn initialize_logging() {
    let log_file_path = log_file_path();
    let file_spec = FileSpec::try_from(log_file_path)
        .expect("failed to initialize logging with path: {log_file_path}");
    _ = LOGGER_HANDLE.set(
        Logger::with(LogSpecification::info())
            .log_to_file(file_spec)
            .format(|w, now, record| {
                write!(
                    w,
                    "{} [{}] {}",
                    now.format("%H:%M:%S%.3f"),
                    record.level(),
                    record.args()
                )
            })
            .start()
            .expect("failed to initialize logger"),
    );
    log_panics::init();

    log::info!("Log initialized");
}

fn log_file_path() -> PathBuf {
    let log_dir = log_dir();

    if !log_dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(&log_dir)
            .unwrap();
    }

    let mut log_file_path = log_dir;
    log_file_path.push(format!("dataflex-lsp-{}.log", std::process::id()));
    log_file_path
}

#[cfg(target_os = "windows")]
fn log_dir() -> PathBuf {
    let mut log_dir = dirs::data_local_dir().unwrap();
    log_dir.push("dataflex-lsp\\logs");
    log_dir
}

#[cfg(target_os = "macos")]
fn log_dir() -> PathBuf {
    let mut log_dir = dirs::home_dir().unwrap();
    log_dir.push("Library/Logs/dataflex-lsp");
    log_dir
}
