use anyhow::Result;
use log::error;
use poise::CreateReply;
use serenity::all::{
    CreateActionRow, CreateButton, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
use spoticord_utils::discord::Colors;

use crate::bot::{Context, FrameworkError};

/// Link the bot's Spotify account (Admin only)
#[poise::command(slash_command, on_error = on_error)]
pub async fn link(ctx: Context<'_>) -> Result<()> {
    // Check if the user has permission to link the bot's account
    // For simplicity, we'll allow anyone for now, but in production you might want to restrict this
    let storage = ctx.data().storage();
    
    // Check if Spotify is already linked
    if storage.get_spotify_credentials().await?.is_some() {
        ctx.send(
            CreateReply::default().embed(
                CreateEmbed::new()
                    .title("Spotify account already linked")
                    .description("The bot already has a Spotify account linked. If you need to re-link, contact the bot administrator.")
                    .color(Colors::Info),
            ).ephemeral(true),
        )
        .await?;

        return Ok(());
    }

    // Direct to web interface for linking
    let link = spoticord_config::base_url();

    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .author(
                        CreateEmbedAuthor::new("Link Spotify account")
                            .url(link)
                            .icon_url("https://spoticord.com/spotify-logo.png"),
                    )
                    .description("Click on the button below to link the bot's Spotify account.")
                    .footer(CreateEmbedFooter::new(
                        "This will allow the bot to play music for everyone in this server.",
                    ))
                    .color(Colors::Info),
            )
            .components(vec![CreateActionRow::Buttons(vec![
                CreateButton::new_link(link).label("Link Spotify Account"),
            ])])
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

async fn on_error(error: FrameworkError<'_>) {
    if let FrameworkError::Command { error, ctx, .. } = error {
        error!("An error occured during linking of new account: {error}");

        _ = ctx
            .send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .description("An error occured whilst trying to link your account.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await;
    } else {
        error!("{error}")
    }
}
