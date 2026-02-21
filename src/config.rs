use crate::error::{PfpError, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Config {
    pub api_url: String,
    pub auth_header: String,
}

#[derive(Deserialize, Debug)]
struct ProfilesFile {
    active: Option<String>,
    profiles: Option<std::collections::HashMap<String, Profile>>,
}

#[derive(Deserialize, Debug)]
struct Profile {
    #[serde(rename = "PREFECT_API_URL")]
    api_url: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let api_url = Self::resolve_api_url()?;
        let auth_header = Self::resolve_auth()?;
        Ok(Config {
            api_url,
            auth_header,
        })
    }

    fn resolve_api_url() -> Result<String> {
        // 1. Environment variable override
        if let Ok(url) = std::env::var("PREFECT_API_URL") {
            return Ok(url);
        }

        // 2. Read from profiles.toml
        let profiles = Self::read_profiles()?;
        let active = profiles.active.unwrap_or_else(|| "default".to_string());
        let profile = profiles
            .profiles
            .and_then(|p| p.into_iter().find(|(k, _)| *k == active).map(|(_, v)| v))
            .ok_or_else(|| PfpError::Config(format!("Profile '{}' not found", active)))?;

        profile.api_url.ok_or(PfpError::NoApiUrl)
    }

    fn resolve_auth() -> Result<String> {
        let auth_string = std::env::var("PREFECT_API_AUTH_STRING").map_err(|_| PfpError::NoAuth)?;

        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(auth_string.as_bytes());
        Ok(format!("Basic {}", encoded))
    }

    fn profiles_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".prefect")
            .join("profiles.toml")
    }

    fn read_profiles() -> Result<ProfilesFile> {
        let path = Self::profiles_path();
        let content = std::fs::read_to_string(&path)
            .map_err(|e| PfpError::Config(format!("Cannot read {}: {}", path.display(), e)))?;
        toml::from_str(&content)
            .map_err(|e| PfpError::Config(format!("Cannot parse {}: {}", path.display(), e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn resolve_api_url_from_env() {
        unsafe {
            std::env::set_var("PREFECT_API_URL", "https://test.example.com/api");
        }
        let result = Config::resolve_api_url();
        unsafe {
            std::env::remove_var("PREFECT_API_URL");
        }
        assert_eq!(result.unwrap(), "https://test.example.com/api");
    }

    #[test]
    #[serial]
    fn resolve_auth_encodes_basic_auth() {
        unsafe {
            std::env::set_var("PREFECT_API_AUTH_STRING", "user:pass");
        }
        let result = Config::resolve_auth();
        unsafe {
            std::env::remove_var("PREFECT_API_AUTH_STRING");
        }
        let header = result.unwrap();
        assert!(header.starts_with("Basic "));
        assert_eq!(header, "Basic dXNlcjpwYXNz");
    }

    #[test]
    #[serial]
    fn resolve_auth_missing_returns_error() {
        unsafe {
            std::env::remove_var("PREFECT_API_AUTH_STRING");
        }
        let result = Config::resolve_auth();
        assert!(matches!(result, Err(PfpError::NoAuth)));
    }

    #[test]
    fn profiles_path_ends_with_expected() {
        let path = Config::profiles_path();
        assert!(path.ends_with(".prefect/profiles.toml"));
    }
}
