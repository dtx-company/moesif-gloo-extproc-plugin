use envoy_ext_proc_proto::envoy::service::ext_proc::v3::{
    external_processor_server::ExternalProcessor, processing_request, ProcessingRequest,
    ProcessingResponse,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use chrono::Utc;
use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::event::Event;
use crate::root_context::EventRootContext;
use crate::utils::*;

#[derive(Default)]
pub struct MoesifGlooExtProcGrpcService {
    config: Arc<Config>, // Store the config in the service
    event_context: Arc<Mutex<EventRootContext>>,
}

impl MoesifGlooExtProcGrpcService {
    pub fn new(config: Config) -> Result<Self, String> {
        // Set and display the log level based on the config and environment variables
        set_and_display_log_level(&config);

        // Initialize EventRootContext with the loaded configuration
        let root_context = EventRootContext::new(config.clone());

        // Create the service instance
        let service = MoesifGlooExtProcGrpcService {
            config: Arc::new(config),
            event_context: Arc::new(Mutex::new(root_context)),
        };

        // Start periodic sending in the background
        service.start_periodic_sending();

        Ok(service)
    }

    fn start_periodic_sending(&self) {
        let event_context = Arc::clone(&self.event_context);
        let batch_max_wait = Duration::from_millis(self.config.env.batch_max_wait as u64);

        log::trace!(
            "Starting periodic sending with batch_max_wait: {:?}",
            batch_max_wait
        );

        tokio::spawn(async move {
            loop {
                log::trace!("Waiting for batch_max_wait period: {:?}", batch_max_wait);
                tokio::time::sleep(batch_max_wait).await;

                log::trace!(
                    "Periodic sending triggered after waiting for: {:?}",
                    batch_max_wait
                );
                let event_context = event_context.lock().await;

                log::trace!("Draining and sending events from the main buffer...");
                event_context.drain_and_send(1).await;

                log::trace!("Periodic sending cycle complete.");
            }
        });
    }
}

#[tonic::async_trait]
impl ExternalProcessor for MoesifGlooExtProcGrpcService {
    type ProcessStream = ReceiverStream<Result<ProcessingResponse, Status>>;

    async fn process(
        &self,
        mut request: Request<Streaming<ProcessingRequest>>,
    ) -> Result<Response<Self::ProcessStream>, Status> {
        log::trace!("Processing new gRPC request...");
        let (tx, rx) = tokio::sync::mpsc::channel(32);

        let config = Arc::clone(&self.config);
        let mut request_headers_received = false;

        tokio::spawn({
            let event_context = Arc::clone(&self.event_context);
            async move {
                let mut event = Event::default(); // Event associated with this channel

                while let Some(message) = request.get_mut().next().await {
                    match message {
                        Ok(msg) => {
                            log::trace!("Received message: {:?}", msg);

                            if let Some(processing_request::Request::RequestHeaders(headers_msg)) =
                                &msg.request
                            {
                                log::trace!("Processing request headers...");
                                request_headers_received = true;
                                event.request.time = Utc::now().to_rfc3339();
                                log::trace!("Generated request time: {}", event.request.time);

                                process_request_headers(&config, &mut event, headers_msg).await;
                            }

                            if let Some(processing_request::Request::ResponseHeaders(
                                response_headers_msg,
                            )) = &msg.request
                            {
                                log::trace!("Processing response headers...");
                                process_response_headers(&mut event, response_headers_msg).await;

                                if request_headers_received {
                                    log::trace!(
                                        "Storing event after matching request and response."
                                    );
                                } else {
                                    log::warn!(
                                        "Received response without a corresponding request. Storing unmatched response."
                                    );
                                }
                                store_and_flush_event(&event_context, &event).await;
                            }

                            log::trace!("Sending simplified gRPC response with no headers");
                            send_grpc_response(tx.clone(), simplified_grpc_response()).await;
                        }

                        Err(e) => {
                            log::error!("Error receiving message: {:?}", e);
                            if tx
                                .send(Err(Status::internal("Error processing request")))
                                .await
                                .is_err()
                            {
                                log::error!("Error sending internal error response: {:?}", e);
                                break;
                            }
                        }
                    }
                }

                // Final processing when the gRPC stream closes
                if !request_headers_received {
                    log::warn!(
                        "Channel closed before receiving a matching request/response. Storing unmatched event."
                    );
                    store_and_flush_event(&event_context, &event).await;
                }
                log::trace!("Stream processing complete.");
            }
        });

        log::trace!("Returning gRPC response stream.");
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
