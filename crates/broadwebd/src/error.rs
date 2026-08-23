use core::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BroadwebdError {
    Io(String),
    InvalidProfile(String),
    InvalidUrl(String),
    MissingPlugin(String),
    Request(String),
    FrameTooLarge { limit: usize, actual: usize },
    ResponseTooLarge { limit: usize, actual: usize },
    UnsupportedRequest(String),
}

impl fmt::Display for BroadwebdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => formatter.write_str(error),
            Self::InvalidProfile(profile) => write!(formatter, "invalid profile id: {profile}"),
            Self::InvalidUrl(url) => write!(formatter, "invalid URL: {url}"),
            Self::MissingPlugin(plugin) => write!(formatter, "missing broadwebd plugin: {plugin}"),
            Self::Request(error) => formatter.write_str(error),
            Self::FrameTooLarge { limit, actual } => {
                write!(
                    formatter,
                    "service frame too large: {actual} bytes over {limit} byte limit"
                )
            }
            Self::ResponseTooLarge { limit, actual } => {
                write!(
                    formatter,
                    "response too large: {actual} bytes over {limit} byte limit"
                )
            }
            Self::UnsupportedRequest(request) => {
                write!(formatter, "unsupported broadwebd request: {request}")
            }
        }
    }
}

impl std::error::Error for BroadwebdError {}

impl From<std::io::Error> for BroadwebdError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
