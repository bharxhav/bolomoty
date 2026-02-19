use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum BoloError {
    #[error("cannot access `{}`: {reason}", path.display())]
    InvalidPath { path: PathBuf, reason: String },

    #[error("cannot walk `{}`: {reason}", path.display())]
    Walk { path: PathBuf, reason: String },

    #[error("cannot read `{}`: {reason}", path.display())]
    Read { path: PathBuf, reason: String },

    #[error("cannot parse `{file}`: {reason}")]
    Parse { file: String, reason: String },

    #[error("`{}` already exists (use -f to overwrite)", path.display())]
    Exists { path: PathBuf },

    #[error("cannot serialize: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("cannot write `{}`: {reason}", path.display())]
    Write { path: PathBuf, reason: String },
}
