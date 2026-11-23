use serde_json::{Map, Value, json};

use super::InnerTubeClient;
use crate::YtmError;
use crate::models::{AddToPlaylistMenu, LikeStatus, PlaylistPrivacy, SongMetadata};
use crate::parsers::account_actions::{
    created_playlist_id, parse_add_to_playlist, parse_song_metadata_for_video,
};
use crate::secret::Secret;

impl InnerTubeClient {
    pub async fn song_metadata(&self, video_id: &str) -> Result<SongMetadata, YtmError> {
        let value = self.playback_queue(None, Some(video_id), None).await?;
        parse_song_metadata_for_video(&value, Some(video_id))
    }

    pub async fn rate_song(&self, video_id: &str, rating: LikeStatus) -> Result<(), YtmError> {
        let endpoint = match rating {
            LikeStatus::Like => "like/like",
            LikeStatus::Dislike => "like/dislike",
            LikeStatus::Indifferent => "like/removelike",
        };
        self.account_action(endpoint, json!({"target": {"videoId": video_id}}))
            .await?;
        Ok(())
    }

    pub async fn edit_library(&self, token: &Secret) -> Result<(), YtmError> {
        self.account_action("feedback", json!({"feedbackTokens": [token.expose()]}))
            .await?;
        Ok(())
    }

    pub async fn add_to_playlist_options(
        &self,
        video_id: &str,
    ) -> Result<AddToPlaylistMenu, YtmError> {
        let value = self
            .account_action(
                "playlist/get_add_to_playlist",
                json!({"videoIds": [video_id]}),
            )
            .await?;
        Ok(parse_add_to_playlist(&value))
    }

    pub async fn add_to_playlist(&self, video_id: &str, playlist_id: &str) -> Result<(), YtmError> {
        self.edit_playlist(
            playlist_id,
            json!({
                "action": "ACTION_ADD_VIDEO",
                "addedVideoId": video_id,
            }),
        )
        .await
    }

    pub async fn remove_from_playlist(
        &self,
        playlist_id: &str,
        video_id: &str,
        set_video_id: &str,
    ) -> Result<(), YtmError> {
        self.edit_playlist(
            playlist_id,
            json!({
                "action": "ACTION_REMOVE_VIDEO",
                "removedVideoId": video_id,
                "setVideoId": set_video_id,
            }),
        )
        .await
    }

    async fn edit_playlist(&self, playlist_id: &str, action: Value) -> Result<(), YtmError> {
        self.account_action(
            "browse/edit_playlist",
            json!({
                "playlistId": bare_playlist_id(playlist_id),
                "actions": [action],
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        video_ids: &[String],
    ) -> Result<String, YtmError> {
        let mut body = Map::from_iter([
            ("title".into(), json!(title)),
            ("privacyStatus".into(), json!(privacy)),
        ]);
        if !description.trim().is_empty() {
            body.insert("description".into(), json!(description));
        }
        if !video_ids.is_empty() {
            body.insert("videoIds".into(), json!(video_ids));
        }
        let result = self.post_with_auth("playlist/create", body, true).await?;
        created_playlist_id(&result)
            .map(str::to_owned)
            .ok_or_else(|| YtmError::Parse {
                endpoint: "playlist/create".into(),
                path: "playlistId".into(),
            })
    }

    pub async fn delete_playlist(&self, playlist_id: &str) -> Result<(), YtmError> {
        self.account_action(
            "playlist/delete",
            json!({"playlistId": bare_playlist_id(playlist_id)}),
        )
        .await?;
        Ok(())
    }

    pub async fn set_artist_subscribed(
        &self,
        channel_id: &str,
        subscribed: bool,
    ) -> Result<(), YtmError> {
        let endpoint = if subscribed {
            "subscription/subscribe"
        } else {
            "subscription/unsubscribe"
        };
        self.account_action(endpoint, json!({"channelIds": [channel_id]}))
            .await?;
        Ok(())
    }

    pub async fn set_playlist_saved(&self, playlist_id: &str, saved: bool) -> Result<(), YtmError> {
        let endpoint = if saved {
            "like/like"
        } else {
            "like/removelike"
        };
        self.account_action(
            endpoint,
            json!({"target": {"playlistId": bare_playlist_id(playlist_id)}}),
        )
        .await?;
        Ok(())
    }

    async fn account_action(&self, endpoint: &str, body: Value) -> Result<Value, YtmError> {
        let Value::Object(body) = body else {
            unreachable!("account action bodies are JSON objects")
        };
        self.post_with_auth(endpoint, body, true).await
    }
}

fn bare_playlist_id(playlist_id: &str) -> &str {
    playlist_id.strip_prefix("VL").unwrap_or(playlist_id)
}
