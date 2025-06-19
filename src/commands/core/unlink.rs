use anyhow::Result;
use log::error;
use poise::CreateReply;
use serenity::all::{CreateEmbed, CreateEmbedFooter};
use spoticord_utils::discord::Colors;

use crate::bot::{Context, FrameworkError};

/// Unlink the bot's Spotify account (Admin only)
#[poise::command(slash_command, on_error = on_error)]
pub async fn unlink(ctx: Context<'_>) -> Result<()> {
    let manager = ctx.data();
    let storage = manager.storage();

    // Disconnect all sessions since we're unlinking the central account
    manager.shutdown_all().await;

    // Check if there's actually a linked account
    let has_credentials = storage.get_spotify_credentials().await?.is_some();

    if !has_credentials {
        ctx.send(
            CreateReply::default()
                .embed(
                    CreateEmbed::new()
                        .title("No Spotify account linked")
                        .description(
                            "The bot doesn't have a Spotify account linked.",
                        )
                        .footer(CreateEmbedFooter::new(
                            "You can use /link to link a Spotify account.",
                        ))
                        .color(Colors::Error),
                )
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    // For now, we'll just inform the user that they need to manually remove the credentials file
    // In a production setup, you might want to implement actual file deletion
    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("Unlink Request")
                    .description(
                        "To unlink the Spotify account, please contact the bot administrator to remove the credentials.",
                    )
                    .footer(CreateEmbedFooter::new(
                        "All music sessions have been stopped.",
                    ))
                    .color(Colors::Info),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

async fn on_error(error: FrameworkError<'_>) {
    if let FrameworkError::Command { error, ctx, .. } = error {
        error!("An error occured during unlinking account: {error}");

        _ = ctx
            .send(
                CreateReply::default()
                    .embed(
                        CreateEmbed::new()
                            .description("An error occured whilst trying to unlink the account.")
                            .color(Colors::Error),
                    )
                    .ephemeral(true),
            )
            .await;
    } else {
        error!("{error}")
    }
}
