use clap::Parser;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt().with_target(false).init();
    match helpyourself::cli::run(helpyourself::cli::Cli::parse()).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
