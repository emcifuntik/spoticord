use anyhow::Result;
use poise::CreateReply;

use crate::bot::Context;

/// Retrieve the Spotify access token. For debugging purposes.
#[poise::command(slash_command)]
pub async fn token(ctx: Context<'_>) -> Result<()> {
    let token = ctx
        .data()
        .storage()
        .get_spotify_token()
        .await;

    let content = match token {
        Ok(Some(token)) => format!("Bot's Spotify token:\n```\n{token}\n```"),
        Ok(None) => {
            "The bot doesn't have a Spotify account linked".to_string()
        }
        Err(why) => format!("Failed to retrieve access token: {why}"),
    };

    ctx.send(CreateReply::default().content(content).ephemeral(true))
        .await?;

    Ok(())
}
