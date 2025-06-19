use anyhow::Result;
use poise::CreateReply;
use serenity::all::{CreateEmbed, CreateEmbedFooter};
use spoticord_utils::discord::Colors;

use crate::bot::Context;

#[poise::command(slash_command)]
pub async fn rename(
    ctx: Context<'_>,

    #[description = "The new device name"]
    #[max_length = 32]
    #[min_length = 1]
    _name: String,
) -> Result<()> {
    // With centralized Spotify account, device naming is managed at the bot level
    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("Device Naming")
                    .description(
                        "Device naming is now managed centrally by the bot administrator.\nThe bot appears as a single device on the linked Spotify account."
                    )
                    .footer(CreateEmbedFooter::new(
                        "Individual device naming is no longer supported in this version."
                    ))
                    .color(Colors::Info),
            )
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
