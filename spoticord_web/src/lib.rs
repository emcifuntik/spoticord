use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use log::{error, info};
use rspotify::{prelude::*, AuthCodeSpotify, Config, Credentials, OAuth, scopes, model::PlayableId};
use serde::{Deserialize, Serialize};
use spoticord_storage::{SpotifyCredentials, Storage};
use std::sync::Arc;

#[derive(Clone)]
pub struct WebServer {
    storage: Storage,
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    error: Option<String>,
    #[allow(dead_code)]
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlayTrackRequest {
    query: String,
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

impl WebServer {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub async fn start(&self, port: u16) -> Result<()> {        let app = Router::new()
            .route("/", get(index_handler))
            .route("/auth", get(auth_handler))
            .route("/callback", get(callback_handler))
            .route("/api/play", post(play_track_handler))
            .route("/api/queue/clear", post(clear_queue_handler))
            .with_state(Arc::new(self.clone()));

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
            .await
            .context("Failed to bind to address")?;

        info!("Web server listening on port {}", port);

        axum::serve(listener, app)
            .await
            .context("Web server error")?;

        Ok(())
    }

    pub fn get_auth_url(&self) -> Result<String> {
        let spotify = self.create_spotify_client();
        let auth_url = spotify.get_authorize_url(false)?;
        Ok(auth_url)
    }

    fn create_spotify_client(&self) -> AuthCodeSpotify {
        let oauth = OAuth {
            redirect_uri: format!("{}/callback", spoticord_config::base_url()),
            scopes: scopes!(
                "user-read-playback-state",
                "user-modify-playback-state",
                "user-read-currently-playing",
                "user-read-private",
                "user-read-email",
                "streaming"
            ),
            ..Default::default()
        };

        AuthCodeSpotify::with_config(
            Credentials {
                id: spoticord_config::spotify_client_id().to_string(),
                secret: Some(spoticord_config::spotify_client_secret().to_string()),
            },
            oauth,
            Config::default(),
        )
    }
}

async fn index_handler() -> Html<&'static str> {
    Html(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Spoticord - Spotify Authentication</title>
            <style>
                body {
                    font-family: Arial, sans-serif;
                    max-width: 600px;
                    margin: 50px auto;
                    padding: 20px;
                    background-color: #f5f5f5;
                }
                .container {
                    background: white;
                    padding: 30px;
                    border-radius: 10px;
                    box-shadow: 0 2px 10px rgba(0,0,0,0.1);
                    text-align: center;
                }
                .btn {
                    display: inline-block;
                    background-color: #1DB954;
                    color: white;
                    padding: 12px 24px;
                    text-decoration: none;
                    border-radius: 25px;
                    font-weight: bold;
                    margin-top: 20px;
                    transition: background-color 0.3s;
                }
                .btn:hover {
                    background-color: #1ed760;
                }
                h1 {
                    color: #333;
                }
                p {
                    color: #666;
                    line-height: 1.6;
                }
            </style>
        </head>
        <body>
            <div class="container">
                <h1>🎵 Spoticord Setup</h1>
                <p>Welcome! This bot needs to connect to a Spotify account to play music in your Discord server.</p>
                <p>Click the button below to authenticate with Spotify. This will allow the bot to control music playback.</p>
                <a href="/auth" class="btn">Connect Spotify Account</a>
            </div>
        </body>
        </html>
        "#,
    )
}

async fn auth_handler(State(server): State<Arc<WebServer>>) -> impl IntoResponse {
    match server.get_auth_url() {
        Ok(auth_url) => {
            // Redirect to Spotify authorization
            (StatusCode::FOUND, [("Location", auth_url)]).into_response()
        }
        Err(e) => {
            error!("Failed to get auth URL: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get auth URL").into_response()
        }
    }
}

async fn callback_handler(
    Query(params): Query<CallbackQuery>,
    State(server): State<Arc<WebServer>>,
) -> impl IntoResponse {
    if let Some(error) = params.error {
        error!("OAuth error: {}", error);
        return Html(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Spoticord - Error</title>
                <style>
                    body { font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; background-color: #f5f5f5; }
                    .container { background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); text-align: center; }
                    .error { color: #d32f2f; }
                </style>
            </head>
            <body>
                <div class="container">
                    <h1 class="error">❌ Authentication Failed</h1>
                    <p>There was an error connecting to Spotify. Please try again.</p>
                </div>
            </body>
            </html>
            "#,
        );
    }

    let Some(code) = params.code else {
        return Html(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Spoticord - Error</title>
                <style>
                    body { font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; background-color: #f5f5f5; }
                    .container { background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); text-align: center; }
                    .error { color: #d32f2f; }
                </style>
            </head>
            <body>
                <div class="container">
                    <h1 class="error">❌ Missing Authorization Code</h1>
                    <p>No authorization code was provided. Please try again.</p>
                </div>
            </body>
            </html>
            "#,
        );
    };    // Exchange code for token
    #[allow(unused_mut)]
    let mut spotify = server.create_spotify_client();
    
    match spotify.request_token(&code).await {
        Ok(()) => {
            // Get token and save to storage
            if let Some(token) = spotify.get_token().lock().await.unwrap().clone() {
                let credentials = SpotifyCredentials::new(
                    token.access_token,
                    token.refresh_token.unwrap_or_default(),
                    token.expires_at.unwrap_or_else(|| {
                        Utc::now() + chrono::Duration::hours(1)
                    }),
                );

                match server.storage.save_spotify_credentials(&credentials).await {
                    Ok(()) => {
                        info!("Successfully saved Spotify credentials");
                        Html(
                            r#"
                            <!DOCTYPE html>
                            <html>
                            <head>
                                <title>Spoticord - Success</title>
                                <style>
                                    body { font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; background-color: #f5f5f5; }
                                    .container { background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); text-align: center; }
                                    .success { color: #2e7d32; }
                                </style>
                            </head>
                            <body>
                                <div class="container">
                                    <h1 class="success">✅ Success!</h1>
                                    <p>Your Spotify account has been successfully connected to Spoticord.</p>
                                    <p>You can now close this window and use the bot in your Discord server.</p>
                                </div>
                            </body>
                            </html>
                            "#,
                        )
                    }
                    Err(e) => {
                        error!("Failed to save credentials: {}", e);
                        Html(
                            r#"
                            <!DOCTYPE html>
                            <html>
                            <head>
                                <title>Spoticord - Error</title>
                                <style>
                                    body { font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; background-color: #f5f5f5; }
                                    .container { background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); text-align: center; }
                                    .error { color: #d32f2f; }
                                </style>
                            </head>
                            <body>
                                <div class="container">
                                    <h1 class="error">❌ Storage Error</h1>
                                    <p>Failed to save authentication credentials. Please try again.</p>
                                </div>
                            </body>
                            </html>
                            "#,
                        )
                    }
                }
            } else {
                error!("No token received from Spotify");
                Html(
                    r#"
                    <!DOCTYPE html>
                    <html>
                    <head>
                        <title>Spoticord - Error</title>
                        <style>
                            body { font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; background-color: #f5f5f5; }
                            .container { background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); text-align: center; }
                            .error { color: #d32f2f; }
                        </style>
                    </head>
                    <body>
                        <div class="container">
                            <h1 class="error">❌ Token Error</h1>
                            <p>Failed to receive token from Spotify. Please try again.</p>
                        </div>
                    </body>
                    </html>
                    "#,
                )
            }
        }
        Err(e) => {
            error!("Failed to request token: {}", e);
            Html(
                r#"
                <!DOCTYPE html>
                <html>
                <head>
                    <title>Spoticord - Error</title>
                    <style>
                        body { font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; background-color: #f5f5f5; }
                        .container { background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); text-align: center; }
                        .error { color: #d32f2f; }
                    </style>
                </head>
                <body>
                    <div class="container">
                        <h1 class="error">❌ Authentication Failed</h1>
                        <p>Failed to authenticate with Spotify. Please try again.</p>
                    </div>
                </body>
                </html>
                "#,
            )
        }
    }
}

async fn play_track_handler(
    State(server): State<Arc<WebServer>>,
    Json(request): Json<PlayTrackRequest>,
) -> impl IntoResponse {
    // Get Spotify credentials
    let credentials = match server.storage.get_spotify_credentials().await {
        Ok(Some(creds)) => creds,
        Ok(None) => {
            return Json(ApiResponse {
                success: false,
                message: "No Spotify account linked".to_string(),
            });
        }
        Err(e) => {
            error!("Failed to get credentials: {}", e);
            return Json(ApiResponse {
                success: false,
                message: "Failed to get credentials".to_string(),
            });
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

    // Search for tracks
    let search_result = match spotify
        .search(&request.query, rspotify::model::SearchType::Track, None, None, Some(5), None)
        .await
    {
        Ok(result) => result,
        Err(why) => {
            error!("Failed to search Spotify: {why}");
            return Json(ApiResponse {
                success: false,
                message: "Failed to search for tracks".to_string(),
            });
        }
    };

    let track = match search_result {
        rspotify::model::SearchResult::Tracks(page) => {
            if let Some(track) = page.items.into_iter().next() {
                track
            } else {
                return Json(ApiResponse {
                    success: false,
                    message: "No tracks found for your search query".to_string(),
                });
            }
        }
        _ => {
            return Json(ApiResponse {
                success: false,
                message: "Unexpected search result type".to_string(),
            });
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
                    return Json(ApiResponse {
                        success: false,
                        message: "Failed to add track to queue".to_string(),
                    });
                }
            }
        }        Ok(None) => {
            // No active playback session, try to find our librespot device and transfer playback to it
            match spotify.device().await {
                Ok(devices) => {
                    // Look for our bot's device
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
                                            return Json(ApiResponse {
                                                success: false,
                                                message: "Failed to add track to queue after transferring playback".to_string(),
                                            });
                                        }
                                    }
                                }
                                Err(why) => {
                                    error!("Failed to transfer playback to device: {why}");
                                    return Json(ApiResponse {
                                        success: false,
                                        message: "Failed to transfer playback to bot device".to_string(),
                                    });
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
                                    return Json(ApiResponse {
                                        success: false,
                                        message: "Found bot device but failed to start playback".to_string(),
                                    });
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
                                return Json(ApiResponse {
                                    success: false,
                                    message: "Failed to start playback. Make sure you have an active Spotify device.".to_string(),
                                });
                            }
                        }
                    }
                }
                Err(why) => {
                    error!("Failed to get devices: {why}");
                    return Json(ApiResponse {
                        success: false,
                        message: "Failed to get available Spotify devices".to_string(),
                    });
                }
            }
        }
        Err(why) => {
            error!("Failed to check playback state: {why}");
            return Json(ApiResponse {
                success: false,
                message: "Failed to connect to Spotify".to_string(),
            });
        }
    }

    let artists = track
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    Json(ApiResponse {
        success: true,
        message: format!("Now playing '{}' by {}", track.name, artists),
    })
}

async fn clear_queue_handler(State(server): State<Arc<WebServer>>) -> impl IntoResponse {
    // Get Spotify credentials
    let credentials = match server.storage.get_spotify_credentials().await {
        Ok(Some(creds)) => creds,
        Ok(None) => {
            return Json(ApiResponse {
                success: false,
                message: "No Spotify account linked".to_string(),
            });
        }
        Err(e) => {
            error!("Failed to get credentials: {}", e);
            return Json(ApiResponse {
                success: false,
                message: "Failed to get credentials".to_string(),
            });
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

    // Get current playback state
    let playback = match spotify.current_playback(None, None::<Vec<_>>).await {
        Ok(Some(playback)) => playback,
        Ok(None) => {
            return Json(ApiResponse {
                success: false,
                message: "No active Spotify playback".to_string(),
            });
        }
        Err(why) => {
            error!("Failed to get current playback: {why}");
            return Json(ApiResponse {
                success: false,
                message: "Failed to check current playback".to_string(),
            });
        }
    };

    let device_id = playback.device.id;

    // Clear queue by seeking to end and pausing
    if let Some(item) = playback.item {
        match item {
            rspotify::model::PlayableItem::Track(track) => {
                let duration_ms = track.duration.num_milliseconds() as u32;
                let seek_position = chrono::TimeDelta::milliseconds((duration_ms.saturating_sub(1000)) as i64);
                
                match spotify.seek_track(seek_position, device_id.as_deref()).await {
                    Ok(_) => {
                        match spotify.pause_playback(device_id.as_deref()).await {
                            Ok(_) => {
                                Json(ApiResponse {
                                    success: true,
                                    message: "Queue cleared successfully".to_string(),
                                })
                            }
                            Err(why) => {
                                error!("Failed to pause playback: {why}");
                                Json(ApiResponse {
                                    success: false,
                                    message: "Seeked to end but failed to pause playback".to_string(),
                                })
                            }
                        }
                    }
                    Err(why) => {
                        error!("Failed to seek track: {why}");
                        Json(ApiResponse {
                            success: false,
                            message: "Failed to clear queue".to_string(),
                        })
                    }
                }
            }
            rspotify::model::PlayableItem::Episode(_) => {
                Json(ApiResponse {
                    success: false,
                    message: "Queue clearing not supported for podcast episodes".to_string(),
                })
            }
        }
    } else {
        Json(ApiResponse {
            success: false,
            message: "No current track".to_string(),
        })
    }
}
