/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Slate-owned internal browser pages.

use std::future::Future;
use std::pin::Pin;

use servo::protocol_handler::{
    DoneChannel, FetchContext, NetworkError, ProtocolHandler, Request, Response,
};

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
        let is_home =
            url.host_str() == Some("home") || url.path().trim_start_matches('/') == "home";

        if is_home {
            return ResourceProtocolHandler::response_for_path(
                request,
                done_chan,
                context,
                "/newtab.html",
            );
        }

        Box::pin(std::future::ready(Response::network_error(
            NetworkError::ResourceLoadError("Invalid Slate internal page".to_owned()),
        )))
    }
}
