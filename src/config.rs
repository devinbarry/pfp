use crate::error::{PfpError, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Config {
    pub api_url: String,
    pub auth_header: Option<String>,
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
    pub fn load(server: Option<&str>) -> Result<Self> {
        let api_url = Self::resolve_api_url(server)?;
        let auth_header = Self::resolve_auth();
        Ok(Config {
            api_url,
            auth_header,
        })
    }

    fn resolve_api_url(server: Option<&str>) -> Result<String> {
        if let Some(server) = server {
            let profiles = Self::read_profiles()?;
            return Self::resolve_profile_api_url(&profiles, server);
        }

        // 1. Environment variable override
        if let Ok(url) = std::env::var("PREFECT_API_URL") {
            return Ok(url);
        }

        // 2. Read from profiles.toml
        let profiles = Self::read_profiles()?;
        let active = profiles.active.as_deref().unwrap_or("default");
        Self::resolve_profile_api_url(&profiles, active)
    }

    fn resolve_profile_api_url(profiles: &ProfilesFile, profile_name: &str) -> Result<String> {
        let profile = profiles
            .profiles
            .as_ref()
            .and_then(|profiles| profiles.get(profile_name))
            .ok_or_else(|| PfpError::Config(format!("Profile '{}' not found", profile_name)))?;

        profile.api_url.clone().ok_or(PfpError::NoApiUrl)
    }

    fn resolve_auth() -> Option<String> {
        let auth_string = std::env::var("PREFECT_API_AUTH_STRING").ok()?;
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(auth_string.as_bytes());
        Some(format!("Basic {}", encoded))
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
        let result = Config::resolve_api_url(None);
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
    fn resolve_auth_missing_returns_none() {
        unsafe {
            std::env::remove_var("PREFECT_API_AUTH_STRING");
        }
        let result = Config::resolve_auth();
        assert!(result.is_none());
    }

    #[test]
    fn profiles_path_ends_with_expected() {
        let path = Config::profiles_path();
        assert!(path.ends_with(".prefect/profiles.toml"));
    }
}
