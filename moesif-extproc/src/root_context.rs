use crate::config::Config;
use crate::utils::*;
use reqwest::header::{HeaderMap as ReqwestHeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};

use crate::event::Event;
use bytes::Bytes;
use std::time::Duration;
use tokio::sync::Mutex;

type CallbackType = Box<dyn Fn(Vec<(String, String)>, Option<Vec<u8>>) + Send>;

#[derive(Default)]
pub struct EventRootContext {
    pub config: Config,
    pub event_byte_buffer: Mutex<Vec<Bytes>>, // Holds serialized, complete events
    // context_id: String,
    is_start: bool,
}

impl EventRootContext {
    pub fn new(config: Config) -> Self {
        EventRootContext {
            config,
            event_byte_buffer: Mutex::new(Vec::new()),
            // context_id: String::new(),
            is_start: true,
        }
    }

    async fn write_events_json(&self, events: Vec<Bytes>) -> Bytes {
        log::trace!("Entering write_events_json with {} events.", events.len());

        let total_size: usize = events.iter().map(|event_bytes| event_bytes.len()).sum();

        let json_array_size = if !events.is_empty() {
            total_size + events.len() - 1 + 2
        } else {
            2 // Just for the empty array '[]'
        };
        let mut event_json_array = Vec::with_capacity(json_array_size);

        event_json_array.push(b'[');
        for (i, event_bytes) in events.iter().enumerate() {
            if i > 0 {
                event_json_array.push(b',');
            }
            event_json_array.extend(event_bytes);

            log::trace!(
                "Adding event to JSON array: {:?}",
                std::str::from_utf8(event_bytes).unwrap_or("Invalid UTF-8")
            );
        }
        event_json_array.push(b']');

        let final_json = std::str::from_utf8(&event_json_array).unwrap_or("Invalid UTF-8");
        log::trace!("Final JSON array being sent: {}", final_json);
        log::trace!(
            "Exiting write_events_json with JSON array size {} bytes.",
            event_json_array.len()
        );
        event_json_array.into() // Return as Bytes
    }

    pub async fn check_and_flush_buffer(&mut self) {
        log::trace!("Entering add_event.");

        let mut immediate_send = false;

        {
            let buffer = self.event_byte_buffer.lock().await;
            log::trace!(
                "Acquired lock on event_byte_buffer. Current buffer size: {}",
                buffer.len()
            );

            if self.is_start {
                // First event in the runtime, perform special action
                immediate_send = true;
                self.is_start = false; // Ensure this block only runs once
                log::trace!("First event processed, setting is_start to false.");
            } else if buffer.len() >= self.config.env.batch_max_size {
                // Buffer full, send immediately
                immediate_send = true;
                log::trace!("Buffer size has reached maximum capacity, triggering flush.");
            }
        }

        if immediate_send {
            self.drain_and_send(1).await;
        }
    }

    pub async fn drain_and_send(&self, drain_at_least: usize) {
        log::trace!(
            "Entering drain_and_send with drain_at_least size: {}",
            drain_at_least
        );

        let mut attempts = 0;
        loop {
            match self.event_byte_buffer.try_lock() {
                Ok(mut buffer) => {
                    log::trace!(
                        "Acquired lock on event_byte_buffer for draining after {} attempts. Current buffer size: {}",
                        attempts, buffer.len()
                    );

                    while buffer.len() >= drain_at_least {
                        log::trace!(
                            "Buffer size {} >= {}. Draining and sending events.",
                            buffer.len(),
                            drain_at_least
                        );

                        log::trace!("Config batch_max_size: {}", self.config.env.batch_max_size);
                        let end = std::cmp::min(buffer.len(), self.config.env.batch_max_size);
                        log::trace!("Calculated end for draining: {}", end);

                        let events_to_send: Vec<Bytes> = buffer.drain(..end).collect();
                        log::trace!(
                            "Drained {} events from buffer for sending.",
                            events_to_send.len()
                        );
                        log::trace!("Buffer size after draining: {}", buffer.len());

                        let body = self.write_events_json(events_to_send).await;

                        log::info!("Dispatching HTTP request with {} events.", end);

                        if let Err(e) = self
                            .dispatch_http_request(
                                "POST",
                                "/v1/events/batch",
                                body,
                                Box::new(|headers, _| {
                                    let config_etag = get_header(&headers, "X-Moesif-Config-Etag");
                                    let rules_etag = get_header(&headers, "X-Moesif-Rules-Etag");
                                    log::info!(
                                        "Event Response eTags: config={:?} rules={:?}",
                                        config_etag,
                                        rules_etag
                                    );
                                }),
                            )
                            .await
                        {
                            log::error!("Failed to dispatch HTTP request: {:?}", e);
                        }

                        log::trace!(
                            "Events drained and sent. Current buffer size: {}",
                            buffer.len()
                        );
                    }

                    log::trace!(
                        "Exiting drain_and_send. Current buffer size: {}",
                        buffer.len()
                    );
                    break;
                }
                Err(_) => {
                    attempts += 1;
                    log::warn!("Failed to acquire lock on event_byte_buffer; will retry after a short delay (attempt: {}).", attempts);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }

    pub async fn push_event(&mut self, event: &Event) {
        let mut buffer = self.event_byte_buffer.lock().await;
        buffer.push(serialize_event_to_bytes(event));
        log::trace!("Event pushed to event_byte_buffer.");
    }

    async fn dispatch_http_request(
        &self,
        method: &str,
        path: &str,
        body: Bytes,
        callback: CallbackType,
    ) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        log::trace!("Entering dispatch_http_request.");

        let client = Client::new();
        let url = format!("{}{}", self.config.env.base_uri, path);

        let method = Method::from_bytes(method.as_bytes())?;
        log::trace!("Using method: {} and URL: {}", method, url);

        let mut headers = ReqwestHeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("x-moesif-application-id"),
            HeaderValue::from_str(&self.config.env.moesif_application_id)?,
        );

        let curl_cmd = generate_curl_command(method.as_str(), &url, &headers, Some(&body));
        log::trace!("Equivalent curl command:\n{}", curl_cmd);

        log::trace!(
            "Dispatching {} request to {} with headers: {:?} and body: {}",
            method,
            url,
            headers,
            std::str::from_utf8(&body).unwrap_or_default()
        );

        let response = client
            .request(method, &url)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        log::trace!("Received response with status: {}", status);

        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();

        let body = response.bytes().await.ok();

        // Call the provided callback with the headers and response body
        callback(headers, body.map(|b| b.to_vec()));

        log::trace!("Exiting dispatch_http_request.");

        Ok(12345) // Replace with actual token or ID logic if needed
    }
}
