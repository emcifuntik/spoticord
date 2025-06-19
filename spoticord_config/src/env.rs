use std::sync::LazyLock;

pub static DISCORD_TOKEN: LazyLock<String> = LazyLock::new(|| {
    std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN environment variable")
});
pub static BASE_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
});
pub static WEB_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("WEB_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("WEB_PORT must be a valid port number")
});
pub static DATA_DIR: LazyLock<String> = LazyLock::new(|| {
    std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string())
});
pub static SPOTIFY_CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    std::env::var("SPOTIFY_CLIENT_ID").expect("missing SPOTIFY_CLIENT_ID environment variable")
});
pub static SPOTIFY_CLIENT_SECRET: LazyLock<String> = LazyLock::new(|| {
    std::env::var("SPOTIFY_CLIENT_SECRET")
        .expect("missing SPOTIFY_CLIENT_SECRET environment variable")
});
