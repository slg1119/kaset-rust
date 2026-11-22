use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ytm_core::api_key::{API_KEY_ENV, fetch_api_key};
use ytm_core::auth::CookieJar;
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};
use ytm_core::parsers::home::parse_home;

/// Probe YouTube Music InnerTube endpoints (read-only).
///
/// KASET_COOKIE_FILE: path to a file containing a Cookie header. Keep it
/// outside the repository. Omit it for signed-out requests.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// POST /browse and print the raw JSON, or the parsed model with --parse.
    Browse {
        browse_id: String,
        #[arg(long)]
        parse: Option<ParseAs>,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
enum ParseAs {
    Home,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();

    let api_key = match std::env::var(API_KEY_ENV) {
        Ok(key) if !key.trim().is_empty() => key.trim().to_owned(),
        _ => fetch_api_key(MUSIC_ORIGIN)
            .await
            .context("resolving API key")?,
    };
    let client = InnerTubeClient::new(ClientConfig::music(api_key), load_cookies()?)?;

    match cli.command {
        Command::Browse {
            browse_id,
            parse,
            out,
        } => {
            let raw = client.browse(&browse_id).await?;
            let text = match parse {
                Some(ParseAs::Home) => serde_json::to_string_pretty(&parse_home(&raw)?)?,
                None => serde_json::to_string_pretty(&raw)?,
            };
            match out {
                Some(path) => std::fs::write(&path, text)
                    .with_context(|| format!("writing {}", path.display()))?,
                None => println!("{text}"),
            }
        }
    }
    Ok(())
}

fn load_cookies() -> Result<Option<CookieJar>> {
    let Some(path) = std::env::var_os("KASET_COOKIE_FILE") else {
        return Ok(None);
    };
    let header = std::fs::read_to_string(&path).context("reading KASET_COOKIE_FILE")?;
    Ok(Some(CookieJar::from_header(&header)))
}
