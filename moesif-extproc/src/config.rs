use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env};

#[derive(Default, Clone)]
pub struct Config {
    pub env: EnvConfig,
    // pub _event_queue_id: u32,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct EnvConfig {
    pub moesif_application_id: String,
    pub user_id_header: Option<String>,
    pub company_id_header: Option<String>,
    #[serde(default = "default_batch_max_size")]
    pub batch_max_size: usize,
    #[serde(default = "default_batch_max_wait")]
    pub batch_max_wait: usize,
    #[serde(default = "default_upstream")]
    pub upstream: String,
    pub base_uri: String,
    #[serde(default = "default_debug")]
    pub debug: bool,
    #[serde(default = "connection_timeout")]
    pub connection_timeout: usize,
    pub rust_log: Option<String>,
}

fn default_batch_max_size() -> usize {
    100
}

fn default_batch_max_wait() -> usize {
    2000
}

fn default_upstream() -> String {
    "outbound|443||api.moesif.net".to_string()
}

fn default_debug() -> bool {
    false
}

fn connection_timeout() -> usize {
    5000
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct AppConfigResponse {
    pub org_id: String,
    pub app_id: String,
    pub sample_rate: i32,
    pub block_bot_traffic: bool,
    pub user_sample_rate: HashMap<String, i32>,
    pub company_sample_rate: HashMap<String, i32>,
    pub user_rules: HashMap<String, Vec<EntityRuleValues>>,
    pub company_rules: HashMap<String, Vec<EntityRuleValues>>,
    pub ip_addresses_blocked_by_name: HashMap<String, String>,
    pub regex_config: Vec<RegexRule>,
    pub billing_config_jsons: HashMap<String, String>,
    pub e_tag: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct EntityRuleValues {
    pub rules: String,
    pub values: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RegexRule {
    pub conditions: Vec<RegexCondition>,
    pub sample_rate: i32,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RegexCondition {
    pub path: String,
    pub value: String,
}

impl EnvConfig {
    pub fn new() -> Self {
        let moesif_application_id =
            env::var("MOESIF_APPLICATION_ID").unwrap_or_else(|_| String::new());
        let user_id_header = env::var("USER_ID_HEADER").ok();
        let company_id_header = env::var("COMPANY_ID_HEADER").ok();
        let batch_max_size = env::var("BATCH_MAX_SIZE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or_else(default_batch_max_size);
        let batch_max_wait = env::var("BATCH_MAX_WAIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or_else(default_batch_max_wait);

        // Use default_upstream() directly in the struct initialization
        let upstream = env::var("UPSTREAM").unwrap_or_else(|_| default_upstream());

        // Attempt to parse base_uri from upstream, fallback to default_base_uri
        let base_uri = Self::parse_upstream_url(&upstream).unwrap();

        let debug = env::var("DEBUG")
            .ok()
            .map_or_else(default_debug, |v| v == "true");

        let rust_log = env::var("RUST_LOG").ok();
        let connection_timeout = env::var("CONNECTION_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or_else(connection_timeout);

        let config = EnvConfig {
            moesif_application_id,
            user_id_header,
            company_id_header,
            batch_max_size,
            batch_max_wait,
            upstream,
            base_uri,
            debug,
            connection_timeout,
            rust_log,
        };

        log::info!("Config initialized: {:?}", config); // Add this line to print the entire config

        config
    }

    fn parse_upstream_url(upstream: &str) -> Result<String, ()> {
        // Logic to parse the upstream string and extract base_uri
        // Example logic assuming the upstream format: "outbound|443||api.moesif.net"
        let parts: Vec<&str> = upstream.split('|').collect();
        if parts.len() == 4 {
            Ok(format!("https://{}", parts[3]))
        } else {
            Err(())
        }
    }
}
