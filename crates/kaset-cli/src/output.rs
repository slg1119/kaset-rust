use std::io::{self, Write};

use serde_json::Value;
use ytm_core::models::{
    Account, BrowseCard, BrowsePage, FeedPage, RadioQueue, SearchItem, SearchResults, ShelfItem,
    Song,
};

pub(crate) enum Output {
    Search(SearchResults),
    Feed(FeedPage),
    Page(BrowsePage),
    Radio(RadioQueue),
    Account(Option<Account>),
}

impl Output {
    pub(crate) fn write(&self, mut out: impl Write, json: bool, dim_ids: bool) -> io::Result<()> {
        if json {
            let mut value = match self {
                Self::Search(results) => serde_json::to_value(results),
                Self::Feed(feed) => serde_json::to_value(feed),
                Self::Page(page) => serde_json::to_value(page),
                Self::Radio(queue) => serde_json::to_value(queue),
                Self::Account(account) => serde_json::to_value(account),
            }?;
            redact_request_tokens(&mut value);
            serde_json::to_writer_pretty(&mut out, &value)?;
            return writeln!(out);
        }

        let mut text = Text { out, dim_ids };
        match self {
            Self::Search(results) => {
                if results.items.is_empty() {
                    writeln!(text.out, "No results.")?;
                }
                for item in &results.items {
                    match item {
                        SearchItem::Song(song) => text.song("Song  ", song)?,
                        SearchItem::Video(song) => text.song("Video  ", song)?,
                        SearchItem::Episode(song) => text.song("Episode  ", song)?,
                        SearchItem::Album(card) => text.card("Album  ", card)?,
                        SearchItem::Playlist(card) => text.card("Playlist  ", card)?,
                        SearchItem::Artist(card) => text.card("Artist  ", card)?,
                        SearchItem::Profile(card) => text.card("Profile  ", card)?,
                        SearchItem::Podcast(card) => text.card("Podcast  ", card)?,
                    }
                }
            }
            Self::Feed(feed) => {
                if feed.sections.is_empty() {
                    writeln!(text.out, "No sections.")?;
                }
                for (index, section) in feed.sections.iter().enumerate() {
                    if index > 0 {
                        writeln!(text.out)?;
                    }
                    writeln!(text.out, "{}", plain(&section.title))?;
                    for item in &section.items {
                        match item {
                            ShelfItem::Song(song) => text.song("  Song  ", song)?,
                            ShelfItem::Album(card) => text.card("  Album  ", card)?,
                            ShelfItem::Playlist(card) => text.card("  Playlist  ", card)?,
                            ShelfItem::Artist(card) => text.card("  Artist  ", card)?,
                            ShelfItem::Category {
                                title, browse_id, ..
                            } => {
                                text.item("  Category  ", title, None, browse_id)?;
                            }
                        }
                    }
                }
            }
            Self::Page(page) => {
                writeln!(text.out, "{}", plain(&page.title))?;
                for subtitle in [&page.subtitle, &page.second_subtitle]
                    .into_iter()
                    .flatten()
                {
                    writeln!(text.out, "{}", plain(subtitle))?;
                }
                writeln!(text.out)?;
                text.tracks(&page.songs)?;
            }
            Self::Radio(queue) => {
                writeln!(text.out, "Up next")?;
                text.tracks(&queue.songs)?;
            }
            Self::Account(Some(account)) => match &account.handle {
                Some(handle) => writeln!(text.out, "{} ({})", plain(&account.name), plain(handle))?,
                None => writeln!(text.out, "{}", plain(&account.name))?,
            },
            Self::Account(None) => writeln!(text.out, "No account found.")?,
        }
        Ok(())
    }
}

/// Pagination and browse-parameter tokens are internal to the API. Preserve
/// the parsed response's fields without emitting their values.
fn redact_request_tokens(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if key == "continuation" || key == "params" {
                    *value = Value::Null;
                } else {
                    redact_request_tokens(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_request_tokens),
        _ => {}
    }
}

fn plain(text: &str) -> String {
    text.replace(char::is_control, " ")
}

struct Text<W> {
    out: W,
    dim_ids: bool,
}

impl<W: Write> Text<W> {
    fn item(
        &mut self,
        prefix: &str,
        title: &str,
        subtitle: Option<&str>,
        id: &str,
    ) -> io::Result<()> {
        write!(self.out, "{prefix}{}", plain(title))?;
        if let Some(subtitle) = subtitle.filter(|text| !text.is_empty()) {
            write!(self.out, " — {}", plain(subtitle))?;
        }
        let id = plain(id);
        if self.dim_ids {
            writeln!(self.out, "  \x1b[2m[{id}]\x1b[0m")
        } else {
            writeln!(self.out, "  [{id}]")
        }
    }

    fn song(&mut self, prefix: &str, song: &Song) -> io::Result<()> {
        let artists = song
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        self.item(prefix, &song.title, Some(&artists), &song.video_id)
    }

    fn card(&mut self, prefix: &str, card: &BrowseCard) -> io::Result<()> {
        self.item(
            prefix,
            &card.title,
            card.subtitle.as_deref(),
            &card.browse_id,
        )
    }

    fn tracks(&mut self, songs: &[Song]) -> io::Result<()> {
        if songs.is_empty() {
            writeln!(self.out, "No tracks.")?;
        }
        for (index, song) in songs.iter().enumerate() {
            self.song(&format!("{}. ", index + 1), song)?;
        }
        Ok(())
    }
}
