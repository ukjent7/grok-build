//! FORK(byok): third-party (BYOK) payload compat lives here, not in `client.rs`.
//! `client.rs` keeps only the `byok_compat` flag derivation plus call sites,
//! so upstream refactors of the request pipeline rarely conflict with the fork.
//!
//! Two unrelated concerns share this file, and only the first is BYOK-only:
//!
//! * Request rewrites a strict third-party gateway rejects -- `normalize_byok_chat_
//!   message_content`, `strip_byok_response_extensions`, `retain_byok_hosted_tool_
//!   entries`. Every one of these is called behind `byok_compat`.
//! * Tolerant response parsing, which is **not** gated and runs on first-party
//!   traffic too: the `deserialize_*` entry points, `apply_terminal_event_overrides`,
//!   `backfill_usage_details`, `coerce_integral_floats_to_ints` and
//!   `is_ignorable_response_event`. A frame that fails the `#[serde(tag = "type")]`
//!   event enum, or that carries a float where an int is declared, is a non-retryable
//!   turn-ending error on any endpoint, so leniency there is not a BYOK feature.
//!
//! First-party requests therefore do enter this file, through the recovery path of
//! `client.rs::deserialize_response_event`. Read an ungated call as intended
//! leniency, not as a missed flag.
//! NOTE: this file is covered by BYOK CI (`cargo test -p xai-grok-sampler`).

use xai_grok_sampling_types::{
    ChatCompletionChunk, ChatCompletionResponse, Result, SamplingError, rs, truncate_bytes,
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

/// Caption of the user message that carries images relocated out of tool results.
const TOOL_IMAGE_CAPTION: &str = "Attached image(s) from tool result:";

/// FORK(byok): normalize Chat Completions message shapes for third-party
/// validators. OpenAI-compatible relays route a stable base URL to upstreams
/// that may type `messages[].content` as a plain string and reject block
/// arrays with `invalid_request_error ... Input should be a valid string`
/// (GLM via the OpenCode zen relay — anomalyco/opencode#32821, #32613; DeepSeek
/// V3.2 via NVIDIA NIM; Xiaomi MiMo — pi-mono openai-completions conventions).
/// The routing can change under a stable base URL, so the standard shapes are
/// emitted unconditionally instead of relying on any one relay's tolerance:
///
/// - `tool` messages: content becomes the joined text of its text parts
///   (`(see attached image)` when only images remain, `(no tool output)`
///   when empty).
/// - images taken out of tool messages are NOT dropped: they are re-attached
///   as a following `user` message so a vision-capable model still sees what
///   its tools produced.
/// - text-only block arrays on other messages are joined into one string;
///   image-bearing messages keep their block array.
pub(crate) fn normalize_byok_chat_message_content(body: &mut serde_json::Value) {
    fn flush_pending_images(
        out: &mut Vec<serde_json::Value>,
        pending: &mut Vec<serde_json::Value>,
    ) {
        if pending.is_empty() {
            return;
        }
        let mut content = vec![serde_json::json!({ "type": "text", "text": TOOL_IMAGE_CAPTION })];
        content.append(pending);
        out.push(serde_json::json!({ "role": "user", "content": content }));
    }

    let Some(messages) = body.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return;
    };
    let original = std::mem::take(messages);
    let mut out: Vec<serde_json::Value> = Vec::with_capacity(original.len() + 1);
    let mut pending_images: Vec<serde_json::Value> = Vec::new();

    for msg in original {
        let is_tool = msg.get("role").and_then(|v| v.as_str()) == Some("tool");
        // Non-tool messages: text-only block arrays join into a plain string;
        // image-bearing messages keep their block array.
        let text_only_join: Option<String> = if is_tool {
            None
        } else if let Some(parts) = msg.get("content").and_then(|v| v.as_array()) {
            let mut has_image = false;
            let mut texts: Vec<&str> = Vec::new();
            for part in parts {
                match part.get("type").and_then(|v| v.as_str()) {
                    Some("text") => {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            texts.push(text);
                        }
                    }
                    Some("image_url") => has_image = true,
                    _ => {}
                }
            }
            (!has_image).then(|| texts.join("\n"))
        } else {
            None
        };

        let mut msg = msg;
        if let Some(joined) = text_only_join {
            if let Some(obj) = msg.as_object_mut() {
                obj.insert("content".to_string(), serde_json::Value::String(joined));
            }
        }

        if is_tool {
            if let Some(obj) = msg.as_object_mut() {
                if let Some(content) = obj.remove("content") {
                    match content {
                        serde_json::Value::Array(parts) => {
                            let mut texts: Vec<String> = Vec::new();
                            let mut image_count = 0usize;
                            for part in parts {
                                match part.get("type").and_then(|v| v.as_str()) {
                                    Some("text") => {
                                        if let Some(text) =
                                            part.get("text").and_then(|v| v.as_str())
                                        {
                                            texts.push(text.to_owned());
                                        }
                                    }
                                    Some("image_url") => {
                                        image_count += 1;
                                        pending_images.push(part);
                                    }
                                    _ => {}
                                }
                            }
                            let joined = texts.join("\n");
                            let content = if !joined.is_empty() {
                                joined
                            } else if image_count > 0 {
                                "(see attached image)".to_string()
                            } else {
                                "(no tool output)".to_string()
                            };
                            obj.insert(
                                "content".to_string(),
                                serde_json::Value::String(content),
                            );
                        }
                        // String-content tool messages pass through unchanged.
                        other => {
                            obj.insert("content".to_string(), other);
                        }
                    }
                }
            }
            out.push(msg);
            continue;
        }

        flush_pending_images(&mut out, &mut pending_images);
        out.push(msg);
    }
    // A conversation may end on its tool results; the relocated images still go out.
    flush_pending_images(&mut out, &mut pending_images);
    *messages = out;
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

/// Rewrite integral floats (`1789666913.0`) as JSON integers, in place.
///
/// A gateway that builds a Unix timestamp in floating point (Python's
/// `time.time()`, a JS `Date.now() / 1000`) serializes it as `1789666913.0`.
/// serde_json routes that literal through its `f64` visitor, so every `u64` field
/// rejects the whole payload as a floating point where an integer was expected --
/// including fields the client never reads, such as Responses `created_at`. That
/// turns a healthy stream into a terminal, non-retryable `Serialization` error.
///
/// Every integral float within +/-2^53 is exactly representable as an integer, and
/// serde's float visitors accept integer input, so the rewrite is lossless for
/// every type such a payload can declare. Fractional, non-finite and out-of-range
/// numbers stay untouched.
pub(crate) fn coerce_integral_floats_to_ints(value: &mut serde_json::Value) {
    // 2^53: the largest magnitude where `f64` still holds every integer exactly.
    const EXACT_F64_INT_MAX: f64 = 9_007_199_254_740_992.0;

    match value {
        serde_json::Value::Number(number) => {
            if let Some(float) = number.as_f64() {
                if float.fract() == 0.0 && float.abs() <= EXACT_F64_INT_MAX {
                    *number = serde_json::Number::from(float as i64);
                }
            }
        }
        serde_json::Value::Array(items) => {
            items.iter_mut().for_each(coerce_integral_floats_to_ints);
        }
        serde_json::Value::Object(members) => {
            members.values_mut().for_each(coerce_integral_floats_to_ints);
        }
        _ => {}
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
            coerce_integral_floats_to_ints(&mut value);
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
    // Keep the raw frame in the surfaced error too: tracing output only helps
    // with debug logging enabled, and this frame is the only diagnostic a
    // misbehaving third-party gateway leaves behind.
    ScreenedMessagePayload::Error(SamplingError::StreamError {
        error_type: "api_error".into(),
        message: format!(
            "upstream sent a stream event without a type field: {}",
            truncate_bytes(trimmed, 200)
        ),
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
                coerce_integral_floats_to_ints(&mut value);
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
            coerce_integral_floats_to_ints(&mut value);
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
    fn tool_block_content_moves_images_to_a_following_user_message() {
        let mut body = serde_json::json!({
            "messages": [
                {"role": "user", "content": "read the screenshot"},
                {"role": "assistant", "content": "", "tool_calls": [
                    {"id": "call-1", "type": "function", "function": {"name": "read_file", "arguments": "{}"}}
                ]},
                {"role": "tool", "tool_call_id": "call-1", "content": [
                    {"type": "text", "text": "file contents"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
                ]},
                {"role": "assistant", "content": "here is what I saw"}
            ]
        });
        normalize_byok_chat_message_content(&mut body);
        assert_eq!(body["messages"][2]["content"], "file contents");
        assert_eq!(body["messages"][3]["role"], "user");
        let parts = body["messages"][3]["content"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], "Attached image(s) from tool result:");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(body["messages"][4]["role"], "assistant");
    }

    #[test]
    fn image_only_tool_result_keeps_caption_text_and_relocates() {
        let mut body = serde_json::json!({
            "messages": [
                {"role": "tool", "tool_call_id": "call-1", "content": [
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
                ]}
            ]
        });
        normalize_byok_chat_message_content(&mut body);
        assert_eq!(body["messages"][0]["content"], "(see attached image)");
        let parts = body["messages"][1]["content"].as_array().unwrap();
        assert_eq!(parts[0]["text"], "Attached image(s) from tool result:");
        assert_eq!(parts[1]["type"], "image_url");
    }

    #[test]
    fn string_tool_results_and_image_bearing_user_messages_are_untouched() {
        let mut body = serde_json::json!({
            "messages": [
                {"role": "tool", "tool_call_id": "call-1", "content": "plain result"},
                {"role": "user", "content": [
                    {"type": "text", "text": "look at this"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
                ]}
            ]
        });
        let before = body.clone();
        normalize_byok_chat_message_content(&mut body);
        assert_eq!(body, before);
    }

    #[test]
    fn text_only_arrays_flatten_and_missing_messages_is_noop() {
        let mut body = serde_json::json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "part one"},
                    {"type": "text", "text": "part two"}
                ]},
                {"role": "tool", "tool_call_id": "call-2", "content": []}
            ]
        });
        normalize_byok_chat_message_content(&mut body);
        assert_eq!(body["messages"][0]["content"], "part one\npart two");
        assert_eq!(body["messages"][1]["content"], "(no tool output)");

        let mut no_messages = serde_json::json!({"model": "glm-5.3-flash"});
        normalize_byok_chat_message_content(&mut no_messages);
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
            r#"upstream sent a stream event without a type field: {"message":"boom"}"#,
        );
        // The tag must be a usable string: null/number/empty fail the strict parse too.
        assert_retryable_error(
            screen_message_payload("", r#"{"type":null,"index":0}"#),
            r#"upstream sent a stream event without a type field: {"type":null,"index":0}"#,
        );
        assert_retryable_error(
            screen_message_payload("", r#"{"type":""}"#),
            r#"upstream sent a stream event without a type field: {"type":""}"#,
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

    #[test]
    fn integral_floats_become_integers_and_other_numbers_are_untouched() {
        let mut value = serde_json::json!({
            "created_at": 1789666913.0,
            "items": [{ "duration": 2.0 }, { "duration": 0.5 }],
            "huge": u64::MAX,
            "text": "1789666913.0",
        });
        coerce_integral_floats_to_ints(&mut value);

        assert_eq!(value["created_at"].as_u64(), Some(1_789_666_913));
        assert_eq!(value["items"][0]["duration"].as_u64(), Some(2));
        // Fractional, out-of-f64-exact-range and non-number values keep their shape.
        assert!(value["items"][1]["duration"].is_f64());
        assert_eq!(value["huge"].as_u64(), Some(u64::MAX));
        assert!(value["text"].is_string());
        // serde's float visitors accept integer input, so a rewritten `f64` field still loads.
        let duration: f64 = serde_json::from_value(value["items"][0]["duration"].clone()).unwrap();
        assert_eq!(duration, 2.0);
    }

    #[test]
    fn float_timestamps_no_longer_kill_responses_events() {
        // Replayed from a real gateway frame (`codex.wzyfromhust.de`, `deepseek-v4.1-flash`):
        // it serialized `created_at` out of a float, so the strict parse failed on a field
        // the client never reads and every turn ended as a non-retryable Serialization error.
        let created = r#"{
            "type": "response.created",
            "sequence_number": 0,
            "response": {
                "id": "resp_944c9f15ab5f4ec291feb4a67dee8ed6",
                "object": "response",
                "created_at": 1789666913.0,
                "model": "deepseek-v4.1-flash",
                "status": "in_progress",
                "output": []
            }
        }"#;
        let event = crate::client::deserialize_response_event(created).expect("created parses");
        let rs::ResponseStreamEvent::ResponseCreated(e) = event else {
            panic!("expected ResponseCreated");
        };
        assert_eq!(e.response.created_at, 1_789_666_913);

        let completed = r#"{
            "type": "response.completed",
            "sequence_number": 1,
            "response": {
                "id": "resp_944c9f15ab5f4ec291feb4a67dee8ed6",
                "object": "response",
                "created_at": 1789666913.0,
                "completed_at": 1789666914.0,
                "model": "deepseek-v4.1-flash",
                "status": "completed",
                "output": [],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 5,
                    "total_tokens": 15
                }
            }
        }"#;
        let event = crate::client::deserialize_response_event(completed).expect("completed parses");
        let rs::ResponseStreamEvent::ResponseCompleted(e) = event else {
            panic!("expected ResponseCompleted");
        };
        assert_eq!(e.response.created_at, 1_789_666_913);
        assert_eq!(e.response.completed_at, Some(1_789_666_914));
    }
}
