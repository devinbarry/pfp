use thiserror::Error;

#[derive(Error, Debug)]
pub enum PfpError {
    #[error("API error: {0}")]
    Api(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("No PREFECT_API_URL found in profile")]
    NoApiUrl,

    #[error("No match: {0}")]
    NoMatch(String),

    #[error("Ambiguous match '{query}', candidates:\n{candidates}")]
    AmbiguousMatch { query: String, candidates: String },

    #[error("Flow run failed: {0}")]
    FlowRunFailed(String),
}

impl PfpError {
    /// Exit code: 1 for flow run failures, 2 for CLI/usage errors.
    pub fn exit_code(&self) -> i32 {
        match self {
            PfpError::FlowRunFailed(_) => 1,
            _ => 2,
        }
    }
}

pub type Result<T> = std::result::Result<T, PfpError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_flow_run_failed() {
        let err = PfpError::FlowRunFailed("bad".to_string());
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn exit_code_no_match() {
        let err = PfpError::NoMatch("no deployment matching 'foo'".to_string());
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn exit_code_api_error() {
        let err = PfpError::Api("500".to_string());
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn no_match_displays_message() {
        let err = PfpError::NoMatch("no flow run matching 'abc123'".to_string());
        assert_eq!(
            format!("{}", err),
            "No match: no flow run matching 'abc123'"
        );
    }

    #[test]
    fn ambiguous_match_displays_candidates() {
        let err = PfpError::AmbiguousMatch {
            query: "abc".to_string(),
            candidates: "  abc-123\n  abc-456".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("abc"));
        assert!(msg.contains("abc-123"));
    }
}
