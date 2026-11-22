use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ytm_core::api_key::{API_KEY_ENV, fetch_api_key};
use ytm_core::auth::CookieJar;
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN, playlist_browse_id};
use ytm_core::models::{FeedPage, SearchFilter, SearchResults};
use ytm_core::parsers::account::parse_account;
use ytm_core::parsers::browse_page::parse_browse_page;
use ytm_core::parsers::home::{parse_feed, parse_feed_continuation, parse_home};
use ytm_core::parsers::radio::{parse_radio, parse_radio_continuation};
use ytm_core::parsers::search::{parse_search, parse_search_continuation};
use ytm_core::parsers::search_suggestions::parse_search_suggestions;

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
    /// Probe a feed by browse ID. Reports shape without printing tokens.
    Feed {
        browse_id: String,
        #[arg(long)]
        params: Option<String>,
    },
    /// Probe the next feed page. Reports shape without printing tokens.
    FeedContinue { token: String },
    /// POST /browse for an album or playlist. Use --parse to print the page model.
    Page {
        browse_id: String,
        #[arg(long)]
        parse: bool,
    },
    /// POST /search and print the raw JSON, or the parsed results with --parse.
    Search {
        query: String,
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Probe a filtered search and report result counts without printing tokens.
    SearchFiltered {
        query: String,
        /// songs, videos, albums, artists, profiles, playlists, featured-playlists,
        /// community-playlists, podcasts, or episodes.
        #[arg(value_parser = search_filter)]
        filter: SearchFilter,
        /// Follow the first continuation when present.
        #[arg(long)]
        next_page: bool,
    },
    /// Probe the next search page without printing tokens.
    SearchContinue { token: String },
    /// POST /music/get_search_suggestions and print the raw JSON, or the
    /// parsed suggestions with --parse.
    Suggest {
        input: String,
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// POST /account/accounts_list (needs KASET_COOKIE_FILE) and print the raw
    /// JSON, or the selected account with --parse.
    Account {
        #[arg(long)]
        parse: bool,
        /// Write output to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// POST /next to start a radio mix. Use --parse to print the queue model.
    Radio {
        video_id: String,
        #[arg(long)]
        parse: bool,
    },
    /// POST /next to continue a radio mix. Use --parse to print the queue model.
    RadioContinue {
        token: String,
        #[arg(long)]
        parse: bool,
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

    let (text, out) = match cli.command {
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
            (text, out)
        }
        Command::Feed { browse_id, params } => {
            let raw = client
                .browse_with_params(&browse_id, params.as_deref())
                .await?;
            (feed_summary(parse_feed(&raw)?), None)
        }
        Command::FeedContinue { token } => {
            let raw = client.browse_continuation(&token).await?;
            (feed_summary(parse_feed_continuation(&raw)?), None)
        }
        Command::Page { browse_id, parse } => {
            let raw = client.browse(&playlist_browse_id(&browse_id)).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_browse_page(&raw)?)?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, None)
        }
        Command::Search { query, parse, out } => {
            let raw = client.search(&query).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_search(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
        Command::SearchFiltered {
            query,
            filter,
            next_page,
        } => {
            let first = parse_search(&client.search_filtered(&query, filter).await?);
            let mut text = search_summary(&first);
            if let Some(token) = first.continuation.filter(|_| next_page) {
                let next = parse_search_continuation(&client.search_continuation(&token).await?);
                text.push_str(&format!("\nNext page: {}", search_summary(&next)));
            }
            (text, None)
        }
        Command::SearchContinue { token } => {
            let page = parse_search_continuation(&client.search_continuation(&token).await?);
            (search_summary(&page), None)
        }
        Command::Suggest { input, parse, out } => {
            let raw = client.search_suggestions(&input).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_search_suggestions(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
        Command::Account { parse, out } => {
            let raw = client.accounts_list().await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_account(&raw))?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, out)
        }
        Command::Radio { video_id, parse } => {
            let raw = client.radio(&video_id).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_radio(&raw)?)?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, None)
        }
        Command::RadioContinue { token, parse } => {
            let raw = client.radio_continuation(&token).await?;
            let text = if parse {
                serde_json::to_string_pretty(&parse_radio_continuation(&raw)?)?
            } else {
                serde_json::to_string_pretty(&raw)?
            };
            (text, None)
        }
    };
    match out {
        Some(path) => {
            std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?
        }
        None => println!("{text}"),
    }
    Ok(())
}

fn search_filter(value: &str) -> Result<SearchFilter, &'static str> {
    serde_json::from_value(serde_json::Value::String(value.replace('-', "_")))
        .map_err(|_| "unknown search filter")
}

fn search_summary(page: &SearchResults) -> String {
    format!(
        "{} items, continuation present: {}",
        page.items.len(),
        page.continuation.is_some()
    )
}

fn feed_summary(page: FeedPage) -> String {
    let items = page
        .sections
        .iter()
        .map(|section| section.items.len())
        .sum::<usize>();
    format!(
        "{} sections, {items} items, {} chips, continuation present: {}",
        page.sections.len(),
        page.chips.len(),
        page.continuation.is_some()
    )
}

fn load_cookies() -> Result<Option<CookieJar>> {
    let Some(path) = std::env::var_os("KASET_COOKIE_FILE") else {
        return Ok(None);
    };
    let header = std::fs::read_to_string(&path).context("reading KASET_COOKIE_FILE")?;
    Ok(Some(CookieJar::from_header(&header)))
}
