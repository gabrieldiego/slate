#![forbid(unsafe_code)]

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyPolicy {
    pub telemetry_enabled: bool,
    pub public_gateway_fallback: bool,
    pub persist_private_session: bool,
}

impl Default for PrivacyPolicy {
    fn default() -> Self {
        Self {
            telemetry_enabled: false,
            public_gateway_fallback: false,
            persist_private_session: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PrivacyPolicy;

    #[test]
    fn defaults_are_conservative() {
        let policy = PrivacyPolicy::default();
        assert!(!policy.telemetry_enabled);
        assert!(!policy.public_gateway_fallback);
        assert!(!policy.persist_private_session);
    }
}
