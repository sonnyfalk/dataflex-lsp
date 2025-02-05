use std::path::PathBuf;

pub fn initialize_logging() {
    let log_file_path = log_file_path();
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::ConfigBuilder::new()
            .set_time_offset_to_local()
            .expect("Failed to get local time offset")
            .build(),
        std::fs::File::create(log_file_path).unwrap(),
    )
    .expect("failed to initialize logger");
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
