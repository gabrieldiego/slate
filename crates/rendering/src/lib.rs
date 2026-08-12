#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

pub const VENDORED_SERVO_PATH: &str = "third_party/servo";

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
    Html(HtmlDocument),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlDocument {
    pub title: String,
    pub heading: String,
    pub paragraphs: Vec<String>,
    pub source: HtmlDocumentSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HtmlDocumentSource {
    BuiltinShim,
    NavigationShim,
    LocalFile,
    WebFetch,
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
        source: HtmlDocumentSource,
    ) -> RenderSurface {
        HtmlShim::from_html(address, html, source).render()
    }

    pub fn render_error(
        self,
        address: &str,
        title: &str,
        heading: &str,
        details: &[String],
        source: HtmlDocumentSource,
    ) -> RenderSurface {
        let escaped_title = escape_html_text(title);
        let escaped_heading = escape_html_text(heading);
        let mut body = format!(
            "<!doctype html><html><head><title>{escaped_title}</title></head>\
             <body><h1>{escaped_heading}</h1>"
        );
        for detail in details {
            body.push_str("<p>");
            body.push_str(&escape_html_text(detail));
            body.push_str("</p>");
        }
        body.push_str("</body></html>");
        HtmlShim::from_html(address, body, source).render()
    }
}

impl RenderBackend for ServoBackend {
    fn name(&self) -> &'static str {
        "Servo vendored backend"
    }

    fn load_home(&self) -> RenderSurface {
        home_surface()
    }

    fn load_address(&self, address: &str) -> RenderSurface {
        match address {
            "slate://home" | "slate://new" => home_surface(),
            address if address.starts_with("slate://tests/") => {
                HtmlShim::builtin_test(address).render()
            }
            address if address.starts_with("slate://search?") => HtmlShim::search(address).render(),
            address if address.starts_with("slate://") => HtmlShim::internal(address).render(),
            address if address.starts_with("file://") => HtmlShim::local_file(address).render(),
            address => HtmlShim::navigation(address).render(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HtmlShim {
    address: String,
    html: String,
    source: HtmlDocumentSource,
}

impl HtmlShim {
    fn from_html(address: &str, html: impl Into<String>, source: HtmlDocumentSource) -> Self {
        Self {
            address: address.to_string(),
            html: html.into(),
            source,
        }
    }

    fn builtin_test(address: &str) -> Self {
        let escaped_address = escape_html_text(address);
        Self::from_html(
            address,
            format!(
                "<!doctype html><html><head><title>Slate HTML Shim</title></head>\
                 <body><h1>Hello from Slate</h1>\
                 <p>This test page is rendered through the Servo backend boundary.</p>\
                 <p>{escaped_address}</p></body></html>"
            ),
            HtmlDocumentSource::BuiltinShim,
        )
    }

    fn search(address: &str) -> Self {
        let query = address
            .split_once("?q=")
            .map(|(_, value)| value.replace('+', " "))
            .unwrap_or_default();
        let escaped_query = escape_html_text(&query);
        Self::from_html(
            address,
            format!(
                "<!doctype html><html><head><title>Search</title></head>\
                 <body><h1>{escaped_query}</h1>\
                 <p>Local search shim.</p>\
                 <p>No remote search request has been issued.</p></body></html>"
            ),
            HtmlDocumentSource::BuiltinShim,
        )
    }

    fn internal(address: &str) -> Self {
        let escaped_address = escape_html_text(address);
        Self::from_html(
            address,
            format!(
                "<!doctype html><html><head><title>Slate Internal Page</title></head>\
                 <body><h1>Slate Internal Page</h1>\
                 <p>{escaped_address}</p>\
                 <p>This address is handled locally.</p></body></html>"
            ),
            HtmlDocumentSource::BuiltinShim,
        )
    }

    fn navigation(address: &str) -> Self {
        let escaped_address = escape_html_text(address);
        Self::from_html(
            address,
            format!(
                "<!doctype html><html><head><title>{escaped_address}</title></head>\
                 <body><h1>{escaped_address}</h1>\
                 <p>Loaded by the Servo navigation shim.</p>\
                 <p>Full document loading will replace this surface.</p></body></html>"
            ),
            HtmlDocumentSource::NavigationShim,
        )
    }

    fn local_file(address: &str) -> Self {
        match read_local_html(address) {
            Ok(html) => Self::from_html(address, html, HtmlDocumentSource::LocalFile),
            Err(error) => {
                let escaped_address = escape_html_text(address);
                let escaped_error = escape_html_text(&error);
                Self::from_html(
                    address,
                    format!(
                        "<!doctype html><html><head><title>Local File Error</title></head>\
                         <body><h1>Could not read local file</h1>\
                         <p>{escaped_address}</p><p>{escaped_error}</p></body></html>"
                    ),
                    HtmlDocumentSource::LocalFile,
                )
            }
        }
    }

    fn render(self) -> RenderSurface {
        let title = first_tag_text(&self.html, "title").unwrap_or_else(|| self.address.clone());
        let heading = first_tag_text(&self.html, "h1").unwrap_or_else(|| title.clone());
        let paragraphs = tag_texts(&self.html, "p");
        let summary = paragraphs
            .first()
            .cloned()
            .unwrap_or_else(|| heading.clone());

        RenderSurface {
            title: title.clone(),
            address: self.address,
            summary,
            metrics: document_metrics(self.source),
            document: RenderDocument::Html(HtmlDocument {
                title,
                heading,
                paragraphs,
                source: self.source,
            }),
        }
    }
}

fn read_local_html(address: &str) -> Result<String, String> {
    let path = file_path_from_address(address)
        .ok_or_else(|| "expected file:///absolute/path.html".to_string())?;
    fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.to_string_lossy()))
}

fn file_path_from_address(address: &str) -> Option<PathBuf> {
    let file = address.strip_prefix("file://")?;
    let path = file
        .strip_prefix("localhost/")
        .map(|path| format!("/{path}"))
        .unwrap_or_else(|| file.to_string());
    if !path.starts_with('/') {
        return None;
    }

    Some(PathBuf::from(percent_decode(&path)))
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

fn document_metrics(source: HtmlDocumentSource) -> Vec<RenderMetric> {
    match source {
        HtmlDocumentSource::WebFetch => vec![
            RenderMetric {
                label: "HTML".to_string(),
                value: "Web".to_string(),
                accent: MetricAccent::Teal,
            },
            RenderMetric {
                label: "Scripts".to_string(),
                value: "Off".to_string(),
                accent: MetricAccent::Amber,
            },
            RenderMetric {
                label: "Route".to_string(),
                value: "Web".to_string(),
                accent: MetricAccent::Blue,
            },
        ],
        HtmlDocumentSource::LocalFile => vec![
            RenderMetric {
                label: "HTML".to_string(),
                value: "File".to_string(),
                accent: MetricAccent::Teal,
            },
            RenderMetric {
                label: "Scripts".to_string(),
                value: "Off".to_string(),
                accent: MetricAccent::Amber,
            },
            RenderMetric {
                label: "Route".to_string(),
                value: "Disk".to_string(),
                accent: MetricAccent::Blue,
            },
        ],
        HtmlDocumentSource::BuiltinShim | HtmlDocumentSource::NavigationShim => vec![
            RenderMetric {
                label: "HTML".to_string(),
                value: "Shim".to_string(),
                accent: MetricAccent::Teal,
            },
            RenderMetric {
                label: "Scripts".to_string(),
                value: "Off".to_string(),
                accent: MetricAccent::Amber,
            },
            RenderMetric {
                label: "Route".to_string(),
                value: "Local".to_string(),
                accent: MetricAccent::Blue,
            },
        ],
    }
}

fn first_tag_text(html: &str, tag: &str) -> Option<String> {
    tag_texts(html, tag).into_iter().next()
}

fn tag_texts(html: &str, tag: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut results = Vec::new();
    let mut cursor = 0;

    while let Some(open_relative) = lower[cursor..].find(&open_prefix) {
        let open = cursor.saturating_add(open_relative);
        let Some(open_end_relative) = lower[open..].find('>') else {
            break;
        };
        let content_start = open.saturating_add(open_end_relative).saturating_add(1);
        let Some(close_relative) = lower[content_start..].find(&close_tag) else {
            break;
        };
        let content_end = content_start.saturating_add(close_relative);
        let text = strip_tags(&html[content_start..content_end]);
        if !text.is_empty() {
            results.push(text);
        }
        cursor = content_end.saturating_add(close_tag.len());
    }

    results
}

fn strip_tags(input: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;

    for ch in input.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => output.push(ch),
            _ => {}
        }
    }

    collapse_whitespace(&decode_basic_entities(&output))
}

fn decode_basic_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
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

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push(high.saturating_mul(16).saturating_add(low));
            index += 3;
            continue;
        }

        output.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.saturating_sub(b'0')),
        b'a'..=b'f' => Some(byte.saturating_sub(b'a').saturating_add(10)),
        b'A'..=b'F' => Some(byte.saturating_sub(b'A').saturating_add(10)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HtmlDocumentSource, RenderBackend, RenderDocument, ServoBackend, VENDORED_SERVO_PATH,
    };
    use std::fs;

    #[test]
    fn servo_backend_points_at_vendored_path() {
        let backend = ServoBackend;
        assert_eq!(backend.vendored_path(), VENDORED_SERVO_PATH);
        assert_eq!(backend.load_home().address, "slate://home");
    }

    #[test]
    fn servo_backend_loads_builtin_html_shim() {
        let backend = ServoBackend;
        let surface = backend.load_address("slate://tests/hello");

        assert_eq!(surface.title, "Slate HTML Shim");
        assert_eq!(
            surface.summary,
            "This test page is rendered through the Servo backend boundary."
        );
        let RenderDocument::Html(document) = surface.document else {
            panic!("expected HTML document");
        };
        assert_eq!(document.heading, "Hello from Slate");
        assert_eq!(document.source, HtmlDocumentSource::BuiltinShim);
    }

    #[test]
    fn servo_backend_loads_navigation_shim() {
        let backend = ServoBackend;
        let surface = backend.load_address("https://example.com");

        assert_eq!(surface.title, "https://example.com");
        assert!(matches!(
            surface.document,
            RenderDocument::Html(super::HtmlDocument {
                source: HtmlDocumentSource::NavigationShim,
                ..
            })
        ));
    }

    #[test]
    fn servo_backend_renders_fetched_html_body() {
        let surface = ServoBackend.render_html(
            "https://example.test",
            "<!doctype html><title>Fetched Fixture</title><h1>Fetched Heading</h1><p>Fetched body.</p>",
            HtmlDocumentSource::WebFetch,
        );

        assert_eq!(surface.title, "Fetched Fixture");
        assert_eq!(surface.summary, "Fetched body.");
        let RenderDocument::Html(document) = surface.document else {
            panic!("expected HTML document");
        };
        assert_eq!(document.heading, "Fetched Heading");
        assert_eq!(document.source, HtmlDocumentSource::WebFetch);
    }

    #[test]
    fn servo_backend_reads_local_html_file() {
        let path =
            std::env::temp_dir().join(format!("slate local html {}.html", std::process::id()));
        fs::write(
            &path,
            "<!doctype html><html><head><title>Local Fixture</title></head>\
             <body><h1>Local Heading</h1><p>Read from disk.</p></body></html>",
        )
        .expect("write local fixture");

        let encoded_path = path.to_string_lossy().replace(' ', "%20");
        let address = format!("file://{encoded_path}");
        let surface = ServoBackend.load_address(&address);
        let _ = fs::remove_file(&path);

        assert_eq!(surface.title, "Local Fixture");
        assert_eq!(surface.summary, "Read from disk.");
        let RenderDocument::Html(document) = surface.document else {
            panic!("expected HTML document");
        };
        assert_eq!(document.heading, "Local Heading");
        assert_eq!(document.source, HtmlDocumentSource::LocalFile);
    }

    #[test]
    fn servo_backend_renders_missing_local_file_error() {
        let address = "file:///tmp/slate-missing-local-page.html";
        let surface = ServoBackend.load_address(address);

        assert_eq!(surface.title, "Local File Error");
        let RenderDocument::Html(document) = surface.document else {
            panic!("expected HTML document");
        };
        assert_eq!(document.heading, "Could not read local file");
        assert_eq!(document.source, HtmlDocumentSource::LocalFile);
    }
}
