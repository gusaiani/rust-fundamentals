#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Schema required this var, but the env didn't have it
    #[error("missing env var `{var}`")]
    Missing { var: String },

    #[error("failed to parse env var `{var}`")]
    Parse {
        var: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// `VarName::parse` rejected the name itself (bad characters, empty, etc.).
    #[error("invalid env var name `{name}`: {reason}")]
    InvalidName { name: String, reason: &'static str },

    /// `Env::get` was called for a name that wasn't in the schema
    #[error("env var `{var}` was not requested in the schema")]
    NotRequested { var: String },
}  