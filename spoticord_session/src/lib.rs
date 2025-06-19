pub mod error;
pub mod lyrics_embed;
pub mod manager;
pub mod playback_embed;

use error::Error;
use error::Result;
use librespot::{
    core::connection,
    discovery::Credentials,
    protocol::keyexchange::ErrorCode,
};
use log::{debug, error, trace};
use lyrics_embed::LyricsEmbed;
use manager::{SessionManager, SessionQuery};
use playback_embed::{PlaybackEmbed, PlaybackEmbedHandle};
use serenity::{
    all::{
        ChannelId, CommandInteraction, CreateEmbed, CreateMessage, GuildChannel, GuildId, UserId,
    },
    async_trait,
};
use songbird::{model::payload::ClientDisconnect, Call, CoreEvent, Event, EventContext};
use spoticord_player::{Player, PlayerEvent, PlayerHandle};
use spoticord_utils::discord::Colors;
use std::{ops::ControlFlow, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};

#[derive(Debug)]
pub enum SessionCommand {
    GetOwner(oneshot::Sender<UserId>),
    GetPlayer(oneshot::Sender<PlayerHandle>),
    GetActive(oneshot::Sender<bool>),

    CreatePlaybackEmbed(
        SessionHandle,
        CommandInteraction,
        playback_embed::UpdateBehavior,
    ),
    CreateLyricsEmbed(SessionHandle, CommandInteraction),

    Reactivate(UserId, oneshot::Sender<Result<()>>),
    ShutdownPlayer,
    Disconnect,
    DisconnectTimedOut,
}

pub struct Session {
    session_manager: SessionManager,
    context: serenity::all::Context,

    guild_id: GuildId,
    text_channel: GuildChannel,
    call: Arc<Mutex<Call>>,
    player: PlayerHandle,

    owner: UserId,
    active: bool,

    timeout_tx: Option<oneshot::Sender<()>>,

    commands: mpsc::Receiver<SessionCommand>,
    events: mpsc::Receiver<PlayerEvent>,

    commands_inner_tx: mpsc::Sender<SessionCommand>,
    commands_inner_rx: mpsc::Receiver<SessionCommand>,

    playback_embed: Option<PlaybackEmbedHandle>,
    lyrics_embed: Option<JoinHandle<()>>,
}

impl Session {
    pub async fn create(
        session_manager: SessionManager,

        context: &serenity::all::Context,
        guild_id: GuildId,
        voice_channel_id: ChannelId,
        text_channel_id: ChannelId,
        owner: UserId,
    ) -> Result<SessionHandle> {
        use Error::*;

        // Set up communication channel
        let (tx, rx) = mpsc::channel(16);
        let handle = SessionHandle {
            guild: guild_id,
            voice_channel: voice_channel_id,
            text_channel: text_channel_id,

            commands: tx,
        };

        // Resolve text channel
        let text_channel = text_channel_id
            .to_channel(&context)
            .await?
            .guild()
            .ok_or(InvalidChannel)?;

        // Create channel for internal command communication (timeouts hint hint)
        // This uses separate channels as to not cause a cyclic dependency
        let (inner_tx, inner_rx) = mpsc::channel(16);

        // Grab user credentials and info before joining call        // With centralized Spotify account, we no longer need per-user accounts
        // Just get the centralized token from storage
        let access_token = session_manager
            .storage()
            .get_spotify_token()
            .await?
            .ok_or_else(|| Error::Other("No Spotify account linked to bot".into()))?;

        // Use a default device name for the bot
        let device_name = "Spoticord Bot".to_string();        let credentials = Credentials::with_access_token(access_token);

        // Hello Discord I'm here
        let call = session_manager
            .songbird()
            .join(guild_id, voice_channel_id)
            .await?;

        // Make sure call guard is dropped or else we can't execute session.run
        {
            let mut call = call.lock().await;

            // Wasn't able to confirm if this is true, but this might reduce network bandwith by not receiving user voice packets
            _ = call.deafen(true).await;

            // Set up call events
            call.add_global_event(Event::Core(CoreEvent::DriverDisconnect), handle.clone());
            call.add_global_event(Event::Core(CoreEvent::ClientDisconnect), handle.clone());
        }

        let (player, events, _auth_data) =
            match Player::create(credentials, call.clone(), device_name).await {
                Ok(player) => player,
                Err(why) => {
                    // Leave call on error, otherwise bot will be stuck in call forever until manually disconnected or taken over
                    _ = call.lock().await.leave().await;

                    error!("Failed to create player: {why}");

                    if let Some(connection::AuthenticationError::LoginFailed(
                        ErrorCode::BadCredentials,
                    )) = why.error.downcast_ref::<connection::AuthenticationError>()
                    {
                        // Authentication failed with centralized credentials
                        error!("Spotify authentication failed - bot credentials may be invalid");
                        return Err(AuthenticationFailed);
                    }                    return Err(why.into());
                }
            };

        // No need to store credentials since they're centralized

        let mut session = Self {
            session_manager,

            context: context.to_owned(),
            text_channel,

            call,
            player,

            guild_id,
            owner,

            active: true,
            timeout_tx: None,

            commands: rx,
            events,

            commands_inner_tx: inner_tx,
            commands_inner_rx: inner_rx,

            playback_embed: None,
            lyrics_embed: None,
        };
        session.start_timeout();

        tokio::spawn(session.run());

        Ok(handle)
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                opt_command = self.commands.recv() => {
                    trace!("Received command: {opt_command:#?}");

                    let Some(command) = opt_command else {
                        break;
                    };

                    if self.handle_command(command).await.is_break() {
                        break;
                    }
                },

                opt_event = self.events.recv(), if self.active => {
                    trace!("Received event: {opt_event:#?}");

                    let Some(event) = opt_event else {
                        self.shutdown_player().await;
                        continue;
                    };

                    self.handle_event(event).await;
                },

                // Internal communication channel
                Some(command) = self.commands_inner_rx.recv() => {
                    trace!("Received internal command: {command:#?}");

                    if self.handle_command(command).await.is_break() {
                        break;
                    }
                }

                else => break,
            }
        }

        trace!("End of Session::run");
    }

    async fn handle_command(&mut self, command: SessionCommand) -> ControlFlow<(), ()> {
        trace!("SessionCommand::{command:?}");

        match command {
            SessionCommand::GetOwner(sender) => _ = sender.send(self.owner),
            SessionCommand::GetPlayer(sender) => _ = sender.send(self.player.clone()),
            SessionCommand::GetActive(sender) => _ = sender.send(self.active),

            SessionCommand::CreatePlaybackEmbed(handle, interaction, behavior) => {
                match PlaybackEmbed::create(self, handle, interaction, behavior).await {
                    Ok(opt_handle) => {
                        self.playback_embed = opt_handle;
                    }
                    Err(why) => {
                        error!("Failed to create playing embed: {why}");
                    }
                };
            }
            SessionCommand::CreateLyricsEmbed(handle, interaction) => {
                match LyricsEmbed::create(self, handle, interaction).await {
                    Ok(Some(lyrics_embed)) => {
                        if let Some(current) = self.lyrics_embed.take() {
                            current.abort();
                        }

                        self.lyrics_embed = Some(lyrics_embed);
                    }
                    Ok(None) => {}
                    Err(why) => {
                        error!("Failed to create lyrics embed: {why}");
                    }
                }
            }

            SessionCommand::Reactivate(new_owner, tx) => {
                _ = tx.send(self.reactivate(new_owner).await)
            }
            SessionCommand::ShutdownPlayer => self.shutdown_player().await,
            SessionCommand::Disconnect => {
                self.disconnect().await;

                return ControlFlow::Break(());
            }
            SessionCommand::DisconnectTimedOut => {
                self.disconnect().await;

                _ = self
                    .text_channel
                    .send_message(
                        &self.context,
                        CreateMessage::new().embed(
                            CreateEmbed::new()
                                .title("It's a little quiet in here")
                                .description("The bot has been inactive for too long, and has been disconnected.")
                                .color(Colors::Warning),
                        ),
                    )
                    .await;

                return ControlFlow::Break(());
            }
        };

        ControlFlow::Continue(())
    }

    async fn handle_event(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::Play => self.stop_timeout(),
            PlayerEvent::Pause => self.start_timeout(),
            PlayerEvent::Stopped => self.shutdown_player().await,
            PlayerEvent::TrackChanged(_) => {}
            PlayerEvent::ConnectionReset => {
                self.disconnect().await;

                _ = self
                    .text_channel
                    .send_message(
                        &self.context,
                        CreateMessage::new().embed(
                            CreateEmbed::new()
                                .title("Spotify connection lost")
                                .description("The bot has lost connection to the Spotify AP servers.\nThis is most likely caused by a connection reset on Spotify's end.\n\nUse `/join` to resummon the bot to your voice channel.")
                                .color(Colors::Error),
                        ),
                    )
                    .await;
            }
        }

        let force_edit = !matches!(event, PlayerEvent::TrackChanged(_));

        if let Some(playback_embed) = &self.playback_embed {
            if playback_embed.invoke_update(force_edit).await.is_err() {
                self.playback_embed = None;
            }
        }
    }

    fn start_timeout(&mut self) {
        if let Some(tx) = self.timeout_tx.take() {
            _ = tx.send(());
        }

        let (tx, rx) = oneshot::channel::<()>();
        self.timeout_tx = Some(tx);

        let inner_tx = self.commands_inner_tx.clone();

        tokio::spawn(async move {
            let mut timer =
                tokio::time::interval(Duration::from_secs(spoticord_config::DISCONNECT_TIME));

            // Ignore immediate tick
            timer.tick().await;

            tokio::select! {
                _ = rx => return,
                _ = timer.tick() => {}
            };

            // Disconnect through inner communication
            _ = inner_tx.send(SessionCommand::DisconnectTimedOut).await;
        });
    }

    fn stop_timeout(&mut self) {
        if let Some(tx) = self.timeout_tx.take() {
            _ = tx.send(());
        }
    }    async fn reactivate(&mut self, new_owner: UserId) -> Result<()> {
        use Error::*;

        if self.active {
            return Err(AlreadyActive);
        }

        // With centralized Spotify account, we no longer need per-user accounts
        // Just get the centralized token from storage
        let access_token = self
            .session_manager
            .storage()
            .get_spotify_token()
            .await?
            .ok_or_else(|| Error::Other("No Spotify account linked to bot".into()))?;

        // Use a default device name for the bot
        let device_name = "Spoticord Bot".to_string();

        let credentials = Credentials::with_access_token(access_token);        let (player, player_events, _auth_data) =
            match Player::create(credentials, self.call.clone(), device_name).await {
                Ok(player) => player,
                Err(why) => {
                    if let Some(connection::AuthenticationError::LoginFailed(
                        ErrorCode::BadCredentials,
                    )) = why.error.downcast_ref::<connection::AuthenticationError>()
                    {
                        // Authentication failed with centralized credentials
                        // Log the error but don't clear anything since it's not user-specific
                        error!("Spotify authentication failed - bot credentials may be invalid");
                    }

                    return Err(why.into());
                }
            };

        // No need to store credentials since they're centralized

        self.owner = new_owner;
        self.player = player;
        self.events = player_events;
        self.active = true;

        Ok(())
    }

    async fn shutdown_player(&mut self) {
        self.player.shutdown().await;
        self.start_timeout();

        self.active = false;

        // Remove owner from session manager
        self.session_manager
            .remove_session(SessionQuery::Owner(self.owner));
    }

    async fn disconnect(&mut self) {
        // Kill timeout if one is running
        self.stop_timeout();

        // Force close channels, as handles may otherwise hold this struct hostage
        self.commands.close();
        self.events.close();

        // Leave call, ignore errors
        let mut call = self.call.lock().await;
        _ = call.leave().await;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Abort timeout task
        if let Some(tx) = self.timeout_tx.take() {
            _ = tx.send(());
        }

        // Abort lyrics task
        if let Some(lyrics) = self.lyrics_embed.take() {
            lyrics.abort();
        }

        // Clean up the session from the session manager
        // This is done in Drop::drop to ensure that the session always cleans up after itself
        //  even if something went wrong

        let session_manager = self.session_manager.clone();
        let guild_id = self.guild_id;
        let owner = self.owner;

        session_manager.remove_session(SessionQuery::Guild(guild_id));
        session_manager.remove_session(SessionQuery::Owner(owner));
    }
}

#[derive(Clone, Debug)]
pub struct SessionHandle {
    guild: GuildId,
    voice_channel: ChannelId,
    text_channel: ChannelId,

    commands: mpsc::Sender<SessionCommand>,
}

impl SessionHandle {
    /// Check if the session handle is valid
    pub fn is_valid(&self) -> bool {
        !self.commands.is_closed()
    }

    pub fn guild(&self) -> GuildId {
        self.guild
    }

    pub fn voice_channel(&self) -> ChannelId {
        self.voice_channel
    }

    pub fn text_channel(&self) -> ChannelId {
        self.text_channel
    }

    /// Retrieve the current owner of the session
    pub async fn owner(&self) -> anyhow::Result<UserId> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(SessionCommand::GetOwner(tx)).await?;

        let result = rx.await?;
        Ok(result)
    }

    /// Retrieve the player handle from the session
    pub async fn player(&self) -> anyhow::Result<PlayerHandle> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(SessionCommand::GetPlayer(tx)).await?;

        let result = rx.await?;
        Ok(result)
    }

    pub async fn active(&self) -> anyhow::Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(SessionCommand::GetActive(tx)).await?;

        let result = rx.await?;
        Ok(result)
    }

    /// Instruct the session to make another user owner.
    ///
    /// This will fail if the session still has an active user assigned to it.
    pub async fn reactivate(&self, new_owner: UserId) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(SessionCommand::Reactivate(new_owner, tx))
            .await?;

        rx.await??;

        Ok(())
    }

    /// Create a playback embed as a response to an interaction
    ///
    /// This playback embed will automatically update when certain events happen
    pub async fn create_playback_embed(
        &self,
        interaction: &CommandInteraction,
        behavior: playback_embed::UpdateBehavior,
    ) -> anyhow::Result<()> {
        self.commands
            .send(SessionCommand::CreatePlaybackEmbed(
                self.clone(),
                interaction.to_owned(),
                behavior,
            ))
            .await?;

        Ok(())
    }

    /// Create a lyrics embed as a response to an interaction
    ///
    /// This lyrics embed will automatically retrieve the lyrics and update the embed accordingly
    pub async fn create_lyrics_embed(&self, interaction: CommandInteraction) -> anyhow::Result<()> {
        self.commands
            .send(SessionCommand::CreateLyricsEmbed(self.clone(), interaction))
            .await?;

        Ok(())
    }

    /// Instruct the session to destroy the player (but keep voice call).
    ///
    /// This is meant to be used for when the session owner leaves the call
    /// and allows other users to become owner using the `/join` command.
    ///
    /// This should also remove the owner from the session manager.
    pub async fn shutdown_player(&self) {
        if let Err(why) = self.commands.send(SessionCommand::ShutdownPlayer).await {
            error!("Failed to send command: {why}");
        }
    }

    /// Instruct the session to destroy itself.
    ///
    /// This should also remove the player and the owner from the session manager.
    pub async fn disconnect(&self) {
        if let Err(why) = self.commands.send(SessionCommand::Disconnect).await {
            error!("Failed to send command: {why}");
        }
    }
}

#[async_trait]
impl songbird::EventHandler for SessionHandle {
    async fn act(&self, event: &EventContext<'_>) -> Option<Event> {
        if !self.is_valid() {
            return Some(Event::Cancel);
        }

        match event {
            // NOTE: Discord can randomly make the driver disconnect when users join/leave the voice channel
            // Nothing we can do about it at this time since that is an issue with either Discord or Songbird
            EventContext::DriverDisconnect(_) => {
                debug!("Bot disconnected from voice gateway, cleaning up");

                self.disconnect().await;
            }

            EventContext::ClientDisconnect(ClientDisconnect { user_id }) => {
                // Ignore disconnects if we're inactive
                if !self.active().await.unwrap_or(false) {
                    return None;
                }

                match self.owner().await {
                    Ok(id) if id.get() == user_id.0 => {
                        debug!("Owner of session disconnected, stopping playback");

                        self.shutdown_player().await;
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        None
    }
}
