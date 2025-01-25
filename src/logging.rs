pub fn initialize_logging() {
    let log_file = format!(
        "{}/Library/Logs/dataflex-lsp/dataflex-lsp-{}.log",
        dirs::home_dir()
            .expect("Failed to get home dir")
            .to_str()
            .unwrap(),
        std::process::id()
    );
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::ConfigBuilder::new()
            .set_time_offset_to_local()
            .expect("Failed to get local time offset")
            .build(),
        std::fs::File::create(log_file).unwrap(),
    )
    .expect("failed to initialize logger");
    log_panics::init();

    log::info!("Log initialized");
}
