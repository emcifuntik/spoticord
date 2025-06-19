use anyhow::Result;
use chrono;
use log::error;
use poise::CreateReply;
use rspotify::{
    model::{SearchResult, PlayableId},
    prelude::*,
};
use serenity::all::{CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, AutocompleteChoice};
use spoticord_session::manager::SessionQuery;
use spoticord_utils::discord::Colors;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap;

use crate::bot::Context;

// Cache for autocomplete results with debouncing
static AUTOCOMPLETE_CACHE: OnceLock<Arc<Mutex<HashMap<String, (Vec<AutocompleteChoice>, Instant)>>>> = OnceLock::new();

fn get_cache() -> &'static Arc<Mutex<HashMap<String, (Vec<AutocompleteChoice>, Instant)>>> {
    AUTOCOMPLETE_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

const AUTOCOMPLETE_CACHE_DURATION: Duration = Duration::from_secs(300); // 5 minutes
const DEBOUNCE_DURATION: Duration = Duration::from_millis(1000); // 1 second

async fn track_autocomplete(
    ctx: Context<'_>,
    partial: &str,
) -> Vec<AutocompleteChoice> {
    if partial.len() < 2 {
        return vec![];
    }

    let partial = partial.to_lowercase();
    
    // Check cache first
    {
        let cache = get_cache().lock().unwrap();
        if let Some((choices, timestamp)) = cache.get(&partial) {
            if timestamp.elapsed() < AUTOCOMPLETE_CACHE_DURATION {
                return choices.clone();
            }
        }
    }

    // Debounce: wait a bit to see if user is still typing
    tokio::time::sleep(DEBOUNCE_DURATION).await;

    let manager = ctx.data();
    let storage = manager.storage();
    
    // Get Spotify credentials for search
    let credentials = match storage.get_spotify_credentials().await {
        Ok(Some(creds)) => creds,
        Ok(None) => return vec![],
        Err(_) => return vec![],
    };

    // Create Spotify client for searching
    let token = rspotify::Token {
        access_token: credentials.access_token.clone(),
        expires_in: chrono::TimeDelta::seconds(3600),
        expires_at: Some(credentials.expires_at),
        refresh_token: Some(credentials.refresh_token.clone()),
        scopes: std::collections::HashSet::new(),
    };

    let spotify = spoticord_config::get_spotify(token);

    // Search for tracks
    let search_result = match spotify
        .search(&partial, rspotify::model::SearchType::Track, None, None, Some(5), None)
        .await
    {
        Ok(result) => result,
        Err(_) => return vec![],
    };

    let choices = match search_result {
        SearchResult::Tracks(page) => {
            page.items
                .into_iter()
                .take(5)
                .map(|track| {
                    let artists = track
                        .artists
                        .iter()
                        .map(|a| a.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    
                    let name = format!("{} - {}", track.name, artists);
                    let value = format!("{} by {}", track.name, artists);
                    
                    AutocompleteChoice::new(name, value)
                })
                .collect()
        }
        _ => vec![],
    };

    // Cache the results
    {
        let mut cache = get_cache().lock().unwrap();
        cache.insert(partial, (choices.clone(), Instant::now()));
        
        // Clean old entries to prevent memory leak
        cache.retain(|_, (_, timestamp)| timestamp.elapsed() < AUTOCOMPLETE_CACHE_DURATION);
    }

    choices
}

/// Play a track (add to queue and start playback)
#[poise::command(slash_command)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "The track to search for and play"]
    #[autocomplete = "track_autocomplete"]
    query: String,
) -> Result<()> {
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
    };    // Create Spotify client with OAuth credentials
    let token = rspotify::Token {
        access_token: credentials.access_token.clone(),
        expires_in: chrono::TimeDelta::seconds(3600),
        expires_at: Some(credentials.expires_at),
        refresh_token: Some(credentials.refresh_token.clone()),
        scopes: std::collections::HashSet::new(),
    };

    let spotify = spoticord_config::get_spotify(token);

    ctx.defer().await?;

    // Search for tracks
    let search_result = match spotify
        .search(&query, rspotify::model::SearchType::Track, None, None, Some(5), None)
        .await
    {
        Ok(result) => result,
        Err(why) => {
            error!("Failed to search Spotify: {why}");
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Search failed")
                            .description("Failed to search for tracks on Spotify.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };    let track = match search_result {
        SearchResult::Tracks(page) => {
            if let Some(track) = page.items.into_iter().next() {
                track
            } else {
                ctx.send(
                    CreateReply::default()
                        .embed(
                            CreateEmbed::new()
                                .title("No results")
                                .description("No tracks found for your search query.")
                                .color(Colors::Error),
                        )
                        .ephemeral(true),
                )
                .await?;
                return Ok(());
            }
        }
        _ => {
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Search error")
                            .description("Unexpected search result type.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };    // Check current playback state first
    let track_id = track.id.as_ref().unwrap();
    let playback_state = spotify.current_playback(None, None::<Vec<_>>).await;
    
    match playback_state {
        Ok(Some(playback)) => {
            // There's an active playback session, we can add to queue
            let playable_id = PlayableId::Track(track_id.clone());
            match spotify.add_item_to_queue(playable_id, None).await {
                Ok(_) => {
                    // If playback is paused, resume it
                    if !playback.is_playing {
                        if let Err(why) = spotify.resume_playback(playback.device.id.as_deref(), None).await {
                            error!("Failed to resume playback: {why}");
                        }
                    }
                }
                Err(why) => {
                    error!("Failed to add track to queue: {why}");
                    ctx.send(
                        CreateReply::default()
                            .embed(
                                CreateEmbed::new()
                                    .title("Queue failed")
                                    .description("Failed to add track to Spotify queue.")
                                    .color(Colors::Error),
                            )
                            .ephemeral(true),
                    )
                    .await?;
                    return Ok(());
                }
            }
        }        Ok(None) => {
            // No active playback session, try to find our librespot device and transfer playback to it
            match spotify.device().await {
                Ok(devices) => {
                    // Look for our bot's device (you might need to adjust the name matching logic)
                    let bot_device = devices.iter().find(|device| {
                        device.name.contains("Spoticord") || device.name.contains("spoticord")
                    });
                      if let Some(device) = bot_device {
                        // Check if device has an ID
                        if let Some(device_id) = &device.id {
                            // Transfer playback to our device first
                            match spotify.transfer_playback(device_id, Some(true)).await {
                                Ok(_) => {
                                    // Now add the track to queue
                                    let playable_id = PlayableId::Track(track_id.clone());
                                    match spotify.add_item_to_queue(playable_id, Some(device_id)).await {
                                        Ok(_) => {
                                            // Successfully added to queue on our device
                                        }
                                        Err(why) => {
                                            error!("Failed to add track to queue after transfer: {why}");
                                            ctx.send(
                                                CreateReply::default()
                                                    .embed(
                                                        CreateEmbed::new()
                                                            .title("Queue failed")
                                                            .description("Failed to add track to queue after transferring playback.")
                                                            .color(Colors::Error),
                                                    )
                                                    .ephemeral(true),
                                            )
                                            .await?;
                                            return Ok(());
                                        }
                                    }
                                }
                                Err(why) => {
                                    error!("Failed to transfer playback to device: {why}");
                                    ctx.send(
                                        CreateReply::default()
                                            .embed(
                                                CreateEmbed::new()
                                                    .title("Transfer failed")
                                                    .description("Failed to transfer playback to bot device.")
                                                    .color(Colors::Error),
                                            )
                                            .ephemeral(true),
                                    )
                                    .await?;
                                    return Ok(());
                                }
                            }
                        } else {
                            // Device found but no ID, fallback to direct playback
                            let track_playable = PlayableId::Track(track_id.clone());
                            match spotify.start_uris_playback([track_playable], None, None, None).await {
                                Ok(_) => {
                                    // Successfully started playback
                                }
                                Err(why) => {
                                    error!("Failed to start playback: {why}");
                                    ctx.send(
                                        CreateReply::default()
                                            .embed(
                                                CreateEmbed::new()
                                                    .title("Playback failed")
                                                    .description("Found bot device but failed to start playback.")
                                                    .color(Colors::Error),
                                            )
                                            .ephemeral(true),
                                    )
                                    .await?;
                                    return Ok(());
                                }
                            }
                        }
                    } else {
                        // No bot device found, start playback directly with the track
                        let track_playable = PlayableId::Track(track_id.clone());
                        match spotify.start_uris_playback([track_playable], None, None, None).await {
                            Ok(_) => {
                                // Successfully started playback
                            }
                            Err(why) => {
                                error!("Failed to start playback: {why}");
                                ctx.send(
                                    CreateReply::default()
                                        .embed(
                                            CreateEmbed::new()
                                                .title("Playback failed")
                                                .description("Failed to start Spotify playback. Make sure you have an active Spotify device or the bot is properly connected.")
                                                .footer(CreateEmbedFooter::new("Open Spotify on any device and try again"))
                                                .color(Colors::Error),
                                        )
                                        .ephemeral(true),
                                )
                                .await?;
                                return Ok(());
                            }
                        }
                    }
                }
                Err(why) => {
                    error!("Failed to get devices: {why}");
                    ctx.send(
                        CreateReply::default()
                            .embed(
                                CreateEmbed::new()
                                    .title("Device query failed")
                                    .description("Failed to get available Spotify devices.")
                                    .color(Colors::Error),
                            )
                            .ephemeral(true),
                    )
                    .await?;
                    return Ok(());
                }
            }
        }
        Err(why) => {
            error!("Failed to check playback state: {why}");
            ctx.send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .title("Connection failed")
                            .description("Failed to connect to Spotify. Please try again.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    }

    let artists = track
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    let duration = track.duration.num_seconds();
    let duration_str = format!("{}:{:02}", duration / 60, duration % 60);

    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .author(
                        CreateEmbedAuthor::new("Now Playing")
                            .icon_url("https://spoticord.com/spotify-logo.png"),
                    )
                    .title(&track.name)
                    .description(format!("by {}", artists))
                    .field("Duration", duration_str, true)
                    .field("Popularity", format!("{}/100", track.popularity), true)
                    .footer(CreateEmbedFooter::new("Track is now playing on Spotify"))
                    .color(Colors::Success),
            )
            .ephemeral(false),
    )
    .await?;

    Ok(())
}
