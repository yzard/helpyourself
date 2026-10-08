use crate::{
    authentication::{hash_password, normalize_username, now},
    config::{Config, TEMPLATE},
    database::Database,
    error::AppError,
};
use clap::{Parser, Subcommand};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(version, about = "Self-hosted health archive")]
pub struct Cli {
    #[arg(long)]
    pub data_dir: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    InitConfig,
    Serve,
    CreateUser {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password_stdin: bool,
    },
    ResetPassword {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password_stdin: bool,
    },
    EnableUser {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password_stdin: bool,
    },
    DisableUser {
        #[arg(long)]
        username: String,
    },
    CheckConfig,
    ProbeProviders,
}

pub fn write_template(data_dir: &std::path::Path) -> Result<(), AppError> {
    if !data_dir.is_absolute() {
        return Err(AppError::Invalid(
            "--data-dir must be an absolute directory",
        ));
    }
    std::fs::create_dir_all(data_dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(data_dir.join("config.toml"))?;
    file.write_all(TEMPLATE.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn read_password(password_stdin: bool) -> Result<String, AppError> {
    if !password_stdin {
        return Ok(rpassword::prompt_password("Password: ")?);
    }
    let mut password = String::new();
    std::io::stdin().take(1027).read_to_string(&mut password)?;
    if password.ends_with('\n') {
        password.pop();
        if password.ends_with('\r') {
            password.pop();
        }
    }
    Ok(password)
}

pub async fn run(cli: Cli) -> Result<(), AppError> {
    if matches!(&cli.command, Command::InitConfig) {
        return write_template(&cli.data_dir);
    }
    let config = Config::load(&cli.data_dir)?;
    if matches!(cli.command, Command::CheckConfig) {
        println!("Configuration valid");
        return Ok(());
    }
    if matches!(cli.command, Command::ProbeProviders) {
        crate::ocr::probe(&config.ocr).await?;
        println!("ocr: service is available (model not loaded, not an accuracy test)");
        if config.providers.analysis.enabled {
            crate::provider::probe(&config.providers.analysis).await?;
            println!("analysis: synthetic text response received");
        } else {
            println!("analysis: disabled");
        }
        return Ok(());
    }
    if matches!(cli.command, Command::Serve) {
        let address = config.server.listen;
        let state = crate::app::AppState::open(config).await?;
        let listener = tokio::net::TcpListener::bind(address).await?;
        tracing::info!(listen = %listener.local_addr()?, "server ready");

        let stop = tokio_util::sync::CancellationToken::new();
        let worker = tokio::spawn(crate::worker::run(state.clone(), stop.clone()));
        let outcome = axum::serve(listener, crate::app::create_application(state.clone()))
            .with_graceful_shutdown(shutdown())
            .await;
        stop.cancel();
        let worker_outcome = worker.await.map_err(|_| AppError::Internal);
        let cleanup_outcome = state.shutdown().await;
        worker_outcome?;
        cleanup_outcome?;
        outcome?;
        return Ok(());
    }
    let database = Database::open(&config.data_dir).await?;
    let reset_password = matches!(&cli.command, Command::ResetPassword { .. });
    match cli.command {
        Command::CreateUser {
            username,
            password_stdin,
        }
        | Command::ResetPassword {
            username,
            password_stdin,
        } => {
            let username = normalize_username(&username)?;
            let password_hash = hash_password(read_password(password_stdin)?).await?;
            if reset_password {
                database
                    .change_credentials(&username, Some(&password_hash))
                    .await?;
                println!("Password reset; sessions revoked");
            } else {
                database
                    .create_user(&username, &password_hash, now()?)
                    .await?;
                println!("User created");
            }
        }
        Command::EnableUser {
            username,
            password_stdin,
        } => {
            let username = normalize_username(&username)?;
            let password_hash = hash_password(read_password(password_stdin)?).await?;
            database.enable_user(&username, &password_hash).await?;
            println!("User enabled with a new password; sessions revoked");
        }
        Command::DisableUser { username } => {
            database
                .change_credentials(&normalize_username(&username)?, None)
                .await?;
            println!("User disabled; sessions revoked");
        }
        _ => unreachable!("command handled above"),
    }
    database.close().await;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
