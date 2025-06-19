mod env;

use rspotify::{AuthCodeSpotify, Config, Credentials, OAuth, Token};
use serenity::all::GatewayIntents;

#[cfg(not(debug_assertions))]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(debug_assertions)]
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-dev");

/// The "listening to" message that shows up under the Spoticord bot user
pub const MOTD: &str = "some good 'ol music";

/// The time it takes (in seconds) for Spoticord to disconnect when no music is being played
pub const DISCONNECT_TIME: u64 = 5 * 60;

pub fn discord_token() -> &'static str {
    &env::DISCORD_TOKEN
}

pub fn discord_intents() -> GatewayIntents {
    GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES
}

pub fn base_url() -> &'static str {
    &env::BASE_URL
}

pub fn web_port() -> u16 {
    *env::WEB_PORT
}

pub fn data_dir() -> &'static str {
    &env::DATA_DIR
}

pub fn spotify_client_id() -> &'static str {
    &env::SPOTIFY_CLIENT_ID
}

pub fn spotify_client_secret() -> &'static str {
    &env::SPOTIFY_CLIENT_SECRET
}

pub fn get_spotify(token: Token) -> AuthCodeSpotify {
    AuthCodeSpotify::from_token_with_config(
        token,
        Credentials {
            id: env::SPOTIFY_CLIENT_ID.to_string(),
            secret: Some(env::SPOTIFY_CLIENT_SECRET.to_string()),
        },
        OAuth::default(),
        Config::default(),
    )
}
