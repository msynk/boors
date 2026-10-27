//! Library error type. Variants mirror the .NET exceptions the collectors distinguish between.

use crate::raw_cache::{RawEntry, RawRequest};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A source answered with a non-success status after the resilience pipeline gave up (C# `SourceRequestException`).
    #[error("{} {} returned HTTP {} ({}).", .0.source, .0.endpoint, .0.status_code, .0.url)]
    SourceRequest(Box<RawEntry>),

    /// Cache-only policy and nothing cached (C# `NotCachedException`).
    #[error("No cached response for {} {} [{}].", .0.source, .0.endpoint, .0.params_key)]
    NotCached(Box<RawRequest>),

    /// Network-level failure (C# `HttpRequestException`).
    #[error("{0}")]
    Http(String),

    /// The resilience pipeline rejected the call: circuit open or a timeout (C# `ExecutionRejectedException`).
    #[error("{0}")]
    Rejected(String),

    /// A response or file could not be parsed (C# `FormatException` / `JsonException`).
    #[error("{0}")]
    Format(String),

    /// A bad argument (C# `ArgumentException`).
    #[error("{0}")]
    Argument(String),

    /// An operation that is not valid in the current state (C# `InvalidOperationException`).
    #[error("{0}")]
    InvalidOperation(String),

    /// A TSETMC filter expression does not compile.
    #[error("{0}")]
    FilterSyntax(String),

    #[error("{0}")]
    NotFound(String),

    #[error("The operation was cancelled.")]
    Cancelled,

    #[error("{0}")]
    Db(#[from] duckdb::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn format(message: impl Into<String>) -> Self {
        Error::Format(message.into())
    }

    /// Errors a collection step counts as one failure instead of aborting the job (C# `TryAsync`).
    pub fn is_step_failure(&self) -> bool {
        matches!(self, Error::SourceRequest(_) | Error::Http(_) | Error::Rejected(_) | Error::Format(_))
    }

    /// Errors that end a job as "failed" (C# `RunAsync`).
    pub fn is_job_failure(&self) -> bool {
        matches!(self, Error::SourceRequest(_) | Error::NotCached(_) | Error::Http(_) | Error::Rejected(_))
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::Format(value.to_string())
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(value: zip::result::ZipError) -> Self {
        Error::Format(format!("Invalid bundle: {value}"))
    }
}
