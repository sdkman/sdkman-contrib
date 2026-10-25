use std::time::Duration;

use ureq::config::ConfigBuilder;
use ureq::typestate::AgentScope;

pub fn agent_config() -> ConfigBuilder<AgentScope> {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(30)))
        .user_agent(concat!("sdkman-contrib/", env!("CARGO_PKG_VERSION")))
}
