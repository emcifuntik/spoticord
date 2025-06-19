use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rspotify::{clients::BaseClient, Token};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}

impl SpotifyCredentials {
    pub fn new(access_token: String, refresh_token: String, expires_at: DateTime<Utc>) -> Self {
        Self {
            access_token,
            refresh_token,
            expires_at,
        }
    }

    pub fn is_expired(&self) -> bool {
        // Consider token expired if it expires within the next minute
        Utc::now() + Duration::minutes(1) > self.expires_at
    }

    pub async fn refresh_if_needed(&mut self) -> Result<bool> {
        if !self.is_expired() {
            return Ok(false);
        }

        let spotify = spoticord_config::get_spotify(Token {
            access_token: self.access_token.clone(),
            refresh_token: Some(self.refresh_token.clone()),
            expires_at: Some(self.expires_at),
            ..Default::default()
        });

        let new_token = spotify
            .refetch_token()
            .await
            .context("Failed to refresh Spotify token")?
            .context("Received empty token from Spotify")?;

        self.access_token = new_token.access_token;
        if let Some(refresh_token) = new_token.refresh_token {
            self.refresh_token = refresh_token;
        }
        self.expires_at = new_token
            .expires_at
            .context("Token missing expiration time")?;

        Ok(true)
    }
}

#[derive(Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

impl Storage {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.data_dir)
            .await
            .context("Failed to create data directory")?;
        Ok(())
    }

    pub async fn get_spotify_credentials(&self) -> Result<Option<SpotifyCredentials>> {
        let path = self.data_dir.join("spotify_credentials.json");
        
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)
            .await
            .context("Failed to read credentials file")?;
        
        let credentials: SpotifyCredentials = serde_json::from_str(&content)
            .context("Failed to parse credentials file")?;
        
        Ok(Some(credentials))
    }

    pub async fn save_spotify_credentials(&self, credentials: &SpotifyCredentials) -> Result<()> {
        let path = self.data_dir.join("spotify_credentials.json");
        let content = serde_json::to_string_pretty(credentials)
            .context("Failed to serialize credentials")?;
        
        fs::write(path, content)
            .await
            .context("Failed to write credentials file")?;
        
        Ok(())
    }

    pub async fn get_spotify_token(&self) -> Result<Option<String>> {
        let mut credentials = match self.get_spotify_credentials().await? {
            Some(creds) => creds,
            None => return Ok(None),
        };

        if credentials.refresh_if_needed().await? {
            // Save updated credentials
            self.save_spotify_credentials(&credentials).await?;
        }

        Ok(Some(credentials.access_token))
    }
}
