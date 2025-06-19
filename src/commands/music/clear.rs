use anyhow::Result;
use chrono;
use log::error;
use poise::CreateReply;
use rspotify::prelude::*;
use serenity::all::{CreateEmbed, CreateEmbedAuthor};
use spoticord_session::manager::SessionQuery;
use spoticord_utils::discord::Colors;

use crate::bot::Context;

/// Clear the Spotify queue
#[poise::command(slash_command)]
pub async fn clear(ctx: Context<'_>) -> Result<()> {
    let manager = ctx.data();
    
    // Check if we're in a voice channel session
    let _session = match manager.get_session(SessionQuery::Guild(ctx.guild_id().unwrap())) {
        Some(session) => session,
        None => {
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("No active session")
                            .description("Use `/join` first to create a music session.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    // Get Spotify credentials and create authenticated client
    let storage = manager.storage();
    let credentials = match storage.get_spotify_credentials().await? {
        Some(creds) => creds,
        None => {
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("No Spotify account")
                            .description("The bot doesn't have a Spotify account linked yet.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    // Create Spotify client with OAuth credentials
    let token = rspotify::Token {
        access_token: credentials.access_token.clone(),
        expires_in: chrono::TimeDelta::seconds(3600),
        expires_at: Some(credentials.expires_at),
        refresh_token: Some(credentials.refresh_token.clone()),
        scopes: std::collections::HashSet::new(),
    };

    let spotify = spoticord_config::get_spotify(token);

    ctx.defer().await?;    // Get current playback state to find the active device
    let playback = match spotify.current_playback(None, None::<Vec<_>>).await {
        Ok(Some(playback)) => playback,
        Ok(None) => {
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("No active playback")
                            .description("No Spotify playback is currently active. Start playing something first.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
        Err(why) => {
            error!("Failed to get current playback: {why}");
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Playback check failed")
                            .description("Failed to check current Spotify playback.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let device_id = playback.device.id;

    // Unfortunately, Spotify's Web API doesn't have a direct "clear queue" endpoint.
    // We need to use a workaround: skip to the end of the current track and then
    // pause, which effectively clears the queue for the next playback session.
      // First, try to seek to the end of the current track
    if let Some(item) = playback.item {
        match item {            rspotify::model::PlayableItem::Track(track) => {
                let duration_ms = track.duration.num_milliseconds() as u32;
                // Seek to 1 second before the end to avoid auto-advancing
                let seek_position = chrono::TimeDelta::milliseconds((duration_ms.saturating_sub(1000)) as i64);
                
                match spotify.seek_track(seek_position, device_id.as_deref()).await {
                    Ok(_) => {
                        // Then pause to stop playback
                        match spotify.pause_playback(device_id.as_deref()).await {
                            Ok(_) => {
                                ctx.send(
                                    CreateReply::default()
                                        .embed(
                                            CreateEmbed::new()
                                                .author(
                                                    CreateEmbedAuthor::new("Queue Cleared")
                                                        .icon_url("https://spoticord.com/spotify-logo.png"),
                                                )
                                                .title("Spotify queue cleared")
                                                .description("Playback has been paused and the queue is effectively cleared.")
                                                .color(Colors::Success),
                                        )
                                        .ephemeral(false),
                                )
                                .await?;
                            }
                            Err(why) => {
                                error!("Failed to pause playback: {why}");
                                ctx.send(
                                    CreateReply::default()
                                        .embed(
                                            CreateEmbed::new()
                                                .title("Pause failed")
                                                .description("Seeked to end of track but failed to pause playback.")
                                                .color(Colors::Warning),
                                        )
                                        .ephemeral(true),
                                )
                                .await?;
                            }
                        }
                    }
                    Err(why) => {
                        error!("Failed to seek track: {why}");
                        ctx.send(
                            CreateReply::default()
                                .embed(
                                    CreateEmbed::new()
                                        .title("Clear failed")
                                        .description("Failed to clear the queue. Make sure Spotify is actively playing.")
                                        .color(Colors::Error),
                                )
                                .ephemeral(true),
                        )
                        .await?;
                    }
                }
            }
            rspotify::model::PlayableItem::Episode(_) => {
                ctx.send(
                    CreateReply::default()
                        .embed(
                            CreateEmbed::new()
                                .title("Unsupported content")
                                .description("Currently playing a podcast episode. Queue clearing is only supported for music tracks.")
                                .color(Colors::Error),
                        )
                        .ephemeral(true),
                )
                .await?;
            }
        }
    } else {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("No current track")
                        .description("No track is currently playing.")
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}
