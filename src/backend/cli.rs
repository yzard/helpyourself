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
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    InitConfig {
        path: PathBuf,
    },
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
    DisableUser {
        #[arg(long)]
        username: String,
    },
    CheckConfig,
    ProbeProviders,
}

pub fn write_template(path: &std::path::Path) -> Result<(), AppError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
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
    if let Command::InitConfig { path } = &cli.command {
        return write_template(path);
    }
    let path = cli
        .config
        .ok_or(AppError::Invalid("--config PATH is required"))?;
    let config = Config::load(&path)?;
    if matches!(cli.command, Command::CheckConfig) {
        println!("Configuration valid");
        return Ok(());
    }
    if matches!(cli.command, Command::ProbeProviders) {
        for (name, provider) in [
            ("ocr", Some(&config.providers.ocr)),
            ("document_parser", config.providers.document_parser.as_ref()),
            ("analysis", Some(&config.providers.analysis)),
        ] {
            if let Some(provider) = provider.filter(|provider| provider.enabled) {
                crate::provider::probe(provider).await?;
                println!("{name}: synthetic text response received (not an OCR accuracy test)");
            } else {
                println!("{name}: disabled");
            }
        }
        return Ok(());
    }
    if matches!(cli.command, Command::Serve) {
        let address = config.server.listen;
        let state = crate::app::AppState::open(config).await?;
        let listener = tokio::net::TcpListener::bind(address).await?;
        tracing::info!(listen = %listener.local_addr()?, "server ready");
        let database = state.database.clone();
        let stop = tokio_util::sync::CancellationToken::new();
        let worker = tokio::spawn(crate::worker::run(state.clone(), stop.clone()));
        let outcome = axum::serve(listener, crate::app::create_application(state))
            .with_graceful_shutdown(shutdown())
            .await;
        stop.cancel();
        worker.await.map_err(|_| AppError::Internal)?;
        database.close().await;
        outcome?;
        return Ok(());
    }
    let database = Database::open(&config.server.data_dir).await?;
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
