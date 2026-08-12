#![forbid(unsafe_code)]

use slate_routing::RoutingPlan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestPolicy {
    pub allow_startup_network: bool,
}

impl Default for RequestPolicy {
    fn default() -> Self {
        Self {
            allow_startup_network: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedRequest {
    pub route: RoutingPlan,
    pub policy: RequestPolicy,
}

impl PlannedRequest {
    pub fn new(route: RoutingPlan, policy: RequestPolicy) -> Self {
        Self { route, policy }
    }
}
