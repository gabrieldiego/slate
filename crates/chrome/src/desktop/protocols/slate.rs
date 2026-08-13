/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Slate-owned internal browser pages.

use std::future::Future;
use std::pin::Pin;

use servo::protocol_handler::{
    DoneChannel, FetchContext, NetworkError, ProtocolHandler, Request, Response,
};
use url::Url;

use crate::desktop::protocols::resource::ResourceProtocolHandler;

#[derive(Default)]
pub struct SlateProtocolHandler {}

impl ProtocolHandler for SlateProtocolHandler {
    fn privileged_paths(&self) -> &'static [&'static str] {
        &["home"]
    }

    fn is_fetchable(&self) -> bool {
        true
    }

    fn load(
        &self,
        request: &mut Request,
        done_chan: &mut DoneChannel,
        context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        if is_slate_home_url(url.as_url()) {
            return ResourceProtocolHandler::response_for_path(
                request,
                done_chan,
                context,
                "/slate-home.html",
            );
        }

        Box::pin(std::future::ready(Response::network_error(
            NetworkError::ResourceLoadError("Invalid Slate internal page".to_owned()),
        )))
    }
}

pub(crate) fn is_slate_home_url(url: &Url) -> bool {
    url.scheme() == "slate"
        && (url.host_str() == Some("home") || url.path().trim_start_matches('/') == "home")
}

#[cfg(test)]
mod tests {
    use super::is_slate_home_url;
    use url::Url;

    #[test]
    fn slate_home_url_matches_host_and_path_forms() {
        assert!(is_slate_home_url(&Url::parse("slate://home").unwrap()));
        assert!(is_slate_home_url(&Url::parse("slate:home").unwrap()));
        assert!(!is_slate_home_url(&Url::parse("slate://settings").unwrap()));
        assert!(!is_slate_home_url(&Url::parse("https://home").unwrap()));
    }
}
