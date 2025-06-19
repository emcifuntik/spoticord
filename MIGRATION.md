# Spoticord Centralization Migration

This document summarizes the changes made to convert Spoticord from a multi-user PostgreSQL/Redis-based system to a centralized single-Spotify-account system with simple file storage.

## Major Changes

### 1. Removed Dependencies
- **PostgreSQL**: Removed `spoticord_database` module and all database-related operations
- **Redis**: Simplified `spoticord_stats` to use in-memory logging instead of Redis storage
- **Diesel**: No longer needed as we removed database operations

### 2. Added New Modules

#### `spoticord_storage`
- Simple file-based storage for Spotify credentials
- Handles token refresh automatically
- Stores credentials in JSON format in a configurable data directory

#### `spoticord_web`
- Built with Axum web framework
- Handles Spotify OAuth flow
- Provides web interface for linking the bot's Spotify account
- Saves credentials to the storage system

### 3. Configuration Changes

#### Environment Variables (Updated)
- `BASE_URL`: Base URL for the web server (replaces `LINK_URL`)
- `WEB_PORT`: Port for the web server (default: 8080)
- `DATA_DIR`: Directory for storing credentials (default: `./data`)
- **Removed**: `DATABASE_URL`, `KV_URL`

### 4. Architecture Changes

#### From Multi-User to Single-Account
- **Before**: Each Discord user could link their own Spotify account
- **After**: One centralized Spotify account serves all users
- **Benefits**: Simpler setup, no database required, works for single Discord server use

#### Session Management
- Sessions no longer tied to individual user accounts
- All sessions use the same centralized Spotify credentials
- Device naming simplified to "Spoticord Bot"

#### Command Changes
- `/link`: Now links the bot's central account (admin operation)
- `/unlink`: Unlinks the bot's account and stops all sessions
- `/rename`: Simplified to show information about centralized device naming

### 5. Web Interface

The bot now includes a web server that provides:
- Landing page explaining the setup process
- OAuth flow with Spotify
- Success/error pages with user-friendly feedback
- Automatic credential storage

### 6. File Structure Changes

```
spoticord/
├── spoticord_storage/          # New: File-based credential storage
├── spoticord_web/              # New: Web server for OAuth
├── spoticord_stats/            # Modified: Simplified without Redis
├── spoticord_session/          # Modified: Uses centralized credentials
├── spoticord_config/           # Modified: Updated environment variables
├── spoticord_database/         # Removed from workspace
└── data/                       # New: Default storage directory
    └── spotify_credentials.json
```

## Setup Instructions

### 1. Environment Configuration
Create a `.env` file with:
```bash
DISCORD_TOKEN=your_discord_bot_token
SPOTIFY_CLIENT_ID=your_spotify_client_id
SPOTIFY_CLIENT_SECRET=your_spotify_client_secret
BASE_URL=http://localhost:8080
WEB_PORT=8080
DATA_DIR=./data
```

### 2. Spotify Application Setup
1. Create a Spotify application at https://developer.spotify.com/
2. Add `{BASE_URL}/callback` as a redirect URI
3. Use the Client ID and Secret in your environment variables

### 3. Running the Bot
1. Start the bot: `cargo run`
2. Visit the web interface at your `BASE_URL`
3. Click "Connect Spotify Account" to link the bot's account
4. Use `/join` in Discord to start playing music

## Benefits of the New Architecture

1. **Simplified Setup**: No PostgreSQL or Redis required
2. **Single Point of Management**: One Spotify account for all users
3. **Lightweight**: File-based storage instead of full database
4. **Self-Contained**: Web server built into the bot
5. **Suitable for Single Server**: Perfect for bots serving one Discord server

## Migration Notes

- **Data Loss**: Existing user accounts and preferences will not be migrated
- **Permissions**: The linked Spotify account will be used for all music playback
- **Device Limit**: Spotify's device limit applies to the single account
- **Usage Pattern**: Best suited for single Discord server deployments

## File Storage Format

Credentials are stored in `{DATA_DIR}/spotify_credentials.json`:
```json
{
  "access_token": "...",
  "refresh_token": "...",
  "expires_at": "2025-06-19T12:00:00Z"
}
```

The storage system automatically refreshes tokens when they expire and updates the file accordingly.
