use anyhow::Result;
use chrono;
use log::error;
use poise::CreateReply;
use rspotify::clients::OAuthClient;
use serenity::all::CreateEmbed;
use spoticord_session::manager::SessionQuery;
use spoticord_utils::discord::Colors;

use crate::bot::Context;

/// Skip the current track
#[poise::command(slash_command)]
pub async fn skip(ctx: Context<'_>) -> Result<()> {
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
                            .description("There's no active music session to skip tracks in.")
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
    let mut credentials = match storage.get_spotify_credentials().await? {
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

    // Refresh token if needed and save if updated
    if credentials.refresh_if_needed().await? {
        storage.save_spotify_credentials(&credentials).await?;
    }

    // Create Spotify client with OAuth credentials
    let token = rspotify::Token {
        access_token: credentials.access_token.clone(),
        expires_in: chrono::TimeDelta::seconds(3600),
        expires_at: Some(credentials.expires_at),
        refresh_token: Some(credentials.refresh_token.clone()),
        scopes: std::collections::HashSet::new(),
    };

    let spotify = spoticord_config::get_spotify(token);    // Skip to next track on Spotify  
    match spotify.next_track(None).await {
        Ok(_) => {
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("⏭️ Track Skipped")
                            .description("Skipped to the next track on Spotify.")
                            .color(Colors::Success),
                    )
                    .ephemeral(false),
            )
            .await?;
        }
        Err(why) => {
            error!("Failed to skip track: {why}");
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Skip failed")
                            .description("Failed to skip track. Make sure Spotify is actively playing.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}
