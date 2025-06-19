mod bot;
mod commands;

use log::{error, info};
use poise::Framework;
use serenity::all::ClientBuilder;
use songbird::SerenityInit;
use spoticord_storage::Storage;
use spoticord_web::WebServer;

#[tokio::main]
async fn main() {
    // Force aws-lc-rs as default crypto provider
    // Since multiple dependencies either enable aws_lc_rs or ring, they cause a clash, so we have to
    // explicitly tell rustls to use the aws-lc-rs provider
    _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Setup logging
    if std::env::var("RUST_LOG").is_err() {
        #[cfg(debug_assertions)]
        std::env::set_var("RUST_LOG", "spoticord");

        #[cfg(not(debug_assertions))]
        std::env::set_var("RUST_LOG", "spoticord=info");
    }

    env_logger::init();

    info!("Today is a good day!");
    info!(" - Spoticord");

    dotenvy::dotenv().ok();

    // Set up storage
    let storage = Storage::new(spoticord_config::data_dir());
    if let Err(why) = storage.init().await {
        error!("Failed to initialize storage: {why}");
        return;
    }

    // Start web server for OAuth
    let web_server = WebServer::new(storage.clone());
    let web_port = spoticord_config::web_port();
    
    tokio::spawn(async move {
        if let Err(why) = web_server.start(web_port).await {
            error!("Web server error: {why}");
        }
    });

    info!("Web server starting on port {}", web_port);
    info!("Visit {} to set up Spotify authentication", spoticord_config::base_url());

    // Set up bot
    let framework = Framework::builder()
        .setup(|ctx, ready, framework| Box::pin(bot::setup(ctx, ready, framework, storage)))
        .options(bot::framework_opts())
        .build();

    let mut client = match ClientBuilder::new(
        spoticord_config::discord_token(),
        spoticord_config::discord_intents(),
    )
    .framework(framework)
    .register_songbird_from_config(songbird::Config::default().use_softclip(false))
    .await
    {
        Ok(client) => client,
        Err(why) => {
            error!("Fatal error when building Serenity client: {why}");
            return;
        }
    };

    if let Err(why) = client.start_autosharded().await {
        error!("Fatal error occured during bot operations: {why}");
        error!("Bot will now shut down!");
    }
}
