mod args;
mod output;
#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::future::Future;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use kaset_session::{
    FileStore, SESSION_FILE_ENV, STORE_ENV, SessionStore, StoreError, keyring_store,
};
use ytm_core::YtmError;
use ytm_core::api_key::music_api_key;
use ytm_core::auth::CookieJar;
use ytm_core::client::{ClientConfig, InnerTubeClient};
use ytm_core::parsers::account::parse_account;
use ytm_core::parsers::browse_page::parse_browse_page;
use ytm_core::parsers::home::parse_feed;
use ytm_core::parsers::radio::parse_radio;
use ytm_core::parsers::search::parse_search;

use args::{Cli, Command};
use output::Output;

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("Not signed in. Sign in with the Kaset app first.")]
    MissingSession,
    #[error("Set {SESSION_FILE_ENV} to the debug session file or use --session-file.")]
    MissingSessionFile,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Api(#[from] YtmError),
    #[error("could not write output: {0}")]
    Output(#[from] io::Error),
}

impl CliError {
    fn exit_code(&self) -> ExitCode {
        ExitCode::from(match self {
            Self::MissingSession => 2,
            Self::Api(YtmError::AuthExpired) => 3,
            _ => 1,
        })
    }

    fn report(&self) -> String {
        match self {
            Self::Api(YtmError::AuthExpired) => {
                "Session expired. Open the Kaset app to sign in again.".into()
            }
            _ => self.to_string(),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run_cli(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Output(error)) if error.kind() == io::ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.report());
            error.exit_code()
        }
    }
}

async fn run_cli(cli: Cli) -> Result<(), CliError> {
    let store: Box<dyn SessionStore> = match session_file(
        cli.session_file,
        std::env::var(STORE_ENV).ok().as_deref(),
        std::env::var_os(SESSION_FILE_ENV),
    )? {
        Some(path) => Box::new(FileStore::new(path)),
        None => Box::new(keyring_store()),
    };
    let output = run(store.as_ref(), cli.command, |jar, command| async move {
        let client = InnerTubeClient::new(ClientConfig::music(music_api_key().await?), Some(jar))?;
        execute(&client, command).await
    })
    .await?;

    let stdout = io::stdout();
    let dim_ids = stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    output.write(&mut stdout.lock(), cli.json, dim_ids)?;
    Ok(())
}

fn session_file(
    explicit: Option<PathBuf>,
    store_mode: Option<&str>,
    env_path: Option<OsString>,
) -> Result<Option<PathBuf>, CliError> {
    match (explicit, store_mode) {
        (Some(path), _) => Ok(Some(path)),
        (None, Some("file")) => env_path
            .filter(|path| !path.is_empty())
            .map(|path| Some(PathBuf::from(path)))
            .ok_or(CliError::MissingSessionFile),
        (None, _) => Ok(None),
    }
}

async fn run<F>(
    store: &dyn SessionStore,
    command: Command,
    request: impl FnOnce(CookieJar, Command) -> F,
) -> Result<Output, CliError>
where
    F: Future<Output = Result<Output, YtmError>>,
{
    let header = store.load()?.ok_or(CliError::MissingSession)?;
    let jar = CookieJar::from_header(header.expose());
    if jar.sapisid().is_none() {
        return Err(YtmError::AuthExpired.into());
    }
    Ok(request(jar, command).await?)
}

async fn execute(client: &InnerTubeClient, command: Command) -> Result<Output, YtmError> {
    Ok(match command {
        Command::Search {
            query,
            filter,
            limit,
        } => {
            let mut results = parse_search(&client.search(&query).await?);
            if let Some(filter) = filter {
                results.items.retain(|item| filter.matches(item));
            }
            if let Some(limit) = limit {
                results.items.truncate(limit);
            }
            Output::Search(results)
        }
        Command::Feed(feed) => Output::Feed(parse_feed(&client.browse(feed.browse_id()).await?)?),
        Command::Page { browse_id } => {
            Output::Page(parse_browse_page(&client.browse(&browse_id).await?)?)
        }
        Command::Radio { video_id } => Output::Radio(parse_radio(&client.radio(&video_id).await?)?),
        Command::Whoami => Output::Account(parse_account(&client.accounts_list().await?)),
    })
}
