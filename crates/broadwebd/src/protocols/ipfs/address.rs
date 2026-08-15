use crate::BroadwebdError;
use url::Url;

pub(super) struct IpfsUrlParts<'a> {
    pub namespace: &'static str,
    pub name: &'a str,
    pub path: &'a str,
    pub query: Option<&'a str>,
}

pub(super) fn ipfs_url_parts(source: &str) -> Result<IpfsUrlParts<'_>, BroadwebdError> {
    let parsed =
        Url::parse(source).map_err(|error| BroadwebdError::InvalidUrl(error.to_string()))?;
    let namespace = match parsed.scheme() {
        "ipfs" => "ipfs",
        "ipns" => "ipns",
        scheme => {
            return Err(BroadwebdError::UnsupportedRequest(format!(
                "unsupported IPFS scheme: {scheme}"
            )));
        }
    };

    let authority = source
        .split_once("://")
        .map(|(_, authority)| authority)
        .ok_or_else(|| BroadwebdError::InvalidUrl(format!("{source} is missing an authority")))?;
    let name_end = authority
        .find(|ch| matches!(ch, '/' | '?' | '#'))
        .unwrap_or(authority.len());
    let name = &authority[..name_end];
    if name.is_empty() {
        return Err(BroadwebdError::InvalidUrl(format!(
            "{source} is missing a content name"
        )));
    }
    if name.contains('@') || name.contains(':') {
        return Err(BroadwebdError::InvalidUrl(format!(
            "{source} must not include userinfo or a port"
        )));
    }

    let remainder = &authority[name_end..];
    let path_end = remainder
        .find(|ch| matches!(ch, '?' | '#'))
        .unwrap_or(remainder.len());
    let path = if remainder.starts_with('/') {
        &remainder[..path_end]
    } else {
        "/"
    };
    let query = remainder.find('?').map(|query_start| {
        let query = &remainder[query_start + 1..];
        query
            .split_once('#')
            .map(|(query, _)| query)
            .unwrap_or(query)
    });

    Ok(IpfsUrlParts {
        namespace,
        name,
        path,
        query,
    })
}
