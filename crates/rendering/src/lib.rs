#![forbid(unsafe_code)]

use core::fmt;
use dpi::PhysicalSize;
use headers::{ContentType, HeaderMapExt, HeaderName, HeaderValue};
use servo::protocol_handler::{
    DoneChannel, FetchContext, HttpStatus, ProtocolHandler, ProtocolRegistry, Request,
    ResourceFetchTiming, Response, ResponseBody,
};
use servo::{
    EventLoopWaker, LoadStatus, Preferences, RenderingContext, Servo, ServoBuilder, ServoUrl,
    SoftwareRenderingContext, WebView, WebViewBuilder, WebViewDelegate,
};
use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, DownloadRecord, FetchDisposition, FetchRouteInfo,
    HttpFetchRequest, HttpFetchResponse,
};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};
use url::Url;

pub const VENDORED_SERVO_PATH: &str = "third_party/servo";

const DEFAULT_SERVO_VIEWPORT_WIDTH: u32 = 1080;
const DEFAULT_SERVO_VIEWPORT_HEIGHT: u32 = 620;
const MAX_SERVO_VIEWPORT_SIZE: u32 = 4096;
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(10);
const SPIN_SLEEP: Duration = Duration::from_millis(4);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderSurface {
    pub title: String,
    pub address: String,
    pub summary: String,
    pub metrics: Vec<RenderMetric>,
    pub document: RenderDocument,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderDocument {
    App,
    Home,
    Web(ServoDocument),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServoDocument {
    pub title: String,
    pub address: String,
    pub frame: ServoFrame,
    pub source: ServoDocumentSource,
    pub status: ServoDocumentStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServoFrame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderViewport {
    pub width: u32,
    pub height: u32,
}

impl RenderViewport {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width: viewport_dimension(width),
            height: viewport_dimension(height),
        }
    }
}

impl Default for RenderViewport {
    fn default() -> Self {
        Self {
            width: DEFAULT_SERVO_VIEWPORT_WIDTH,
            height: DEFAULT_SERVO_VIEWPORT_HEIGHT,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoDocumentSource {
    SlateGenerated,
    LocalFile,
    Web,
    Broadweb,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServoDocumentStatus {
    Rendered,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderMetric {
    pub label: String,
    pub value: String,
    pub accent: MetricAccent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricAccent {
    Teal,
    Amber,
    Blue,
}

pub trait RenderBackend {
    fn name(&self) -> &'static str;
    fn load_home(&self) -> RenderSurface;
    fn load_address(&self, address: &str) -> RenderSurface;

    fn load_address_with_viewport(
        &self,
        address: &str,
        _viewport: RenderViewport,
    ) -> RenderSurface {
        self.load_address(address)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServoBackend;

impl ServoBackend {
    pub fn vendored_path(self) -> &'static str {
        VENDORED_SERVO_PATH
    }

    pub fn render_html(
        self,
        address: &str,
        html: impl Into<String>,
        source: ServoDocumentSource,
    ) -> RenderSurface {
        self.render_html_with_viewport(address, html, source, RenderViewport::default())
    }

    pub fn render_html_with_viewport(
        self,
        address: &str,
        html: impl Into<String>,
        source: ServoDocumentSource,
        viewport: RenderViewport,
    ) -> RenderSurface {
        let html = html.into();
        let target = data_html_url(&html);
        self.render_servo_url(address, &target, source, viewport)
    }

    pub fn render_error(
        self,
        address: &str,
        title: &str,
        heading: &str,
        details: &[String],
        source: ServoDocumentSource,
    ) -> RenderSurface {
        self.render_error_with_viewport(
            address,
            title,
            heading,
            details,
            source,
            RenderViewport::default(),
        )
    }

    pub fn render_error_with_viewport(
        self,
        address: &str,
        title: &str,
        heading: &str,
        details: &[String],
        source: ServoDocumentSource,
        viewport: RenderViewport,
    ) -> RenderSurface {
        let escaped_title = escape_html_text(title);
        let escaped_heading = escape_html_text(heading);
        let mut body = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <title>{escaped_title}</title>\
             <style>body{{font-family:sans-serif;margin:48px;color:#262626;background:#fbfaf8}}\
             h1{{font-size:34px;color:#0b6b68}}p{{font-size:17px;line-height:1.5}}</style>\
             </head><body><h1>{escaped_heading}</h1>"
        );
        for detail in details {
            body.push_str("<p>");
            body.push_str(&escape_html_text(detail));
            body.push_str("</p>");
        }
        body.push_str("</body></html>");
        self.render_html_with_viewport(address, body, source, viewport)
    }

    fn render_servo_url(
        self,
        display_address: &str,
        servo_address: &str,
        source: ServoDocumentSource,
        viewport: RenderViewport,
    ) -> RenderSurface {
        let result = render_with_servo(display_address, servo_address, source, viewport);
        match result {
            Ok(document) => surface_from_servo_document(document),
            Err(error) => engine_error_surface(display_address, source, error),
        }
    }
}

impl RenderBackend for ServoBackend {
    fn name(&self) -> &'static str {
        "Servo engine"
    }

    fn load_home(&self) -> RenderSurface {
        home_surface()
    }

    fn load_address(&self, address: &str) -> RenderSurface {
        self.load_address_with_viewport(address, RenderViewport::default())
    }

    fn load_address_with_viewport(&self, address: &str, viewport: RenderViewport) -> RenderSurface {
        match address {
            "slate://home" | "slate://new" => home_surface(),
            address if address.starts_with("slate://tests/") => self.render_html_with_viewport(
                address,
                builtin_test_html(address),
                ServoDocumentSource::SlateGenerated,
                viewport,
            ),
            address if address.starts_with("slate://search?") => self.render_html_with_viewport(
                address,
                search_html(address),
                ServoDocumentSource::SlateGenerated,
                viewport,
            ),
            address if address.starts_with("slate://") => self.render_html_with_viewport(
                address,
                internal_html(address),
                ServoDocumentSource::SlateGenerated,
                viewport,
            ),
            address if requires_private_network_host_adapter(address) => self
                .render_error_with_viewport(
                address,
                "Protocol Adapter Required",
                "This route needs a Slate protocol adapter",
                &[
                    address.to_string(),
                    "Slate will not send this address through normal DNS or direct web routing."
                        .to_string(),
                ],
                ServoDocumentSource::Blocked,
                viewport,
            ),
            address if has_ipfs_service_scheme(address) => self.render_broadwebd_fetch(
                address,
                HttpFetchRequest::default_profile(address),
                ServoDocumentSource::Broadweb,
                viewport,
            ),
            address if has_broadweb_scheme(address) => {
                self.render_servo_url(address, address, ServoDocumentSource::Broadweb, viewport)
            }
            address if has_http_service_scheme(address) => self.render_broadwebd_fetch(
                address,
                HttpFetchRequest::default_profile(address),
                ServoDocumentSource::Web,
                viewport,
            ),
            address if has_servo_supported_scheme(address) => {
                let source = if address.starts_with("file://") {
                    ServoDocumentSource::LocalFile
                } else {
                    ServoDocumentSource::Web
                };
                self.render_servo_url(address, address, source, viewport)
            }
            address => self.render_error_with_viewport(
                address,
                "Unsupported Address",
                "Servo cannot navigate this address yet",
                &[address.to_string()],
                ServoDocumentSource::Blocked,
                viewport,
            ),
        }
    }
}

impl ServoBackend {
    fn render_broadwebd_fetch(
        self,
        address: &str,
        request: HttpFetchRequest,
        source: ServoDocumentSource,
        viewport: RenderViewport,
    ) -> RenderSurface {
        match fetch_with_default_broadwebd(request) {
            Ok(response) => match &response.disposition {
                FetchDisposition::RenderHtml => {
                    let html =
                        broadweb_html_with_document_base(address, &response.body_text_lossy());
                    let mut surface =
                        self.render_html_with_viewport(address, html, source, viewport);
                    append_broadweb_route_metrics(&mut surface, response.route.as_ref());
                    surface
                }
                FetchDisposition::Download { suggested_filename } => {
                    let mut surface = self.render_html_with_viewport(
                        address,
                        download_ready_html(
                            &response.final_url,
                            response.content_type.as_deref(),
                            suggested_filename,
                            response.download.as_ref(),
                        ),
                        source,
                        viewport,
                    );
                    append_broadweb_route_metrics(&mut surface, response.route.as_ref());
                    surface
                }
                FetchDisposition::ErrorPage { status_code } => {
                    let mut surface = self.render_html_with_viewport(
                        address,
                        broadweb_response_error_html(address, *status_code, &response),
                        ServoDocumentSource::Blocked,
                        viewport,
                    );
                    append_broadweb_route_metrics(&mut surface, response.route.as_ref());
                    surface
                }
            },
            Err(error) => self.render_error_with_viewport(
                address,
                "Broadweb Fetch Error",
                "Slate could not fetch this page through broadwebd",
                &[address.to_string(), error.to_string()],
                ServoDocumentSource::Blocked,
                viewport,
            ),
        }
    }
}

fn fetch_with_default_broadwebd(
    request: HttpFetchRequest,
) -> Result<HttpFetchResponse, BroadwebdError> {
    #[cfg(test)]
    if let Some(daemon) = test_broadwebd_override() {
        return daemon.fetch_http(request);
    }

    thread_local! {
        static BROADWEBD: RefCell<Option<BroadwebDaemon>> = RefCell::new(None);
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

#[cfg(test)]
static TEST_BROADWEBD: Mutex<Option<Arc<BroadwebDaemon>>> = Mutex::new(None);

#[cfg(test)]
fn with_test_broadwebd<R>(daemon: BroadwebDaemon, run: impl FnOnce() -> R) -> R {
    let daemon = Arc::new(daemon);
    {
        let mut slot = TEST_BROADWEBD.lock().expect("lock test broadwebd override");
        let previous = slot.replace(daemon);
        assert!(previous.is_none(), "test broadwebd override is already set");
    }

    let result = run();

    {
        let mut slot = TEST_BROADWEBD.lock().expect("lock test broadwebd override");
        let installed = slot.take();
        assert!(installed.is_some(), "test broadwebd override was removed");
    }

    result
}

#[cfg(test)]
fn test_broadwebd_override() -> Option<Arc<BroadwebDaemon>> {
    TEST_BROADWEBD
        .lock()
        .expect("lock test broadwebd override")
        .clone()
}

fn broadweb_fetch_protocol_response(url: ServoUrl, timing: ResourceFetchTiming) -> Response {
    let address = url.to_string();
    match fetch_with_default_broadwebd(
        HttpFetchRequest::default_profile(&address).for_subresource(),
    ) {
        Ok(fetch_response) => broadweb_fetch_response(url, timing, fetch_response),
        Err(error) => broadweb_error_protocol_response(url, timing, error),
    }
}

fn broadweb_fetch_protocol_response_on_worker(
    url: ServoUrl,
    timing: ResourceFetchTiming,
) -> Response {
    std::thread::spawn(move || broadweb_fetch_protocol_response(url, timing))
        .join()
        .expect("broadweb protocol fetch worker panicked")
}

fn broadweb_fetch_response(
    url: ServoUrl,
    timing: ResourceFetchTiming,
    fetch_response: HttpFetchResponse,
) -> Response {
    let mut response = Response::new(url, timing);
    response.status = http_status(fetch_response.status_code);
    if let Some(content_type) = fetch_response.content_type.as_deref() {
        insert_content_type(&mut response, content_type);
    }
    *response.body.lock() = ResponseBody::Done(fetch_response.body);
    response
}

fn broadweb_error_protocol_response(
    url: ServoUrl,
    timing: ResourceFetchTiming,
    error: BroadwebdError,
) -> Response {
    let address = url.to_string();
    let mut response = Response::new(url, timing);
    response.status = HttpStatus::new_raw(502, b"Bad Gateway".to_vec());
    response.headers.typed_insert(ContentType::html());
    *response.body.lock() =
        ResponseBody::Done(broadweb_fetch_error_html(&address, &error.to_string()).into_bytes());
    response
}

fn broadweb_placeholder_protocol_response(url: ServoUrl, timing: ResourceFetchTiming) -> Response {
    let html = broadweb_placeholder_html(&url.to_string());
    let mut response = Response::new(url, timing);
    *response.body.lock() = ResponseBody::Done(html.into_bytes());
    response.headers.typed_insert(ContentType::html());
    response.status = HttpStatus::default();
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

#[derive(Clone)]
struct ServoWaker {
    triggered: Arc<AtomicBool>,
}

impl ServoWaker {
    fn new() -> Self {
        Self {
            triggered: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl EventLoopWaker for ServoWaker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        self.triggered.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct ServoDelegateState {
    page_title: RefCell<Option<String>>,
}

impl WebViewDelegate for ServoDelegateState {
    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        *self.page_title.borrow_mut() = title;
    }

    fn notify_new_frame_ready(&self, webview: WebView) {
        webview.paint();
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct BroadwebProtocolHandler;

impl ProtocolHandler for BroadwebProtocolHandler {
    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let timing = ResourceFetchTiming::new(request.timing_type());
        let response = if has_ipfs_service_scheme(url.as_url().as_str()) {
            broadweb_fetch_protocol_response_on_worker(url, timing)
        } else {
            broadweb_placeholder_protocol_response(url, timing)
        };

        Box::pin(std::future::ready(response))
    }

    fn is_fetchable(&self) -> bool {
        true
    }

    fn is_secure(&self) -> bool {
        true
    }
}

struct ServoRenderer {
    servo: Servo,
}

impl ServoRenderer {
    fn new() -> Self {
        init_servo_crypto();

        let mut preferences = Preferences::default();
        preferences.network_http_proxy_uri.clear();
        preferences.network_https_proxy_uri.clear();

        let waker = ServoWaker::new();
        let servo = ServoBuilder::default()
            .preferences(preferences)
            .protocol_registry(broadweb_protocol_registry())
            .event_loop_waker(Box::new(waker))
            .build();

        Self { servo }
    }

    fn render(
        &self,
        display_address: &str,
        servo_address: &str,
        source: ServoDocumentSource,
        viewport: RenderViewport,
    ) -> Result<ServoDocument, ServoRenderError> {
        let url = Url::parse(servo_address)
            .map_err(|error| ServoRenderError::new(format!("invalid Servo URL: {error}")))?;
        let rendering_context = servo_rendering_context(viewport)?;
        rendering_context.make_current().map_err(|error| {
            ServoRenderError::new(format!("Servo context setup failed: {error:?}"))
        })?;

        let delegate = Rc::new(ServoDelegateState::default());
        let webview = WebViewBuilder::new(&self.servo, rendering_context.clone())
            .url(url)
            .delegate(delegate.clone())
            .build();

        let load_webview = webview.clone();
        let loaded = spin_until(&self.servo, LOAD_TIMEOUT, move || {
            load_webview.load_status() != LoadStatus::Complete
        });
        if !loaded {
            return Err(ServoRenderError::new(format!(
                "Servo load timed out after {} seconds",
                LOAD_TIMEOUT.as_secs()
            )));
        }

        let captured = Rc::new(RefCell::new(None));
        let captured_result = Rc::clone(&captured);
        webview.take_screenshot(None, move |result| {
            *captured_result.borrow_mut() = Some(result.map_err(|error| format!("{error:?}")));
        });

        let screenshot_ready = spin_until(&self.servo, SCREENSHOT_TIMEOUT, {
            let captured = Rc::clone(&captured);
            move || captured.borrow().is_none()
        });
        if !screenshot_ready {
            return Err(ServoRenderError::new(format!(
                "Servo screenshot timed out after {} seconds",
                SCREENSHOT_TIMEOUT.as_secs()
            )));
        }

        let image = captured
            .borrow_mut()
            .take()
            .ok_or_else(|| ServoRenderError::new("Servo did not return a screenshot"))?
            .map_err(ServoRenderError::new)?;
        let frame = frame_from_image(image)?;
        let title = delegate
            .page_title
            .borrow()
            .clone()
            .or_else(|| webview.page_title())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| display_address.to_string());
        let address = if servo_address.starts_with("data:") {
            display_address.to_string()
        } else {
            match source {
                ServoDocumentSource::SlateGenerated | ServoDocumentSource::Blocked => {
                    display_address.to_string()
                }
                ServoDocumentSource::LocalFile
                | ServoDocumentSource::Web
                | ServoDocumentSource::Broadweb => webview
                    .url()
                    .map(|url| url.to_string())
                    .unwrap_or_else(|| display_address.to_string()),
            }
        };

        Ok(ServoDocument {
            title,
            address,
            frame,
            source,
            status: ServoDocumentStatus::Rendered,
        })
    }
}

fn render_with_servo(
    display_address: &str,
    servo_address: &str,
    source: ServoDocumentSource,
    viewport: RenderViewport,
) -> Result<ServoDocument, ServoRenderError> {
    thread_local! {
        static SERVO_RENDERER: ServoRenderer = ServoRenderer::new();
    }

    SERVO_RENDERER
        .with(|renderer| renderer.render(display_address, servo_address, source, viewport))
}

fn servo_rendering_context(
    viewport: RenderViewport,
) -> Result<Rc<dyn RenderingContext>, ServoRenderError> {
    let context = SoftwareRenderingContext::new(PhysicalSize {
        width: viewport.width,
        height: viewport.height,
    })
    .map_err(|error| ServoRenderError::new(format!("software rendering failed: {error:?}")))?;
    Ok(Rc::new(context))
}

fn viewport_dimension(value: usize) -> u32 {
    u32::try_from(value.max(1))
        .unwrap_or(MAX_SERVO_VIEWPORT_SIZE)
        .clamp(1, MAX_SERVO_VIEWPORT_SIZE)
}

fn spin_until(servo: &Servo, timeout: Duration, waiting: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while waiting() {
        if Instant::now() >= deadline {
            return false;
        }
        servo.spin_event_loop();
        std::thread::sleep(SPIN_SLEEP);
    }
    true
}

fn init_servo_crypto() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn broadweb_protocol_registry() -> ProtocolRegistry {
    let mut registry = ProtocolRegistry::with_internal_protocols();
    for scheme in ["ipfs", "ipns", "i2p", "gemini", "magnet"] {
        let _ = registry.register(scheme, BroadwebProtocolHandler);
    }
    registry
}

fn frame_from_image(image: servo::RgbaImage) -> Result<ServoFrame, ServoRenderError> {
    let width = usize::try_from(image.width())
        .map_err(|_| ServoRenderError::new("Servo screenshot width is too large"))?;
    let height = usize::try_from(image.height())
        .map_err(|_| ServoRenderError::new("Servo screenshot height is too large"))?;
    let pixels = image.as_raw().chunks_exact(4).map(rgb_from_rgba).collect();
    Ok(ServoFrame {
        width,
        height,
        pixels,
    })
}

fn rgb_from_rgba(rgba: &[u8]) -> u32 {
    let red = u32::from(rgba[0]);
    let green = u32::from(rgba[1]);
    let blue = u32::from(rgba[2]);
    let alpha = u32::from(rgba[3]);
    let inverse_alpha = 255_u32.saturating_sub(alpha);
    let blended_red = red
        .saturating_mul(alpha)
        .saturating_add(255_u32.saturating_mul(inverse_alpha))
        / 255;
    let blended_green = green
        .saturating_mul(alpha)
        .saturating_add(255_u32.saturating_mul(inverse_alpha))
        / 255;
    let blended_blue = blue
        .saturating_mul(alpha)
        .saturating_add(255_u32.saturating_mul(inverse_alpha))
        / 255;
    (blended_red << 16) | (blended_green << 8) | blended_blue
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServoRenderError {
    message: String,
}

impl ServoRenderError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ServoRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ServoRenderError {}

fn surface_from_servo_document(document: ServoDocument) -> RenderSurface {
    RenderSurface {
        title: document.title.clone(),
        address: document.address.clone(),
        summary: "Rendered by Servo".to_string(),
        metrics: document_metrics(document.source, true),
        document: RenderDocument::Web(document),
    }
}

fn engine_error_surface(
    address: &str,
    source: ServoDocumentSource,
    error: ServoRenderError,
) -> RenderSurface {
    RenderSurface {
        title: "Servo Render Error".to_string(),
        address: address.to_string(),
        summary: error.to_string(),
        metrics: document_metrics(source, false),
        document: RenderDocument::Web(ServoDocument {
            title: "Servo Render Error".to_string(),
            address: address.to_string(),
            frame: ServoFrame {
                width: 0,
                height: 0,
                pixels: Vec::new(),
            },
            source,
            status: ServoDocumentStatus::Failed(error.to_string()),
        }),
    }
}

fn home_surface() -> RenderSurface {
    RenderSurface {
        title: "New Tab".to_string(),
        address: "slate://home".to_string(),
        summary: "Home".to_string(),
        metrics: vec![
            RenderMetric {
                label: "Privacy First".to_string(),
                value: String::new(),
                accent: MetricAccent::Teal,
            },
            RenderMetric {
                label: "Tracker Blocked".to_string(),
                value: "23".to_string(),
                accent: MetricAccent::Amber,
            },
            RenderMetric {
                label: "Ads Blocked".to_string(),
                value: "184".to_string(),
                accent: MetricAccent::Blue,
            },
            RenderMetric {
                label: "Time Saved".to_string(),
                value: "2h 14m".to_string(),
                accent: MetricAccent::Teal,
            },
        ],
        document: RenderDocument::Home,
    }
}

fn document_metrics(source: ServoDocumentSource, rendered: bool) -> Vec<RenderMetric> {
    vec![
        RenderMetric {
            label: "HTML".to_string(),
            value: if rendered { "Servo" } else { "Error" }.to_string(),
            accent: MetricAccent::Teal,
        },
        RenderMetric {
            label: "CSS".to_string(),
            value: if rendered { "Servo" } else { "Off" }.to_string(),
            accent: MetricAccent::Blue,
        },
        RenderMetric {
            label: "JS".to_string(),
            value: if rendered { "Servo" } else { "Off" }.to_string(),
            accent: MetricAccent::Amber,
        },
        RenderMetric {
            label: "Route".to_string(),
            value: route_label(source).to_string(),
            accent: MetricAccent::Teal,
        },
    ]
}

fn append_broadweb_route_metrics(surface: &mut RenderSurface, route: Option<&FetchRouteInfo>) {
    let Some(route) = route else {
        return;
    };

    surface.metrics.push(RenderMetric {
        label: "Profile".to_string(),
        value: route.profile.clone(),
        accent: MetricAccent::Blue,
    });
    surface.metrics.push(RenderMetric {
        label: "Transport".to_string(),
        value: route.transport_id.clone(),
        accent: MetricAccent::Teal,
    });
    surface.metrics.push(RenderMetric {
        label: "Boundary".to_string(),
        value: route_boundary_label(&route.privacy_boundary).to_string(),
        accent: MetricAccent::Amber,
    });
}

fn route_boundary_label(privacy_boundary: &str) -> &'static str {
    if privacy_boundary.contains("public IPFS gateway") {
        "Public IPFS Gateway"
    } else if privacy_boundary.contains("local IPFS gateway") {
        "Local IPFS Gateway"
    } else if privacy_boundary.contains("local Kubo RPC") {
        "Local Kubo RPC"
    } else if privacy_boundary.contains("direct HTTP") {
        "Direct Web"
    } else {
        "Broadweb"
    }
}

fn route_label(source: ServoDocumentSource) -> &'static str {
    match source {
        ServoDocumentSource::SlateGenerated => "Local",
        ServoDocumentSource::LocalFile => "Disk",
        ServoDocumentSource::Web => "Web",
        ServoDocumentSource::Broadweb => "Broadweb",
        ServoDocumentSource::Blocked => "Blocked",
    }
}

fn builtin_test_html(address: &str) -> String {
    let escaped_address = escape_html_text(address);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Slate Servo Test</title>\
         <style>body{{font-family:sans-serif;margin:48px;background:#f4fbf8;color:#24302e}}\
         h1{{color:#0b6b68;font-size:38px}}p{{font-size:18px;line-height:1.5}}</style>\
         <script>document.addEventListener('DOMContentLoaded',()=>{{\
         document.body.dataset.servoScript='ready';\
         const marker=document.createElement('p');\
         marker.textContent='JavaScript executed inside Servo.';\
         document.body.appendChild(marker);\
         }});</script></head>\
         <body><h1>Hello from Servo</h1>\
         <p>This test page is rendered by the vendored Servo engine.</p>\
         <p>{escaped_address}</p></body></html>"
    )
}

fn search_html(address: &str) -> String {
    let query = address
        .split_once("?q=")
        .map(|(_, value)| value.replace('+', " "))
        .unwrap_or_default();
    let escaped_query = escape_html_text(&query);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Search</title>\
         <style>body{{font-family:sans-serif;margin:48px;color:#262626;background:#fbfaf8}}\
         h1{{font-size:34px;color:#0b6b68}}p{{font-size:17px}}</style></head>\
         <body><h1>{escaped_query}</h1><p>Local search surface.</p>\
         <p>No remote search request has been issued.</p></body></html>"
    )
}

fn internal_html(address: &str) -> String {
    let escaped_address = escape_html_text(address);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Slate Internal Page</title>\
         <style>body{{font-family:sans-serif;margin:48px;color:#262626;background:#fbfaf8}}\
         h1{{font-size:34px;color:#0b6b68}}p{{font-size:17px}}</style></head>\
         <body><h1>Slate Internal Page</h1><p>{escaped_address}</p>\
         <p>This address is handled locally, then painted by Servo.</p></body></html>"
    )
}

fn broadweb_placeholder_html(address: &str) -> String {
    let escaped_address = escape_html_text(address);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Broadweb Route Pending</title>\
         <style>body{{font-family:sans-serif;margin:48px;color:#262626;background:#f2fbf7}}\
         h1{{font-size:34px;color:#0b6b68}}p{{font-size:17px;line-height:1.5}}\
         code{{background:#e5f0ee;padding:4px 6px;border-radius:4px}}</style></head>\
         <body><h1>Broadweb route pending</h1>\
         <p><code>{escaped_address}</code></p>\
         <p>Servo received this address through Slate's broadweb protocol callback.</p>\
         <p>No I2P, Gemini, magnet, or other pending adapter network request has been issued yet.</p>\
         </body></html>"
    )
}

fn broadweb_fetch_error_html(address: &str, error: &str) -> String {
    let escaped_address = escape_html_text(address);
    let escaped_error = escape_html_text(error);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Broadweb Fetch Error</title>\
         <style>body{{font-family:sans-serif;margin:48px;color:#262626;background:#fbfaf8}}\
         h1{{font-size:34px;color:#0b6b68}}p{{font-size:17px;line-height:1.5}}\
         code{{background:#e5f0ee;padding:4px 6px;border-radius:4px}}</style></head>\
         <body><h1>Broadweb fetch error</h1>\
         <p><code>{escaped_address}</code></p>\
         <p>{escaped_error}</p>\
         </body></html>"
    )
}

fn broadweb_response_error_html(
    address: &str,
    status_code: u16,
    response: &HttpFetchResponse,
) -> String {
    let escaped_address = escape_html_text(address);
    let escaped_final_url = escape_html_text(&response.final_url);
    let escaped_content_type =
        escape_html_text(response.content_type.as_deref().unwrap_or("unknown"));
    let body_excerpt = response_body_excerpt(response);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Broadweb Response Error</title>\
         <style>body{{font-family:sans-serif;margin:48px;color:#262626;background:#fbfaf8}}\
         h1{{font-size:34px;color:#7a2200}}p{{font-size:17px;line-height:1.5}}\
         code{{background:#f1e8e2;padding:4px 6px;border-radius:4px}}</style></head>\
         <body><h1>Broadweb response error</h1>\
         <p>Status: <code>{status_code}</code></p>\
         <p>Address: <code>{escaped_address}</code></p>\
         <p>Final URL: <code>{escaped_final_url}</code></p>\
         <p>Content type: <code>{escaped_content_type}</code></p>\
         {body_excerpt}\
         </body></html>"
    )
}

fn response_body_excerpt(response: &HttpFetchResponse) -> String {
    let text = response.body_text_lossy();
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let excerpt: String = text.chars().take(280).collect();
    format!("<p>{}</p>", escape_html_text(&excerpt))
}

fn download_ready_html(
    address: &str,
    content_type: Option<&str>,
    suggested_filename: &str,
    download: Option<&DownloadRecord>,
) -> String {
    let escaped_address = escape_html_text(address);
    let escaped_content_type = escape_html_text(content_type.unwrap_or("unknown"));
    let escaped_filename = escape_html_text(suggested_filename);
    let download_details = download.map_or_else(String::new, |download| {
        format!(
            "<p>Saved to <code>{}</code></p>\
             <p>Profile: <code>{}</code></p>\
             <p>Size: {} bytes</p>",
            escape_html_text(&download.path.to_string_lossy()),
            escape_html_text(&download.profile),
            download.size_bytes
        )
    });
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>Download Ready</title>\
         <style>body{{font-family:sans-serif;margin:48px;color:#262626;background:#fbfaf8}}\
         h1{{font-size:34px;color:#0b6b68}}p{{font-size:17px;line-height:1.5}}\
         code{{background:#e5f0ee;padding:4px 6px;border-radius:4px}}</style></head>\
         <body><h1>Download ready</h1>\
         <p><code>{escaped_filename}</code></p>\
         <p>{escaped_content_type}</p>\
         <p>{escaped_address}</p>\
         {download_details}\
         <p>The response was fetched through broadwebd and should be handed to Slate's download flow.</p>\
         </body></html>"
    )
}

fn broadweb_html_with_document_base(address: &str, html: &str) -> String {
    if !has_ipfs_service_scheme(address) || contains_base_tag(html) {
        return html.to_string();
    }

    let base = format!("<base href=\"{}\">", escape_html_text(address));
    insert_head_child(html, &base)
}

fn contains_base_tag(html: &str) -> bool {
    html.to_ascii_lowercase().contains("<base")
}

fn insert_head_child(html: &str, child: &str) -> String {
    if let Some(insert_at) = opening_tag_end(html, "<head") {
        return insert_at_byte(html, insert_at, child);
    }

    let head = format!("<head>{child}</head>");
    if let Some(insert_at) = opening_tag_end(html, "<html") {
        return insert_at_byte(html, insert_at, &head);
    }

    format!("{head}{html}")
}

fn opening_tag_end(html: &str, tag_start: &str) -> Option<usize> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find(tag_start)?;
    html[start..].find('>').map(|offset| start + offset + 1)
}

fn insert_at_byte(input: &str, index: usize, value: &str) -> String {
    let mut output = String::with_capacity(input.len() + value.len());
    output.push_str(&input[..index]);
    output.push_str(value);
    output.push_str(&input[index..]);
    output
}

fn has_servo_supported_scheme(address: &str) -> bool {
    Url::parse(address)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https" | "file" | "data" | "about"))
}

fn has_http_service_scheme(address: &str) -> bool {
    Url::parse(address)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn has_ipfs_service_scheme(address: &str) -> bool {
    Url::parse(address)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "ipfs" | "ipns"))
}

fn has_broadweb_scheme(address: &str) -> bool {
    Url::parse(address)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "ipfs" | "ipns" | "i2p" | "gemini" | "magnet"))
}

fn requires_private_network_host_adapter(address: &str) -> bool {
    let lower = address.to_ascii_lowercase();
    lower.contains(".onion") || lower.contains(".i2p")
}

fn data_html_url(html: &str) -> String {
    format!("data:text/html;charset=utf-8,{}", percent_encode_data(html))
}

fn percent_encode_data(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();

    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte / 16)]));
            output.push(char::from(HEX[usize::from(byte % 16)]));
        }
    }

    output
}

fn escape_html_text(input: &str) -> String {
    let mut output = String::new();
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
    use super::{
        RenderBackend, RenderDocument, RenderViewport, ServoBackend, ServoDocumentSource,
        ServoDocumentStatus, VENDORED_SERVO_PATH, broadweb_html_with_document_base,
        with_test_broadwebd,
    };
    use slate_broadwebd::{
        BroadwebDaemon, HttpFetchService, IpfsConfig, IpfsService, PluginRegistry,
    };
    use std::fs;
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn servo_backend_points_at_vendored_path() {
        let backend = ServoBackend;
        assert_eq!(backend.vendored_path(), VENDORED_SERVO_PATH);
        assert_eq!(backend.load_home().address, "slate://home");
    }

    #[test]
    fn servo_backend_headless_rendering_smoke_tests() {
        servo_backend_renders_generated_html_with_servo();
        servo_backend_executes_css_and_javascript();
        servo_backend_renders_requested_viewport_size();
        servo_backend_reads_local_html_file();
        private_protocol_addresses_do_not_fall_through_to_web();
        broadweb_schemes_use_servo_protocol_callback();
        servo_backend_renders_ipfs_fixture_with_subresource();
        servo_backend_renders_ipfs_kubo_fixture_with_subresource();
        servo_backend_renders_ipns_fixture();
        servo_backend_records_ipfs_download_fixture();
        servo_backend_renders_ipfs_gateway_error_fixture();
    }

    fn servo_backend_renders_generated_html_with_servo() {
        let surface = ServoBackend.load_address("slate://tests/hello");

        assert_eq!(surface.title, "Slate Servo Test");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::SlateGenerated);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
        assert!(!document.frame.pixels.is_empty());
    }

    fn servo_backend_executes_css_and_javascript() {
        let surface = ServoBackend.render_html(
            "slate://tests/css-js",
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <title>Before Script</title>\
             <style>body{background:#143d3a;color:white}h1{font-size:42px}</style>\
             <script>document.title='After Script';</script></head>\
             <body><h1>Styled Page</h1></body></html>",
            ServoDocumentSource::SlateGenerated,
        );

        assert_eq!(surface.title, "After Script");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
        assert!(!document.frame.pixels.is_empty());
    }

    fn servo_backend_renders_requested_viewport_size() {
        let viewport = RenderViewport::new(640, 360);
        let surface = ServoBackend.render_html_with_viewport(
            "slate://tests/viewport",
            "<!doctype html><title>Viewport Fixture</title><body>Viewport</body>",
            ServoDocumentSource::SlateGenerated,
            viewport,
        );

        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.frame.width, 640);
        assert_eq!(document.frame.height, 360);
    }

    fn servo_backend_reads_local_html_file() {
        let path =
            std::env::temp_dir().join(format!("slate local html {}.html", std::process::id()));
        fs::write(
            &path,
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <title>Local Servo Fixture</title>\
             <style>body{background:#f0faf8}</style>\
             <script>document.title='Local Servo Script';</script></head>\
             <body><h1>Local Heading</h1></body></html>",
        )
        .expect("write local fixture");

        let encoded_path = path.to_string_lossy().replace(' ', "%20");
        let address = format!("file://{encoded_path}");
        let surface = ServoBackend.load_address(&address);
        let _ = fs::remove_file(&path);

        assert_eq!(surface.title, "Local Servo Script");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::LocalFile);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
    }

    fn private_protocol_addresses_do_not_fall_through_to_web() {
        let surface = ServoBackend.load_address("http://example.onion");

        assert_eq!(surface.title, "Protocol Adapter Required");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::Blocked);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
    }

    fn broadweb_schemes_use_servo_protocol_callback() {
        let surface = ServoBackend.load_address("gemini://example.test");

        assert_eq!(surface.title, "Broadweb Route Pending");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::Broadweb);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
    }

    fn servo_backend_renders_ipfs_fixture_with_subresource() {
        let (gateway, server) = local_ipfs_gateway_fixture();
        let state_root = test_state_root("rendering-ipfs-fixture");
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local IPFS gateway config"),
        ));
        registry.register_service(HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(&state_root, Default::default(), registry)
            .expect("test broadwebd");
        let surface = with_test_broadwebd(daemon, || {
            ServoBackend.load_address_with_viewport(
                "ipfs://bafybeigdyrzt/index.html",
                RenderViewport::new(640, 360),
            )
        });
        let requests = server.join().expect("gateway fixture");
        let style_download_path = state_root
            .join("profiles")
            .join("default")
            .join("temporary")
            .join("downloads")
            .join("style.css");

        assert_eq!(surface.title, "IPFS Fixture Ready");
        assert_metric(&surface.metrics, "Profile", "default");
        assert_metric(&surface.metrics, "Transport", "ipfs-gateway");
        assert_metric(&surface.metrics, "Boundary", "Local IPFS Gateway");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::Broadweb);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
        assert_eq!(document.frame.width, 640);
        assert_eq!(document.frame.height, 360);
        assert!(
            requests
                .iter()
                .any(|request| request == "/ipfs/bafybeigdyrzt/index.html")
        );
        assert!(
            requests
                .iter()
                .any(|request| request == "/ipfs/bafybeigdyrzt/style.css"),
            "expected Servo to load relative IPFS CSS through broadwebd, got {requests:?}"
        );
        assert!(
            !style_download_path.exists(),
            "IPFS CSS subresource should not be recorded as a user download"
        );
        let _ = fs::remove_dir_all(state_root);
    }

    fn servo_backend_renders_ipfs_kubo_fixture_with_subresource() {
        let (rpc, server) = local_ipfs_kubo_rpc_fixture();
        let state_root = test_state_root("rendering-ipfs-kubo-fixture");
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::with_kubo_rpc(&rpc).expect("local Kubo RPC config"),
        ));
        registry.register_service(HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(&state_root, Default::default(), registry)
            .expect("test broadwebd");
        let surface = with_test_broadwebd(daemon, || {
            ServoBackend.load_address_with_viewport(
                "ipfs://bafybeigdyrzt/index.html",
                RenderViewport::new(640, 360),
            )
        });
        let requests = server.join().expect("Kubo fixture");
        let style_download_path = state_root
            .join("profiles")
            .join("default")
            .join("temporary")
            .join("downloads")
            .join("style.css");

        assert_eq!(surface.title, "IPFS Kubo Fixture Ready");
        assert_metric(&surface.metrics, "Profile", "default");
        assert_metric(&surface.metrics, "Transport", "ipfs-kubo-rpc");
        assert_metric(&surface.metrics, "Boundary", "Local Kubo RPC");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::Broadweb);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
        assert_eq!(document.frame.width, 640);
        assert_eq!(document.frame.height, 360);
        assert!(
            requests
                .iter()
                .any(|request| request == "/api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html")
        );
        assert!(
            requests
                .iter()
                .any(|request| request == "/api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Fstyle.css"),
            "expected Servo to load relative IPFS CSS through Kubo RPC, got {requests:?}"
        );
        assert!(
            !style_download_path.exists(),
            "Kubo-backed IPFS CSS subresource should not be recorded as a user download"
        );
        let _ = fs::remove_dir_all(state_root);
    }

    fn servo_backend_renders_ipns_fixture() {
        let (gateway, server) = local_ipns_gateway_fixture();
        let state_root = test_state_root("rendering-ipns-fixture");
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local IPFS gateway config"),
        ));
        registry.register_service(HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(&state_root, Default::default(), registry)
            .expect("test broadwebd");
        let surface = with_test_broadwebd(daemon, || {
            ServoBackend.load_address_with_viewport(
                "ipns://example.net/index.html",
                RenderViewport::new(640, 360),
            )
        });
        let requests = server.join().expect("gateway fixture");
        let _ = fs::remove_dir_all(state_root);

        assert_eq!(surface.title, "IPNS Fixture Ready");
        assert_metric(&surface.metrics, "Profile", "default");
        assert_metric(&surface.metrics, "Transport", "ipfs-gateway");
        assert_metric(&surface.metrics, "Boundary", "Local IPFS Gateway");
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::Broadweb);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
        assert!(
            requests
                .iter()
                .any(|request| request == "/ipns/example.net/index.html"),
            "expected IPNS page to be fetched through broadwebd, got {requests:?}"
        );
    }

    fn servo_backend_records_ipfs_download_fixture() {
        let (gateway, server) = local_ipfs_download_gateway_fixture();
        let state_root = test_state_root("rendering-ipfs-download-fixture");
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local IPFS gateway config"),
        ));
        registry.register_service(HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(&state_root, Default::default(), registry)
            .expect("test broadwebd");
        let surface = with_test_broadwebd(daemon, || {
            ServoBackend.load_address_with_viewport(
                "ipfs://bafybeigdyrzt/picture.png",
                RenderViewport::new(640, 360),
            )
        });
        let requests = server.join().expect("gateway fixture");
        let download_path = state_root
            .join("profiles")
            .join("default")
            .join("temporary")
            .join("downloads")
            .join("picture.png");

        assert_eq!(surface.title, "Download Ready");
        assert_metric(&surface.metrics, "Profile", "default");
        assert_metric(&surface.metrics, "Transport", "ipfs-gateway");
        assert_eq!(
            fs::read(&download_path).expect("read recorded IPFS download"),
            b"png-ish"
        );
        assert!(
            requests
                .iter()
                .any(|request| request == "/ipfs/bafybeigdyrzt/picture.png"),
            "expected IPFS download to be fetched through broadwebd, got {requests:?}"
        );

        let _ = fs::remove_dir_all(state_root);
    }

    fn servo_backend_renders_ipfs_gateway_error_fixture() {
        let (gateway, server) = local_ipfs_error_gateway_fixture();
        let state_root = test_state_root("rendering-ipfs-error-fixture");
        let mut registry = PluginRegistry::new();
        registry.register_protocol_service(IpfsService::new(
            IpfsConfig::new(&gateway).expect("local IPFS gateway config"),
        ));
        registry.register_service(HttpFetchService);
        let daemon = BroadwebDaemon::start_with_registry(&state_root, Default::default(), registry)
            .expect("test broadwebd");
        let surface = with_test_broadwebd(daemon, || {
            ServoBackend.load_address_with_viewport(
                "ipfs://bafybeigdyrzt/missing.txt",
                RenderViewport::new(640, 360),
            )
        });
        let requests = server.join().expect("gateway fixture");
        let download_path = state_root
            .join("profiles")
            .join("default")
            .join("temporary")
            .join("downloads")
            .join("missing.txt");

        assert_eq!(surface.title, "Broadweb Response Error");
        assert_metric(&surface.metrics, "Profile", "default");
        assert_metric(&surface.metrics, "Transport", "ipfs-gateway");
        assert!(
            !download_path.exists(),
            "IPFS gateway error should not be recorded as a user download"
        );
        let RenderDocument::Web(document) = surface.document else {
            panic!("expected Servo document");
        };
        assert_eq!(document.source, ServoDocumentSource::Blocked);
        assert_eq!(document.status, ServoDocumentStatus::Rendered);
        assert!(
            requests
                .iter()
                .any(|request| request == "/ipfs/bafybeigdyrzt/missing.txt"),
            "expected IPFS error to be fetched through broadwebd, got {requests:?}"
        );

        let _ = fs::remove_dir_all(state_root);
    }

    #[test]
    fn broadweb_html_injects_ipfs_document_base() {
        let html = broadweb_html_with_document_base(
            "ipfs://bafybeigdyrzt/site/index.html",
            "<!doctype html><html><head><title>IPFS</title></head><body></body></html>",
        );

        assert!(html.contains(
            "<head><base href=\"ipfs://bafybeigdyrzt/site/index.html\"><title>IPFS</title>"
        ));
    }

    #[test]
    fn broadweb_html_preserves_existing_base() {
        let original = "<!doctype html><html><head><base href=\"ipfs://example/\"><title>IPFS</title></head></html>";
        let html = broadweb_html_with_document_base("ipfs://bafybeigdyrzt", original);

        assert_eq!(html, original);
    }

    #[test]
    fn broadweb_html_does_not_base_non_ipfs_documents() {
        let original = "<!doctype html><html><head><title>HTTP</title></head></html>";
        let html = broadweb_html_with_document_base("https://example.test", original);

        assert_eq!(html, original);
    }

    fn assert_metric(metrics: &[super::RenderMetric], label: &str, value: &str) {
        assert!(
            metrics
                .iter()
                .any(|metric| metric.label == label && metric.value == value),
            "expected metric {label}={value}, got {metrics:?}"
        );
    }

    fn local_ipfs_gateway_fixture() -> (String, thread::JoinHandle<Vec<String>>) {
        local_gateway_fixture(2, ipfs_fixture_response)
    }

    fn local_ipns_gateway_fixture() -> (String, thread::JoinHandle<Vec<String>>) {
        local_gateway_fixture(1, ipns_fixture_response)
    }

    fn local_ipfs_download_gateway_fixture() -> (String, thread::JoinHandle<Vec<String>>) {
        local_gateway_fixture(1, ipfs_download_fixture_response)
    }

    fn local_ipfs_error_gateway_fixture() -> (String, thread::JoinHandle<Vec<String>>) {
        local_gateway_fixture(1, ipfs_error_fixture_response)
    }

    fn local_ipfs_kubo_rpc_fixture() -> (String, thread::JoinHandle<Vec<String>>) {
        local_gateway_fixture(2, ipfs_kubo_fixture_response)
    }

    fn local_gateway_fixture(
        expected_requests: usize,
        response_for: fn(&str) -> (&'static str, &'static str, &'static str),
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind IPFS gateway fixture");
        listener
            .set_nonblocking(true)
            .expect("set fixture listener nonblocking");
        let address = format!("http://{}", listener.local_addr().expect("local address"));
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(5);
            while requests.len() < expected_requests && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = [0_u8; 2048];
                        let read = stream.read(&mut request).expect("read fixture request");
                        let request = String::from_utf8_lossy(&request[..read]);
                        let path = request_path(&request);
                        let (status, content_type, body) = response_for(&path);
                        requests.push(path);
                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream
                            .write_all(response.as_bytes())
                            .expect("write fixture response");
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("accept fixture request: {error}"),
                }
            }
            requests
        });
        (address, server)
    }

    fn request_path(request: &str) -> String {
        request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_string()
    }

    fn ipfs_fixture_response(path: &str) -> (&'static str, &'static str, &'static str) {
        match path {
            "/ipfs/bafybeigdyrzt/index.html" => (
                "200 OK",
                "text/html; charset=utf-8",
                "<!doctype html><html><head><meta charset=\"utf-8\">\
                 <title>IPFS Fixture</title><link rel=\"stylesheet\" href=\"style.css\">\
                 <script>document.title='IPFS Fixture Ready';</script></head>\
                 <body><h1>Fetched through broadwebd</h1></body></html>",
            ),
            "/ipfs/bafybeigdyrzt/style.css" => (
                "200 OK",
                "text/css",
                "body{background:#eefaf7;color:#12302c}",
            ),
            _ => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                "missing fixture path",
            ),
        }
    }

    fn ipfs_kubo_fixture_response(path: &str) -> (&'static str, &'static str, &'static str) {
        match path {
            "/api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Findex.html" => (
                "200 OK",
                "text/html; charset=utf-8",
                "<!doctype html><html><head><meta charset=\"utf-8\">\
                 <title>IPFS Kubo Fixture</title><link rel=\"stylesheet\" href=\"style.css\">\
                 <script>document.title='IPFS Kubo Fixture Ready';</script></head>\
                 <body><h1>Fetched through Kubo RPC</h1></body></html>",
            ),
            "/api/v0/cat?arg=%2Fipfs%2Fbafybeigdyrzt%2Fstyle.css" => (
                "200 OK",
                "text/css",
                "body{background:#eef8ff;color:#102a43}",
            ),
            _ => (
                "500 Internal Server Error",
                "text/plain; charset=utf-8",
                "missing Kubo fixture path",
            ),
        }
    }

    fn ipns_fixture_response(path: &str) -> (&'static str, &'static str, &'static str) {
        match path {
            "/ipns/example.net/index.html" => (
                "200 OK",
                "text/html; charset=utf-8",
                "<!doctype html><html><head><meta charset=\"utf-8\">\
                 <title>IPNS Fixture</title>\
                 <script>document.title='IPNS Fixture Ready';</script></head>\
                 <body><h1>Fetched from IPNS through broadwebd</h1></body></html>",
            ),
            _ => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                "missing fixture path",
            ),
        }
    }

    fn ipfs_download_fixture_response(path: &str) -> (&'static str, &'static str, &'static str) {
        match path {
            "/ipfs/bafybeigdyrzt/picture.png" => ("200 OK", "image/png", "png-ish"),
            _ => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                "missing fixture path",
            ),
        }
    }

    fn ipfs_error_fixture_response(path: &str) -> (&'static str, &'static str, &'static str) {
        match path {
            "/ipfs/bafybeigdyrzt/missing.txt" => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                "missing IPFS content",
            ),
            _ => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                "missing fixture path",
            ),
        }
    }

    fn test_state_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "slate-rendering-test-{}-{name}",
            std::process::id()
        ))
    }
}
