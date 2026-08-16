/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Broadweb protocol handlers for the headed Slate shell.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;

use headers::{ContentType, HeaderMapExt, HeaderName, HeaderValue};
use servo::ServoUrl;
use servo::protocol_handler::{
    DoneChannel, FetchContext, HttpStatus, ProtocolHandler, Request, ResourceFetchTiming, Response,
    ResponseBody,
};
use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, FetchDisposition, HttpFetchRequest, HttpFetchResponse,
};

#[derive(Default)]
pub struct BroadwebProtocolHandler {}

impl ProtocolHandler for BroadwebProtocolHandler {
    fn is_fetchable(&self) -> bool {
        true
    }

    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let address = url.to_string();
        let is_navigation = request.is_navigation_request();
        let mut fetch_request = HttpFetchRequest::default_profile(&address);
        if !is_navigation {
            fetch_request = fetch_request.for_subresource();
        }
        let timing = ResourceFetchTiming::new(request.timing_type());

        let response = match fetch_with_default_broadwebd(fetch_request) {
            Ok(fetch_response) => {
                broadweb_fetch_response(url, timing, fetch_response, is_navigation)
            }
            Err(error) => broadweb_error_response(url, timing, &address, error),
        };
        Box::pin(std::future::ready(response))
    }
}

pub(crate) fn fetch_with_default_broadwebd(
    request: HttpFetchRequest,
) -> Result<HttpFetchResponse, BroadwebdError> {
    thread_local! {
        static BROADWEBD: RefCell<Option<BroadwebDaemon>> = const { RefCell::new(None) };
    }

    BROADWEBD.with(|daemon| {
        if daemon.borrow().is_none() {
            *daemon.borrow_mut() = Some(BroadwebDaemon::start_default_session()?);
        }

        let daemon = daemon.borrow();
        daemon
            .as_ref()
            .expect("broadwebd should be initialized")
            .fetch_http(request)
    })
}

fn broadweb_fetch_response(
    request_url: ServoUrl,
    timing: ResourceFetchTiming,
    fetch_response: HttpFetchResponse,
    is_navigation: bool,
) -> Response {
    let response_url = ServoUrl::parse(&fetch_response.final_url).unwrap_or(request_url);
    let mut response = Response::new(response_url, timing);
    response.status = http_status(fetch_response.status_code);
    if let Some(content_type) = fetch_response.content_type.as_deref() {
        insert_content_type(&mut response, content_type);
    }

    let body = match fetch_response.disposition {
        FetchDisposition::ErrorPage { status_code } if is_navigation => {
            response.headers.typed_insert(ContentType::html());
            broadweb_response_error_html(status_code, &fetch_response).into_bytes()
        }
        FetchDisposition::Download { .. } if is_navigation => {
            response.headers.typed_insert(ContentType::html());
            broadweb_download_ready_html(&fetch_response).into_bytes()
        }
        _ => fetch_response.body,
    };
    *response.body.lock() = ResponseBody::Done(body);
    response
}

fn broadweb_error_response(
    url: ServoUrl,
    timing: ResourceFetchTiming,
    address: &str,
    error: BroadwebdError,
) -> Response {
    let mut response = Response::new(url, timing);
    response.status = HttpStatus::new_raw(502, b"Bad Gateway".to_vec());
    response.headers.typed_insert(ContentType::html());
    *response.body.lock() =
        ResponseBody::Done(broadweb_fetch_error_html(address, &error.to_string()).into_bytes());
    response
}

fn http_status(status_code: u16) -> HttpStatus {
    if (100..=599).contains(&status_code) {
        HttpStatus::new_raw(status_code, Vec::new())
    } else {
        HttpStatus::new_error()
    }
}

fn insert_content_type(response: &mut Response, content_type: &str) {
    if let Ok(value) = HeaderValue::from_str(content_type) {
        response
            .headers
            .insert(HeaderName::from_static("content-type"), value);
    }
}

fn broadweb_fetch_error_html(address: &str, error: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Broadweb Fetch Error</title></head>\
         <body><h1>Broadweb Fetch Error</h1>\
         <p><code>{}</code></p><pre>{}</pre></body></html>",
        escape_html_text(address),
        escape_html_text(error)
    )
}

fn broadweb_response_error_html(status_code: u16, response: &HttpFetchResponse) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Broadweb Response Error</title></head>\
         <body><h1>Broadweb Response Error</h1>\
         <p>Status: {}</p><p>Address: <code>{}</code></p><pre>{}</pre></body></html>",
        status_code,
        escape_html_text(&response.final_url),
        escape_html_text(&response.body_text_lossy())
    )
}

pub(crate) fn broadweb_download_ready_html(response: &HttpFetchResponse) -> String {
    let Some(download) = response.download.as_ref() else {
        return format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <title>Download Ready</title></head>\
             <body><h1>Download Ready</h1>\
             <p>Slate received a downloadable response from <code>{}</code>.</p>\
             <p><a href=\"slate://downloads\">Open Downloads</a></p></body></html>",
            escape_html_text(&response.final_url)
        );
    };

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Download Saved</title></head>\
         <body><h1>Download Saved</h1>\
         <p><strong>{}</strong></p>\
         <p><code>{}</code></p>\
         <p>{} bytes</p>\
         <p><a href=\"slate://downloads\">Open Downloads</a></p></body></html>",
        escape_html_text(&download.filename),
        escape_html_text(&download.path.to_string_lossy()),
        download.size_bytes
    )
}

pub(crate) fn escape_html_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{broadweb_download_ready_html, escape_html_text, http_status};
    use slate_broadwebd::{DownloadRecord, HttpFetchResponse};
    use std::path::PathBuf;

    #[test]
    fn broadweb_protocol_escapes_error_html() {
        assert_eq!(
            escape_html_text("ipfs://cid/<script>&\"'"),
            "ipfs://cid/&lt;script&gt;&amp;&quot;&#39;"
        );
    }

    #[test]
    fn broadweb_protocol_maps_http_status_codes() {
        assert_eq!(http_status(404).raw_code(), 404);
        assert_eq!(http_status(200).raw_code(), 200);
    }

    #[test]
    fn broadweb_download_ready_html_escapes_download_metadata() {
        let html = broadweb_download_ready_html(
            &HttpFetchResponse::new(
                "ipfs://cid/file.txt",
                200,
                Some("text/plain".to_string()),
                Vec::new(),
                b"body".to_vec(),
            )
            .with_download(DownloadRecord::new(
                "default",
                "file <final>.txt",
                PathBuf::from("/tmp/file <final>.txt"),
                4,
                Some("text/plain".to_string()),
            )),
        );

        assert!(html.contains("Download Saved"));
        assert!(html.contains("file &lt;final&gt;.txt"));
        assert!(html.contains("/tmp/file &lt;final&gt;.txt"));
        assert!(html.contains("slate://downloads"));
    }
}
