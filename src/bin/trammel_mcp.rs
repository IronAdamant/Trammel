fn main() -> std::process::ExitCode {
    let store = std::env::var("TRAMMEL_DB_PATH").ok().map(std::path::PathBuf::from);
    if let Err(err) = trammel::mcp::serve(store) {
        eprintln!("{err}");
        return std::process::ExitCode::from(1);
    }
    std::process::ExitCode::SUCCESS
}
