use envoy_ext_proc_proto::envoy::service::ext_proc::v3::{
    processing_response, HeadersResponse, HttpHeaders, ProcessingResponse,
};
use tonic::Status;

use envoy_ext_proc_proto::envoy::config::core::v3::HeaderMap;

use crate::config::Config;
use crate::root_context::EventRootContext;
use reqwest::header::HeaderMap as ReqwestHeaderMap;

use crate::event::{Event, ResponseInfo};
use bytes::Bytes;
use chrono::Utc;
use log::LevelFilter;
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

type Headers = Vec<(String, String)>;

// Handle request headers
pub async fn process_request_headers(
    config: &Arc<Config>,
    event: &mut Event,
    headers_msg: &HttpHeaders,
) {
    log::trace!("Processing request headers...");

    let headers = headers_msg.headers.as_ref();
    if headers.is_none() {
        log::warn!("No headers found in request.");
    } else {
        log::trace!("Headers found: {:?}", headers);
    }

    event.direction = "Incoming".to_string();

    // Process and extract relevant headers
    event.request.headers = header_list_to_map(headers.cloned());
    log::trace!("Parsed headers: {:?}", event.request.headers);

    event.request.uri = event
        .request
        .headers
        .get(":path")
        .unwrap_or(&"".into())
        .clone();
    log::trace!("Parsed URI: {}", event.request.uri);

    event.request.verb = event
        .request
        .headers
        .get(":method")
        .unwrap_or(&"GET".into())
        .clone();
    log::trace!("Parsed method: {}", event.request.verb);

    event.request.headers.retain(|k, _| !k.starts_with(":"));
    log::trace!("Filtered headers: {:?}", event.request.headers);

    event.request.ip_address = get_client_ip(&event.request.headers);
    log::trace!("Client IP: {:?}", event.request.ip_address);

    event.request.api_version = event.request.headers.get("x-api-version").cloned();
    log::trace!("API Version: {:?}", event.request.api_version);

    event.request.transfer_encoding = event.request.headers.get("transfer-encoding").cloned();
    log::trace!("Transfer Encoding: {:?}", event.request.transfer_encoding);

    add_env_headers_to_event(config, event).await;

    log_event(event);
}

// Handle response headers
pub async fn process_response_headers(event: &mut Event, response_headers_msg: &HttpHeaders) {
    log::trace!("Processing response headers...");
    log::trace!("Received Response Headers: {:?}", response_headers_msg);

    if let Some(header_map) = &response_headers_msg.headers {
        log::trace!("List of Headers in HTTP Response:");

        for header in &header_map.headers {
            log::trace!(
                "{} - {}",
                header.key,
                String::from_utf8_lossy(&header.raw_value)
            );
        }
    } else {
        log::warn!("No headers found in response.");
    }

    let status_str = extract_status(response_headers_msg);

    let response = ResponseInfo {
        time: Utc::now().to_rfc3339(),
        status: status_str.parse::<usize>().unwrap_or(0),
        headers: header_list_to_map(response_headers_msg.headers.clone()),
        ip_address: None,
        body: serde_json::Value::Null,
    };

    event.response = Some(response);
}

pub async fn store_and_flush_event(event_context: &Arc<Mutex<EventRootContext>>, event: &Event) {
    log_event(event);

    let mut event_root_context = event_context.lock().await;

    // Add the event to the main buffer
    event_root_context.push_event(event).await;

    // Check if we need to flush the buffer
    event_root_context.check_and_flush_buffer().await;
}

pub async fn send_grpc_response(
    tx: tokio::sync::mpsc::Sender<Result<ProcessingResponse, Status>>,
    response: ProcessingResponse,
) {
    log::trace!("Attempting to send response...");
    if let Err(e) = tx.send(Ok(response)).await {
        log::error!("Error sending response: {:?}", e);
    } else {
        log::trace!("Response sent successfully.");
    }
}

pub async fn add_env_headers_to_event(config: &Arc<Config>, event: &mut Event) {
    // Log the pre-loaded values from EnvConfig
    log::trace!("Config USER_ID_HEADER: {:?}", config.env.user_id_header);
    log::trace!(
        "Config COMPANY_ID_HEADER: {:?}",
        config.env.company_id_header
    );

    // Log the values directly from the environment
    log::trace!("Env USER_ID_HEADER: {:?}", std::env::var("USER_ID_HEADER"));
    log::trace!(
        "Env COMPANY_ID_HEADER: {:?}",
        std::env::var("COMPANY_ID_HEADER")
    );

    if let Some(user_id_header) = &config.env.user_id_header {
        event.user_id = Some(user_id_header.clone());
    }

    if let Some(company_id_header) = &config.env.company_id_header {
        event.company_id = Some(company_id_header.clone());
    }
}

pub fn extract_status(headers_msg: &HttpHeaders) -> String {
    if let Some(header_map) = &headers_msg.headers {
        log::trace!("List of Significant Headers in HTTP Response:");

        for header in &header_map.headers {
            log::trace!(
                "{} - {}",
                header.key,
                String::from_utf8_lossy(&header.raw_value)
            );
        }
    }

    let status_str = headers_msg
        .headers
        .as_ref()
        .and_then(|header_map| {
            header_map
                .headers
                .iter()
                .find(|header| header.key == ":status")
                .map(|header| String::from_utf8_lossy(&header.raw_value).to_string())
        })
        .unwrap_or_else(|| "0".to_string());

    log::trace!("Extracted status: {}", status_str);

    status_str
}

pub fn header_list_to_map(header_map: Option<HeaderMap>) -> HashMap<String, String> {
    let mut map = HashMap::new();

    if let Some(header_map) = header_map {
        for header in header_map.headers {
            let key = header.key.to_lowercase();
            let value = String::from_utf8_lossy(&header.raw_value).to_string(); // Convert raw_value to String
            map.insert(key, value);
        }
    }

    map
}

pub fn get_client_ip(headers: &HashMap<String, String>) -> Option<String> {
    let possible_headers = vec![
        "x-client-ip",
        "x-forwarded-for",
        "cf-connecting-ip",
        "fastly-client-ip",
        "true-client-ip",
        "x-real-ip",
        "x-cluster-client-ip",
        "x-forwarded",
        "forwarded-for",
        "forwarded",
        "x-appengine-user-ip",
        "cf-pseudo-ipv4",
    ];

    for header in possible_headers {
        if let Some(value) = headers.get(header) {
            let ips: Vec<&str> = value.split(',').collect();
            for ip in ips {
                if IpAddr::from_str(ip.trim()).is_ok() {
                    return Some(ip.trim().to_string());
                }
            }
        }
    }
    None
}

pub fn log_event(event: &Event) {
    let json = serde_json::to_string(event).unwrap();
    log::info!("Request & Response Data: {}", json);
}

pub fn serialize_event_to_bytes(event: &Event) -> Bytes {
    Bytes::from(serde_json::to_vec(event).unwrap())
}

pub fn simplified_grpc_response() -> ProcessingResponse {
    let headers_response = HeadersResponse { response: None };

    ProcessingResponse {
        dynamic_metadata: None,
        mode_override: None,
        override_message_timeout: None,
        response: Some(processing_response::Response::RequestHeaders(
            headers_response,
        )),
    }
}

pub fn generate_curl_command(
    method: &str,
    url: &str,
    headers: &ReqwestHeaderMap,
    body: Option<&Bytes>,
) -> String {
    let mut curl_cmd = format!("curl -v -X {} '{}'", method, url);

    // Add headers to the curl command
    for (key, value) in headers {
        let header_value = value.to_str().unwrap_or("");
        curl_cmd.push_str(&format!(" -H '{}: {}'", key, header_value));
    }

    // Add body to the curl command
    if let Some(body) = body {
        let body_str = std::str::from_utf8(body).unwrap_or("");
        curl_cmd.push_str(&format!(" --data '{}'", body_str));
    }

    curl_cmd
}

pub fn get_header(headers: &Headers, name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, header_value)| header_value.to_owned())
}

pub fn set_and_display_log_level(config: &Config) {
    // Check if RUST_LOG is set
    if let Some(rust_log) = &config.env.rust_log {
        match rust_log.to_lowercase().as_str() {
            "trace" => log::set_max_level(LevelFilter::Trace),
            "debug" => log::set_max_level(LevelFilter::Debug),
            "info" => log::set_max_level(LevelFilter::Info),
            "warn" => log::set_max_level(LevelFilter::Warn),
            "error" => log::set_max_level(LevelFilter::Error),
            _ => {
                // If RUST_LOG is set to an invalid value, fall back to default logic
                set_level_based_on_debug(config);
            }
        }
    } else {
        // If RUST_LOG is not set, use the DEBUG environment variable logic
        set_level_based_on_debug(config);
    }

    // Display the current log level
    match log::max_level() {
        LevelFilter::Error => println!("Logging level set to: ERROR"),
        LevelFilter::Warn => println!("Logging level set to: WARN"),
        LevelFilter::Info => println!("Logging level set to: INFO"),
        LevelFilter::Debug => println!("Logging level set to: DEBUG"),
        LevelFilter::Trace => println!("Logging level set to: TRACE"),
        LevelFilter::Off => println!("Logging is turned OFF"),
    }
}

fn set_level_based_on_debug(config: &Config) {
    if config.env.debug {
        log::set_max_level(LevelFilter::Trace);
    } else {
        log::set_max_level(LevelFilter::Warn);
    }
}
