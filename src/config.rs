use crate::error::{PfpError, Result};
use base64::Engine;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Config {
    pub api_url: String,
    pub auth_header: Option<String>,
}

#[derive(Deserialize)]
struct ProfilesFile {
    active: Option<String>,
    profiles: Option<std::collections::HashMap<String, Profile>>,
}

#[derive(Deserialize)]
struct Profile {
    #[serde(rename = "PREFECT_API_URL")]
    api_url: Option<String>,
    #[serde(rename = "PREFECT_API_AUTH_STRING")]
    auth_string: Option<String>,
}

impl Config {
    pub fn load(server: Option<&str>) -> Result<Self> {
        // An explicit server is an atomic profile selection: URL and auth must
        // come from the same profile. A process-wide credential must never be
        // silently paired with a different server's URL.
        if let Some(server) = server {
            let profiles = Self::read_profiles()?;
            return Self::from_profile(&profiles, server);
        }

        // Preserve the established environment-first behavior when no server
        // is selected. The two environment values remain one explicit pair.
        if let Ok(url) = std::env::var("PREFECT_API_URL") {
            return Ok(Self {
                api_url: url,
                auth_header: Self::encode_auth(
                    std::env::var("PREFECT_API_AUTH_STRING").ok().as_deref(),
                ),
            });
        }

        // An active profile was a safe implicit default while pfp managed one
        // server. Once profiles contain distinct API URLs, silently choosing
        // the active one can make a valid deployment on another server look
        // stale or absent, and can send a mutation to the wrong cluster.
        let profiles = Self::read_profiles()?;
        Self::require_unambiguous_implicit_server(&profiles)?;

        // A single configured server may still have profile aliases. Preserve
        // the active profile so its URL and authentication remain one pair.
        // Retain the historical auth environment override for this implicit,
        // single-server selection.
        let active = profiles.active.as_deref().unwrap_or("default");
        let mut config = Self::from_profile(&profiles, active)?;
        if let Ok(auth_string) = std::env::var("PREFECT_API_AUTH_STRING") {
            config.auth_header = Self::encode_auth(Some(&auth_string));
        }
        Ok(config)
    }

    fn from_profile(profiles: &ProfilesFile, profile_name: &str) -> Result<Self> {
        let profile = profiles
            .profiles
            .as_ref()
            .and_then(|profiles| profiles.get(profile_name))
            .ok_or_else(|| PfpError::Config(format!("Profile '{}' not found", profile_name)))?;

        Ok(Self {
            api_url: profile.api_url.clone().ok_or(PfpError::NoApiUrl)?,
            auth_header: Self::encode_auth(profile.auth_string.as_deref()),
        })
    }

    fn require_unambiguous_implicit_server(profiles: &ProfilesFile) -> Result<()> {
        let profile_map = profiles.profiles.as_ref();
        let distinct_urls: HashSet<&str> = profile_map
            .into_iter()
            .flat_map(|profiles| profiles.values())
            .filter_map(|profile| profile.api_url.as_deref())
            .filter(|url| !url.trim().is_empty())
            .collect();

        if distinct_urls.len() <= 1 {
            return Ok(());
        }

        let mut profile_names: Vec<&str> = profile_map
            .into_iter()
            .flat_map(|profiles| profiles.iter())
            .filter_map(|(name, profile)| {
                profile
                    .api_url
                    .as_deref()
                    .filter(|url| !url.trim().is_empty())
                    .map(|_| name.as_str())
            })
            .collect();
        profile_names.sort_unstable();

        Err(PfpError::Config(format!(
            "Multiple Prefect servers are configured in profiles [{}]; select one with --server <PROFILE> or set PREFECT_API_URL explicitly",
            profile_names.join(", ")
        )))
    }

    fn encode_auth(auth_string: Option<&str>) -> Option<String> {
        let auth_string = auth_string?;
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
    fn environment_pair_is_used_without_profile_selection() {
        unsafe {
            std::env::set_var("PREFECT_API_URL", "https://test.example.com/api");
            std::env::set_var("PREFECT_API_AUTH_STRING", "environment:secret");
        }
        let result = Config::load(None).unwrap();
        unsafe {
            std::env::remove_var("PREFECT_API_URL");
            std::env::remove_var("PREFECT_API_AUTH_STRING");
        }
        assert_eq!(result.api_url, "https://test.example.com/api");
        assert_eq!(
            result.auth_header.as_deref(),
            Some("Basic ZW52aXJvbm1lbnQ6c2VjcmV0")
        );
    }

    #[test]
    fn encode_auth_encodes_basic_auth() {
        let header = Config::encode_auth(Some("user:pass")).unwrap();
        assert!(header.starts_with("Basic "));
        assert_eq!(header, "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn encode_auth_missing_returns_none() {
        assert!(Config::encode_auth(None).is_none());
    }

    #[test]
    fn profile_resolves_url_and_auth_as_one_pair() {
        let profiles: ProfilesFile = toml::from_str(
            r#"active = "alpha"

[profiles.alpha]
PREFECT_API_URL = "https://alpha.example/api"
PREFECT_API_AUTH_STRING = "alpha:secret"

[profiles.beta]
PREFECT_API_URL = "https://beta.example/api"
PREFECT_API_AUTH_STRING = "beta:secret"
"#,
        )
        .unwrap();

        let config = Config::from_profile(&profiles, "beta").unwrap();
        assert_eq!(config.api_url, "https://beta.example/api");
        assert_eq!(
            config.auth_header.as_deref(),
            Some("Basic YmV0YTpzZWNyZXQ=")
        );
    }

    #[test]
    fn implicit_selection_rejects_multiple_distinct_server_urls() {
        let profiles: ProfilesFile = toml::from_str(
            r#"active = "alpha"

[profiles.alpha]
PREFECT_API_URL = "https://alpha.example/api"
PREFECT_API_AUTH_STRING = "alpha:secret"

[profiles.beta]
PREFECT_API_URL = "https://beta.example/api"
PREFECT_API_AUTH_STRING = "beta:secret"
"#,
        )
        .unwrap();

        let error = Config::require_unambiguous_implicit_server(&profiles).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("Multiple Prefect servers"));
        assert!(message.contains("[alpha, beta]"));
        assert!(message.contains("--server <PROFILE>"));
        assert!(!message.contains("secret"));
        assert!(!message.contains("https://"));
    }

    #[test]
    fn profile_aliases_for_one_server_remain_unambiguous() {
        let profiles: ProfilesFile = toml::from_str(
            r#"active = "operator"

[profiles.operator]
PREFECT_API_URL = "https://prefect.example/api"
PREFECT_API_AUTH_STRING = "operator:secret"

[profiles.automation]
PREFECT_API_URL = "https://prefect.example/api"
PREFECT_API_AUTH_STRING = "automation:secret"
"#,
        )
        .unwrap();

        Config::require_unambiguous_implicit_server(&profiles).unwrap();
    }

    #[test]
    fn missing_and_empty_urls_do_not_create_phantom_servers() {
        let profiles: ProfilesFile = toml::from_str(
            r#"active = "configured"

[profiles.configured]
PREFECT_API_URL = "https://prefect.example/api"

[profiles.missing]

[profiles.empty]
PREFECT_API_URL = ""
"#,
        )
        .unwrap();

        Config::require_unambiguous_implicit_server(&profiles).unwrap();
    }

    #[test]
    fn profiles_path_ends_with_expected() {
        let path = Config::profiles_path();
        assert!(path.ends_with(".prefect/profiles.toml"));
    }
}
