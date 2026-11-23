# Kaset Domain Language

## Music Experience

YouTube Music browsing and playback. InnerTube supplies catalog and account
data. A hidden player WebView handles playback, and Rust owns playback state.

## Session Epoch

The generation of the current sign-in state. In-flight API calls and cookie
reads capture it so responses from an old account or sign-in state cannot
update the active session.

## Browse ID and Video ID

A browse ID opens an InnerTube feed or detail page, such as an album, playlist,
artist, or podcast show. A video ID identifies a playable track or episode.
An album browse ID and its library playlist ID serve different endpoints.

## Continuation

An opaque, request-specific cursor for the next page of a feed, library
collection, search, detail page, or radio queue. Use it only with the
endpoint and request that produced it.

## Queue and Play Order

The queue holds the loaded songs and the current entry. Play order is the
sequence shown in the queue panel, which can differ from insertion order
when shuffled. Queue editing commands address positions in play order.

## Radio

A YouTube Music mix seeded from a song or an artist's mix endpoint. The
player can append later pages as the queue runs low. Queue generation and
session checks prevent stale radio pages from changing current playback.

## Library

The signed-in user's saved collections, including playlists, albums,
artists, and subscribed podcasts. Account actions use InnerTube. The app
invalidates the affected queries after an accepted write.

## Playback Remote Control

The standalone CLI sends a fixed set of playback commands to a running
desktop app over a same-user local connection. The app handles these
commands through the same player service used by the UI.
