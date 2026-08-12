#![forbid(unsafe_code)]

pub const VENDORED_SERVO_PATH: &str = "third_party/servo";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderSurface {
    pub title: String,
    pub address: String,
    pub summary: String,
    pub metrics: Vec<RenderMetric>,
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
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServoBackend;

impl ServoBackend {
    pub fn vendored_path(self) -> &'static str {
        VENDORED_SERVO_PATH
    }
}

impl RenderBackend for ServoBackend {
    fn name(&self) -> &'static str {
        "Servo vendored backend"
    }

    fn load_home(&self) -> RenderSurface {
        RenderSurface {
            title: "New Tab".to_string(),
            address: "slate://home".to_string(),
            summary: "Servo boundary active; renderer embedding pending.".to_string(),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderBackend, ServoBackend, VENDORED_SERVO_PATH};

    #[test]
    fn servo_backend_points_at_vendored_path() {
        let backend = ServoBackend;
        assert_eq!(backend.vendored_path(), VENDORED_SERVO_PATH);
        assert_eq!(backend.load_home().address, "slate://home");
    }
}
