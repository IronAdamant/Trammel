fn main() -> std::process::ExitCode {
    let name = std::env::args().next().unwrap_or_default();
    let base = std::path::Path::new(&name).file_name().and_then(|s| s.to_str()).unwrap_or("");
    if base.starts_with("trammel-mcp") {
        let store = std::env::var("TRAMMEL_DB_PATH").ok().map(std::path::PathBuf::from);
        if let Err(err) = trammel::mcp::serve(store) {
            eprintln!("{err}");
            return std::process::ExitCode::from(1);
        }
        return std::process::ExitCode::SUCCESS;
    }
    trammel::cli::main()
}
