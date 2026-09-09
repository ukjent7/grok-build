//! FORK(byok): third-party (BYOK) payload compat lives here, not in `client.rs`.
//! `client.rs` keeps only the `byok_compat` flag derivation plus call sites,
//! so upstream refactors of the request pipeline rarely conflict with the fork.
//! First-party requests never enter these helpers.
//! NOTE: this file is covered by BYOK CI (`cargo test -p xai-grok-sampler`).

use xai_grok_sampling_types::{
    ChatCompletionChunk, ChatCompletionResponse, Result, SamplingError, rs,
};

/// Drop hosted-tool entries a strict third-party Responses endpoint rejects.
/// `web_search` is standard OpenAI; `x_search` is xAI-only and 400s elsewhere as an
/// unknown tool `type`. Only applied in BYOK mode; first-party keeps everything.
pub(crate) fn retain_byok_hosted_tool_entries(entries: &mut Vec<serde_json::Value>) {
    entries.retain(|t| t.get("type").and_then(|t| t.as_str()) != Some("x_search"));
}

/// Drop xAI-proprietary Responses keys a strict third-party implementation rejects.
/// `reasoning` without an effort is an xAI default (`effort: null` 400s elsewhere);
/// `prompt_cache_key` only warms the xAI prefix cache. Runs after the other
/// post-serialization patches, so it sees the final body.
pub(crate) fn strip_byok_response_extensions(body: &mut serde_json::Value) {
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    let null_effort = obj
        .get("reasoning")
        .and_then(|v| v.get("effort"))
        .is_some_and(serde_json::Value::is_null);
    if null_effort {
        obj.remove("reasoning");
    }
    obj.remove("prompt_cache_key");
}

/// Flatten Chat Completions `messages[].content` block arrays a strict
/// third-party validator rejects with `invalid_request_error ... Input should be
/// a valid string`. Observed through the OpenCode zen relay's "Console Go" GLM
/// upstream, whose relay forwards list-type content to a model that only
/// accepts plain strings (same relay behavior as anomalyco/opencode#32613 and
/// #32821: tool-role `content` must be a string, not a ContentPart array).
///
/// Rules, applied only in BYOK mode:
/// - `tool` messages: content becomes the joined text of its text parts. These
///   providers have no tool-role image representation, so image parts are
///   replaced by an omission note instead of being silently dropped.
/// - any message whose blocks are all text is joined into one string.
/// - a `user` message mixing image and text parts keeps its block array: those
///   providers accept image blocks for vision, and flattening would silently
///   disable image input.
pub(crate) fn flatten_byok_chat_message_content(body: &mut serde_json::Value) {
    let Some(messages) = body.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for message in messages.iter_mut() {
        let Some(message) = message.as_object_mut() else {
            continue;
        };
        let is_tool = message.get("role").and_then(|v| v.as_str()) == Some("tool");
        let replacement: Option<String> = match message.get("content").and_then(|v| v.as_array()) {
            Some(parts) => {
                let mut text_parts: Vec<&str> = Vec::new();
                let mut image_count = 0usize;
                for part in parts {
                    match part.get("type").and_then(|v| v.as_str()) {
                        Some("text") => {
                            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                text_parts.push(text);
                            }
                        }
                        Some("image_url") => image_count += 1,
                        _ => {}
                    }
                }
                // Image-bearing user content stays a block array (vision input);
                // everything else is flattened to the plain string these providers require.
                if !is_tool && image_count > 0 {
                    None
                } else {
                    let mut joined = text_parts.join("\n");
                    if is_tool && image_count > 0 {
                        if !joined.is_empty() {
                            joined.push('\n');
                        }
                        joined.push_str(&format!(
                            "[{image_count} image(s) from this tool result were omitted: this provider accepts only plain-string tool output.]"
                        ));
                    }
                    Some(joined)
                }
            }
            None => None,
        };
        if let Some(joined) = replacement {
            message.insert("content".to_string(), serde_json::Value::String(joined));
        }
    }
}

/// Backfill `usage` detail objects a standard third-party gateway omits.
/// The fork's `ResponseUsage` requires `input_tokens_details` /
/// `output_tokens_details`, but they are only token breakdowns — a missing
/// object means zero, not a broken response.
pub(crate) fn backfill_usage_details(value: &mut serde_json::Value) {
    for pointer in ["/response/usage", "/usage"] {
        if let Some(usage) = value.pointer_mut(pointer).and_then(|v| v.as_object_mut()) {
            usage
                .entry("input_tokens_details")
                .or_insert_with(|| serde_json::json!({ "cached_tokens": 0 }));
            usage
                .entry("output_tokens_details")
                .or_insert_with(|| serde_json::json!({ "reasoning_tokens": 0 }));
        }
    }
}

/// Deserialize a unary Responses body, tolerating standard third-party shapes
/// the fork's types are stricter than (see `backfill_usage_details`).
pub(crate) fn deserialize_response_body(bytes: &[u8]) -> Result<rs::Response> {
    match serde_json::from_slice::<rs::Response>(bytes) {
        Ok(obj) => Ok(obj),
        Err(first_err) => {
            let raw_body = String::from_utf8_lossy(bytes);
            tracing::error!(
                error = %first_err,
                raw_body = %raw_body,
                "Failed to deserialize rs::Response"
            );
            let mut value: serde_json::Value = serde_json::from_slice(bytes)
                .map_err(|_| SamplingError::Serialization(first_err))?;
            backfill_usage_details(&mut value);
            serde_json::from_value::<rs::Response>(value).map_err(|retry_err| {
                tracing::error!(
                    error = %retry_err,
                    raw_body = %raw_body,
                    "Failed to deserialize rs::Response after sanitize"
                );
                SamplingError::Serialization(retry_err)
            })
        }
    }
}

/// Outcome of screening a raw Messages SSE payload before typed deserialization.
#[derive(Debug)]
pub(crate) enum ScreenedMessagePayload {
    /// Hand the payload to the strict `MessageStreamEvent` parse.
    Parse,
    /// Keep-alive / placeholder frame with no content: skip it without touching the stream.
    Skip,
    /// The upstream reported an error in a frame the tagged enum can never parse
    /// (e.g. opencode zen/go's `event: error` + `data: {}`). Surface it as a retryable
    /// `StreamError` instead of the turn-killing, non-retryable `Serialization` error.
    Error(SamplingError),
}

/// Screen a raw Anthropic Messages SSE payload before typed deserialization.
///
/// The `MessageStreamEvent` enum is `#[serde(tag = "type")]`, so any data payload
/// without a usable string `type` fails with `missing field 'type'`, which the client
/// classifies as a non-retryable Serialization error and kills the whole turn.
/// Third-party gateways do send such frames: opencode zen/go reports stream errors as
/// `event: error` + `data: {}`, and relays inject typeless `{}` placeholder heartbeats.
/// The old grok-gateway-proxy repaired exactly these frames with the same policy.
/// Unknown-type and non-JSON payloads stay on the strict path so a real protocol
/// break remains loud rather than being silently guessed away.
pub(crate) fn screen_message_payload(event_name: &str, data: &str) -> ScreenedMessagePayload {
    let trimmed = data.trim();
    // An empty payload fails the strict parse with a fatal EOF error and can never carry content.
    if trimmed.is_empty() {
        tracing::debug!(backend = "messages", event = %event_name, "skipping empty Messages SSE payload");
        return ScreenedMessagePayload::Skip;
    }
    let parsed = serde_json::from_str::<serde_json::Value>(trimmed).ok();
    let Some(members) = parsed.as_ref().and_then(|v| v.as_object()) else {
        // Not a JSON object (or invalid JSON): keep the strict path. Outside a `ping`
        // frame nothing meaningful lives here, and the strict parse stays loud.
        if event_name.eq_ignore_ascii_case("ping") {
            tracing::debug!(backend = "messages", raw_data = %trimmed, "skipping non-JSON ping frame");
            return ScreenedMessagePayload::Skip;
        }
        return ScreenedMessagePayload::Parse;
    };
    // The enum tag must be a non-empty string: null/number/empty tags fail the strict parse identically.
    if members
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|t| !t.is_empty())
    {
        return ScreenedMessagePayload::Parse;
    }
    if event_name.eq_ignore_ascii_case("ping") {
        tracing::debug!(backend = "messages", raw_data = %trimmed, "skipping typeless ping frame");
        return ScreenedMessagePayload::Skip;
    }
    if members.is_empty() {
        if event_name.eq_ignore_ascii_case("error") {
            tracing::warn!(
                backend = "messages",
                raw_data = %trimmed,
                "upstream sent an empty error event"
            );
            return ScreenedMessagePayload::Error(SamplingError::StreamError {
                error_type: "api_error".into(),
                message: "upstream sent an empty error event".into(),
                code: None,
            });
        }
        // An empty object on a non-error frame is most likely a placeholder heartbeat:
        // drop it rather than fabricate an error that interrupts a healthy stream.
        tracing::debug!(backend = "messages", event = %event_name, "skipping typeless empty Messages SSE event");
        return ScreenedMessagePayload::Skip;
    }
    tracing::warn!(
        backend = "messages",
        raw_data = %trimmed,
        "upstream sent a stream event without a type field"
    );
    ScreenedMessagePayload::Error(SamplingError::StreamError {
        error_type: "api_error".into(),
        message: "upstream sent a stream event without a type field".into(),
        code: None,
    })
}

/// Some third-party chat gateways emit `"finish_reason": ""` instead of `null`.
/// Empty string carries no information (pi treats it as absent too), so null it
/// before typed deserialization instead of failing the whole turn.
/// Any other unknown reason stays loud: its semantics can't be guessed
/// (e.g. mistaking a truncation marker for `stop` would silently cut the turn).
pub(crate) fn null_out_empty_finish_reasons(value: &mut serde_json::Value) {
    if let Some(choices) = value.get_mut("choices").and_then(|v| v.as_array_mut()) {
        for choice in choices {
            if choice.get("finish_reason").and_then(|v| v.as_str()) == Some("") {
                choice["finish_reason"] = serde_json::Value::Null;
            }
        }
    }
}

/// Deserialize a Chat stream chunk, tolerating `"finish_reason": ""`.
pub(crate) fn deserialize_chat_chunk(data: &str) -> Result<ChatCompletionChunk> {
    match serde_json::from_str::<ChatCompletionChunk>(data) {
        Ok(chunk) => Ok(chunk),
        Err(first_err) => {
            if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(data) {
                null_out_empty_finish_reasons(&mut value);
                if let Ok(chunk) = serde_json::from_value::<ChatCompletionChunk>(value) {
                    return Ok(chunk);
                }
            }
            tracing::error!(
                error = %first_err,
                raw_data = %data,
                "Failed to deserialize ChatCompletionChunk from stream"
            );
            Err(SamplingError::Serialization(first_err))
        }
    }
}

/// Deserialize a unary Chat body, tolerating `"finish_reason": ""`.
pub(crate) fn deserialize_chat_response(bytes: &[u8]) -> Result<ChatCompletionResponse> {
    match serde_json::from_slice::<ChatCompletionResponse>(bytes) {
        Ok(obj) => Ok(obj),
        Err(first_err) => {
            let raw_body = String::from_utf8_lossy(bytes);
            tracing::error!(
                error = %first_err,
                raw_body = %raw_body,
                "Failed to deserialize ChatCompletionResponse"
            );
            let mut value: serde_json::Value = serde_json::from_slice(bytes)
                .map_err(|_| SamplingError::Serialization(first_err))?;
            null_out_empty_finish_reasons(&mut value);
            serde_json::from_value::<ChatCompletionResponse>(value).map_err(|retry_err| {
                tracing::error!(
                    error = %retry_err,
                    raw_body = %raw_body,
                    "Failed to deserialize ChatCompletionResponse after sanitize"
                );
                SamplingError::Serialization(retry_err)
            })
        }
    }
}

/// Keep-alive / extension events a third-party Responses implementation may inject
/// (e.g. `ping` from gateways/proxies during long reasoning turns).
/// Unknown to async-openai's typed event enum, so skip them instead of failing the stream.
/// Anything else still goes through strict deserialization, so a real protocol break stays loud.
pub(crate) fn is_ignorable_response_event(event_name: &str, data: &str) -> bool {
    if event_name == "ping" {
        return true;
    }
    // Some gateways put the discriminator only in the payload.
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
        if value.get("type").and_then(|t| t.as_str()) == Some("ping") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use xai_grok_sampling_types::messages::MessageStreamEvent;

    #[test]
    fn tool_block_content_flattens_to_string_with_image_note() {
        let mut body = serde_json::json!({
            "model": "glm-5.3-flash",
            "messages": [
                {"role": "user", "content": "hi"},
                {
                    "role": "tool",
                    "tool_call_id": "call-1",
                    "content": [
                        {"type": "text", "text": "file contents"},
                        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
                        {"type": "image_url", "image_url": {"url": "data:image/png;base64,BBBB"}}
                    ]
                }
            ]
        });
        flatten_byok_chat_message_content(&mut body);
        let content = body["messages"][1]["content"].as_str().unwrap();
        assert!(content.starts_with("file contents\n"));
        assert!(content.contains("[2 image(s) from this tool result were omitted"));
        // The upstream rejects list content; nothing may stay an array.
        assert_eq!(body["messages"][1]["tool_call_id"], "call-1");
    }

    #[test]
    fn text_only_block_content_flattens_for_every_role() {
        let mut body = serde_json::json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "part one"},
                    {"type": "text", "text": "part two"}
                ]},
                {"role": "assistant", "content": [{"type": "text", "text": "done"}]}
            ]
        });
        flatten_byok_chat_message_content(&mut body);
        assert_eq!(body["messages"][0]["content"], "part one\npart two");
        assert_eq!(body["messages"][1]["content"], "done");
    }

    #[test]
    fn image_bearing_user_content_keeps_its_block_array() {
        let mut body = serde_json::json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "look at this"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
                ]}
            ]
        });
        let before = body.clone();
        flatten_byok_chat_message_content(&mut body);
        assert_eq!(body, before);
    }

    #[test]
    fn string_content_and_missing_messages_are_untouched() {
        let mut body = serde_json::json!({
            "messages": [
                {"role": "tool", "tool_call_id": "call-1", "content": "plain string"},
                {"role": "user", "content": "also plain"}
            ]
        });
        let before = body.clone();
        flatten_byok_chat_message_content(&mut body);
        assert_eq!(body, before);

        let mut no_messages = serde_json::json!({"model": "glm-5.3-flash"});
        flatten_byok_chat_message_content(&mut no_messages);
        assert_eq!(no_messages, serde_json::json!({"model": "glm-5.3-flash"}));
    }

    /// Documents the exact failure this screener exists for: the tagged enum needs a
    /// string `type`, so an empty object fails non-retryably at the payload's end.
    #[test]
    fn empty_object_fails_typed_parse_with_the_reported_error() {
        let err = serde_json::from_str::<MessageStreamEvent>("{}").unwrap_err();
        assert_eq!(err.to_string(), "missing field `type` at line 1 column 2");
        assert!(!SamplingError::from(err).is_retryable());
    }

    fn assert_retryable_error(screened: ScreenedMessagePayload, expected_message: &str) {
        match screened {
            ScreenedMessagePayload::Error(err) => {
                assert!(err.is_retryable(), "the repaired error must stay retryable");
                let SamplingError::StreamError {
                    error_type,
                    message,
                    code,
                } = err
                else {
                    panic!("expected StreamError, got {err:?}");
                };
                assert_eq!(error_type, "api_error");
                assert_eq!(message, expected_message);
                assert_eq!(code, None);
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    /// opencode zen/go reports stream errors as `event: error` + `data: {}`; the old
    /// gateway rewrote this into a legal error event so the client retries.
    #[test]
    fn empty_object_on_error_event_becomes_retryable_stream_error() {
        assert_retryable_error(
            screen_message_payload("error", "{}"),
            "upstream sent an empty error event",
        );
        assert_retryable_error(
            screen_message_payload("ERROR", "  {}  "),
            "upstream sent an empty error event",
        );
    }

    #[test]
    fn typeless_payload_with_members_becomes_retryable_stream_error() {
        assert_retryable_error(
            screen_message_payload("", r#"{"message":"boom"}"#),
            "upstream sent a stream event without a type field",
        );
        // The tag must be a usable string: null/number/empty fail the strict parse too.
        assert_retryable_error(
            screen_message_payload("", r#"{"type":null,"index":0}"#),
            "upstream sent a stream event without a type field",
        );
        assert_retryable_error(
            screen_message_payload("", r#"{"type":""}"#),
            "upstream sent a stream event without a type field",
        );
    }

    #[test]
    fn empty_and_placeholder_payloads_are_skipped() {
        assert!(matches!(
            screen_message_payload("", ""),
            ScreenedMessagePayload::Skip
        ));
        assert!(matches!(
            screen_message_payload("ping", " \r\n"),
            ScreenedMessagePayload::Skip
        ));
        // Placeholder heartbeats: empty object on a frame the upstream did not mark as error.
        assert!(matches!(
            screen_message_payload("ping", "{}"),
            ScreenedMessagePayload::Skip
        ));
        assert!(matches!(
            screen_message_payload("keepalive", "{}"),
            ScreenedMessagePayload::Skip
        ));
    }

    #[test]
    fn typed_events_pass_through_to_the_strict_parse() {
        for data in [
            r#"{"type":"ping"}"#,
            r#"{"type":"message_start","message":{}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"slow down"}}"#,
        ] {
            assert!(matches!(
                screen_message_payload("content_block_delta", data),
                ScreenedMessagePayload::Parse
            ));
        }
    }

    #[test]
    fn non_object_and_invalid_payloads_stay_strict_outside_ping() {
        // A real protocol break must stay loud, not be guessed away.
        assert!(matches!(
            screen_message_payload("", "[1,2]"),
            ScreenedMessagePayload::Parse
        ));
        assert!(matches!(
            screen_message_payload("", "{oops"),
            ScreenedMessagePayload::Parse
        ));
    }

    #[test]
    fn ping_frames_with_unparseable_payloads_are_skipped() {
        // Keep-alives carry no content; never let one kill the stream.
        assert!(matches!(
            screen_message_payload("ping", "alive"),
            ScreenedMessagePayload::Skip
        ));
        assert!(matches!(
            screen_message_payload("ping", r#"{"error":"x"}"#),
            ScreenedMessagePayload::Skip
        ));
    }
}
