use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum PfpError {
    #[error("API error: {0}")]
    Api(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("No PREFECT_API_AUTH_STRING set")]
    NoAuth,

    #[error("No PREFECT_API_URL found in profile")]
    NoApiUrl,

    #[error("No deployment matching '{0}'")]
    NoMatch(String),

    #[error("Ambiguous match '{query}', candidates:\n{candidates}")]
    AmbiguousMatch { query: String, candidates: String },

    #[error("Flow run failed: {0}")]
    FlowRunFailed(String),
}

pub type Result<T> = std::result::Result<T, PfpError>;
