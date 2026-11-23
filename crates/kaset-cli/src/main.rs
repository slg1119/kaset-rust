mod args;
mod output;
#[cfg(test)]
mod remote_tests;
#[cfg(test)]
mod tests;

use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use kaset_ipc::ClientError;
use kaset_session::{
    FileStore, SESSION_FILE_ENV, STORE_ENV, SessionStore, StoreError, keyring_store,
};
use ytm_core::YtmError;
use ytm_core::api_key::music_api_key;
use ytm_core::auth::CookieJar;
use ytm_core::client::{ClientConfig, InnerTubeClient, playlist_browse_id};
use ytm_core::parsers::account::parse_account;
use ytm_core::parsers::browse_page::parse_browse_page_for_id;
use ytm_core::parsers::home::parse_feed;
use ytm_core::parsers::radio::parse_radio;
use ytm_core::parsers::search::parse_search;

use args::{ApiCommand, Cli, Command};
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
    #[error(transparent)]
    Playback(#[from] ClientError),
    #[error("could not write output: {0}")]
    Output(#[from] io::Error),
}

impl CliError {
    fn exit_code(&self) -> ExitCode {
        ExitCode::from(match self {
            Self::MissingSession
            | Self::Playback(ClientError::Rejected(kaset_ipc::Error::NotSignedIn)) => 2,
            Self::Api(YtmError::AuthExpired)
            | Self::Playback(ClientError::Rejected(kaset_ipc::Error::AuthExpired)) => 3,
            Self::Playback(ClientError::NotRunning) => 4,
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
    let stdout = io::stdout();
    let dim_ids = dim_ids(
        stdout.is_terminal(),
        std::env::var_os("NO_COLOR").as_deref(),
    );
    let command = match cli.command {
        Command::Api(command) => command,
        Command::Playback(command) => {
            let response = kaset_ipc::request(command.into()).await?;
            output::write_playback(stdout.lock(), &response, cli.json, dim_ids)?;
            return Ok(());
        }
    };
    #[cfg(debug_assertions)]
    let explicit_file = cli.session_file;
    #[cfg(not(debug_assertions))]
    let explicit_file = None;
    let store: Box<dyn SessionStore> = match session_file(
        explicit_file,
        std::env::var(STORE_ENV).ok().as_deref(),
        std::env::var_os(SESSION_FILE_ENV),
    )? {
        Some(path) => Box::new(FileStore::new(path)),
        None => Box::new(keyring_store()),
    };
    let output = run(store.as_ref(), command, |jar, command| async move {
        let client = InnerTubeClient::new(ClientConfig::music(music_api_key().await?), Some(jar))?;
        execute(&client, command).await
    })
    .await?;

    output.write(&mut stdout.lock(), cli.json, dim_ids)?;
    Ok(())
}

fn dim_ids(is_terminal: bool, no_color: Option<&OsStr>) -> bool {
    is_terminal && no_color.is_none_or(OsStr::is_empty)
}

fn session_file(
    explicit: Option<PathBuf>,
    store_mode: Option<&str>,
    env_path: Option<OsString>,
) -> Result<Option<PathBuf>, CliError> {
    if !cfg!(debug_assertions) {
        return Ok(None);
    }
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
    command: ApiCommand,
    request: impl FnOnce(CookieJar, ApiCommand) -> F,
) -> Result<Output, CliError>
where
    F: Future<Output = Result<Output, YtmError>>,
{
    let mut retries = 0;
    let header = loop {
        match store.load() {
            Err(StoreError::Corrupt(_)) if retries < 3 => {
                retries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            result => break result?.ok_or(CliError::MissingSession)?,
        }
    };
    let jar = CookieJar::from_header(header.expose());
    if jar.sapisid().is_none() {
        return Err(YtmError::AuthExpired.into());
    }
    Ok(request(jar, command).await?)
}

async fn execute(client: &InnerTubeClient, command: ApiCommand) -> Result<Output, YtmError> {
    Ok(match command {
        ApiCommand::Search {
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
        ApiCommand::Feed(feed) => {
            Output::Feed(parse_feed(&client.browse(feed.browse_id()).await?)?)
        }
        ApiCommand::Page { browse_id } => {
            let browse_id = playlist_browse_id(&browse_id);
            Output::Page(parse_browse_page_for_id(
                &client.browse(&browse_id).await?,
                &browse_id,
            )?)
        }
        ApiCommand::Radio { video_id } => {
            Output::Radio(parse_radio(&client.radio(&video_id).await?)?)
        }
        ApiCommand::Whoami => Output::Account(parse_account(&client.accounts_list().await?)),
    })
}
