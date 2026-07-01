use rumqttc::{ConnectionError, Event, Incoming};
use tokio::time::{sleep, Duration};

/// Drives the rumqttc event loop in a background tokio task.
///
/// Logs incoming publish events at `DEBUG` level and connection / disconnection
/// events at `INFO` level. On error, waits 500 ms and continues — rumqttc
/// handles the actual reconnect internally.
///
/// The returned `JoinHandle` runs until the associated `AsyncClient` (and
/// therefore the event loop) is dropped.
pub fn spawn_event_loop(mut eventloop: rumqttc::EventLoop) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(event) => handle_event(event),
                Err(e) => {
                    handle_error(e);
                    // Brief pause before the next poll so we don't spin-burn
                    // CPU while rumqttc is waiting to reconnect.
                    sleep(Duration::from_millis(500)).await;
                }
            }
        }
    })
}

fn handle_event(event: Event) {
    match event {
        Event::Incoming(Incoming::Publish(publish)) => {
            tracing::debug!(
                topic = %publish.topic,
                qos  = ?publish.qos,
                len  = publish.payload.len(),
                "mqtt: incoming publish"
            );
        }
        Event::Incoming(Incoming::ConnAck(ack)) => {
            tracing::info!(
                session_present = ack.session_present,
                code = ?ack.code,
                "mqtt: connected"
            );
        }
        Event::Incoming(Incoming::Disconnect) => {
            tracing::info!("mqtt: disconnected by broker");
        }
        Event::Incoming(Incoming::PingResp) => {
            tracing::trace!("mqtt: ping response");
        }
        Event::Outgoing(outgoing) => {
            tracing::trace!(?outgoing, "mqtt: outgoing packet");
        }
        other => {
            tracing::debug!(?other, "mqtt: event");
        }
    }
}

fn handle_error(e: ConnectionError) {
    // Distinguish transient network errors from configuration mistakes so
    // operators can tell them apart in logs.
    match &e {
        ConnectionError::Io(io_err) => {
            tracing::warn!(error = %io_err, "mqtt: connection I/O error, will retry");
        }
        ConnectionError::MqttState(state_err) => {
            tracing::warn!(error = %state_err, "mqtt: state error, will retry");
        }
        other => {
            tracing::error!(error = %other, "mqtt: unexpected connection error, will retry");
        }
    }
}
