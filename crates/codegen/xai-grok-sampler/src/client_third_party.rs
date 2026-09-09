//! FORK(byok): third-party (BYOK) payload compat lives here, not in `client.rs`.
//! `client.rs` keeps only the `byok_compat` flag derivation plus call sites,
//! so upstream refactors of the request pipeline rarely conflict with the fork.
//! First-party requests never enter these helpers.

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
