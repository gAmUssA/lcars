//! Newline-delimited JSON framing for the Herdr socket API (protocol 22).
//!
//! Observed behaviour (docs/herdr-api.md): one request per connection, the
//! server closes after the response; a successful `events.subscribe` keeps
//! the connection open and pushes event frames on it.

use herdr_compat::api::schema::{
    ErrorResponse, EventEnvelope, Request, SubscriptionEventEnvelope, SuccessResponse,
};
use serde_json::Value;

/// One line read from a Herdr socket.
#[derive(Debug, Clone, PartialEq)]
// Frames are decoded, dispatched and dropped immediately; boxing the event
// variants would add one allocation per pushed event for no benefit.
#[allow(clippy::large_enum_variant)]
pub enum Frame {
    /// `{"id":…,"result":{…}}`
    Response(Box<SuccessResponse>),
    /// `{"id":…,"error":{…}}`
    Error(ErrorResponse),
    /// Per-pane subscription push, `event` is dotted (`pane.agent_status_changed`).
    Subscription(SubscriptionEventEnvelope),
    /// Broadcast push, `event` is snake_case (`pane_created`).
    Broadcast(EventEnvelope),
    /// Valid JSON that matched no known shape. Kept so one unknown frame
    /// never kills a connection.
    Unknown(Value),
}

/// Parse one line. Fails only on invalid JSON or a malformed `id` frame.
pub fn parse_frame(line: &str) -> Result<Frame, serde_json::Error> {
    let value: Value = serde_json::from_str(line)?;
    if value.get("id").is_some() {
        if value.get("error").is_some() {
            return serde_json::from_value(value).map(Frame::Error);
        }
        return serde_json::from_value(value).map(|r| Frame::Response(Box::new(r)));
    }
    match value.get("event").and_then(Value::as_str) {
        Some(event) if event.contains('.') => Ok(serde_json::from_value(value.clone())
            .map(Frame::Subscription)
            .unwrap_or(Frame::Unknown(value))),
        Some(_) => Ok(serde_json::from_value(value.clone())
            .map(Frame::Broadcast)
            .unwrap_or(Frame::Unknown(value))),
        None => Ok(Frame::Unknown(value)),
    }
}

/// Encode a request as one NDJSON line, newline included.
pub fn encode_request(request: &Request) -> Result<String, serde_json::Error> {
    let mut line = serde_json::to_string(request)?;
    line.push('\n');
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_compat::api::schema::{
        EventData, Method, PingParams, ResponseResult, SubscriptionEventData,
    };

    const STATUS_FLOW: &str =
        include_str!("../../../../../../docs/fixtures/herdr-status-flow.json");
    const EDGE_CASES: &str =
        include_str!("../../../../../../docs/fixtures/herdr-subscribe-edge-cases.json");

    fn recorded_lines(fixture: &str) -> Vec<String> {
        let entries: Vec<Value> = serde_json::from_str(fixture).unwrap();
        entries
            .iter()
            .filter(|e| e["tag"] != "note")
            .filter_map(|e| e["frame"].as_str())
            .filter(|l| *l != "<EOF>")
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn every_recorded_socket_frame_parses_to_a_known_shape() {
        let mut counts = std::collections::BTreeMap::new();
        for line in recorded_lines(STATUS_FLOW)
            .into_iter()
            .chain(recorded_lines(EDGE_CASES))
        {
            let frame = parse_frame(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
            let kind = match &frame {
                Frame::Response(_) => "response",
                Frame::Error(_) => "error",
                Frame::Subscription(_) => "subscription",
                Frame::Broadcast(_) => "broadcast",
                Frame::Unknown(v) => panic!("unknown frame shape: {v}"),
            };
            *counts.entry(kind).or_insert(0) += 1;
        }
        // Counted by hand from the two recordings (CLI records, notes and
        // EOF markers excluded): 5 subscribe acks, 2 rejected subscribes,
        // 9 status pushes, 9 broadcast pushes.
        assert_eq!(counts["response"], 5);
        assert_eq!(counts["error"], 2);
        assert_eq!(counts["subscription"], 9);
        assert_eq!(counts["broadcast"], 9);
    }

    #[test]
    fn pong_frame_carries_version_and_protocol() {
        let line = r#"{"id":"lcars-ping-1","result":{"type":"pong","version":"0.9.4","protocol":22,"capabilities":{"live_handoff":true,"detached_server_daemon":true,"endpoint_protocol_generation":1,"surface_interest":true,"health_check":true,"ssh_agent_registration":true}}}"#;
        match parse_frame(line).unwrap() {
            Frame::Response(response) => {
                let SuccessResponse { id, result } = *response;
                let ResponseResult::Pong {
                    version, protocol, ..
                } = result
                else {
                    panic!("expected pong, got {result:?}")
                };
                assert_eq!(id, "lcars-ping-1");
                assert_eq!(version, "0.9.4");
                assert_eq!(protocol, 22);
            }
            other => panic!("expected pong, got {other:?}"),
        }
    }

    #[test]
    fn mini_pong_without_ssh_agent_flag_still_parses() {
        let line = r#"{"id":"ops:mini:1","result":{"type":"pong","version":"0.9.0","protocol":22,"capabilities":{"live_handoff":true,"detached_server_daemon":true,"endpoint_protocol_generation":1,"surface_interest":true,"health_check":true}}}"#;
        match parse_frame(line).unwrap() {
            Frame::Response(response) => assert!(matches!(
                response.result,
                ResponseResult::Pong { protocol: 22, .. }
            )),
            other => panic!("expected pong, got {other:?}"),
        }
    }

    #[test]
    fn missing_pane_id_error_is_an_error_frame() {
        let line = r#"{"id":"lcars-B-nofilter-status","error":{"code":"invalid_request","message":"invalid request: missing field `pane_id` at line 1 column 133"}}"#;
        match parse_frame(line).unwrap() {
            Frame::Error(err) => {
                assert_eq!(err.error.code, "invalid_request");
                assert!(err.error.message.contains("pane_id"));
            }
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn status_change_is_a_subscription_frame() {
        let line = r#"{"data":{"agent":"lcars-test","agent_status":"blocked","pane_id":"w9:p2","workspace_id":"w9"},"event":"pane.agent_status_changed"}"#;
        match parse_frame(line).unwrap() {
            Frame::Subscription(SubscriptionEventEnvelope {
                data: SubscriptionEventData::PaneAgentStatusChanged(e),
                ..
            }) => {
                assert_eq!(e.pane_id, "w9:p2");
                assert_eq!(e.agent.as_deref(), Some("lcars-test"));
            }
            other => panic!("expected status change, got {other:?}"),
        }
    }

    #[test]
    fn pane_closed_is_a_broadcast_frame() {
        let line = r#"{"data":{"pane_id":"w9:p2","type":"pane_closed","workspace_id":"w9"},"event":"pane_closed"}"#;
        match parse_frame(line).unwrap() {
            Frame::Broadcast(EventEnvelope {
                data: EventData::PaneClosed { pane_id, .. },
                ..
            }) => assert_eq!(pane_id, "w9:p2"),
            other => panic!("expected pane_closed, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_kind_is_kept_not_rejected() {
        let line = r#"{"data":{"x":1},"event":"pane_teleported"}"#;
        assert!(matches!(parse_frame(line).unwrap(), Frame::Unknown(_)));
        let line = r#"{"data":{"x":1},"event":"pane.teleported"}"#;
        assert!(matches!(parse_frame(line).unwrap(), Frame::Unknown(_)));
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_frame("{not json").is_err());
    }

    #[test]
    fn request_encodes_as_one_line_with_params() {
        let request = Request {
            id: "ops:1".into(),
            method: Method::Ping(PingParams::default()),
        };
        let line = encode_request(&request).unwrap();
        assert!(line.ends_with('\n'));
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["id"], "ops:1");
        assert_eq!(value["method"], "ping");
        assert!(
            value.get("params").is_some(),
            "params key is required by the server: {line}"
        );
    }
}
