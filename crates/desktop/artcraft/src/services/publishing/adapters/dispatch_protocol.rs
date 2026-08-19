use serde::Deserialize;
use super::publisher_adapter::{PublisherError, PublisherErrorCode};

/// Canonical response body from a Donut/Extension `POST /v1/workers/{id}/dispatch` call.
/// Follows the floword-production v1 protocol.
#[derive(Debug, Deserialize)]
pub struct ProductionDispatchResponse {
  pub protocol: Option<String>,
  #[serde(rename = "protocolVersion")]
  pub protocol_version: Option<u32>,
  #[serde(rename = "requestId")]
  pub request_id: Option<String>,
  /// True if the action was performed successfully.
  pub ok: Option<bool>,
  /// Authoritative evidence of the completed action (e.g. postId, postUrl).
  pub result: Option<serde_json::Value>,
  /// Structured error, present when ok=false.
  pub error: Option<ProductionDispatchError>,
}

/// Structured error nested inside the response body when ok=false.
#[derive(Debug, Deserialize)]
pub struct ProductionDispatchError {
  pub code: Option<String>,
  pub message: Option<String>,
  /// Extension-reported retryability hint. Only trusted for non-outcome-uncertain errors.
  pub retryable: Option<bool>,
}

/// Map a known runtime error code / message to the correct `PublisherError`.
///
/// Rules:
/// - Prefer `err.code` for taxonomy; supplement with `err.message`.
/// - AUTH_REQUIRED / LOGIN_REQUIRED → `AuthRequired` (not retryable)
/// - CAPABILITY_UNAVAILABLE / PROTOCOL_MISMATCH / METHOD_NOT_SUPPORTED → `CapabilityUnavailable` (not retryable)
/// - TARGET_AMBIGUOUS → `TargetAmbiguous` (not retryable)
/// - BRIDGE_TIMEOUT / network timeouts after possible dispatch → `VerifyFailed` (not retryable; outcome unknown)
/// - Unknown code: `UploadFailed` — respect extension's `retryable` flag.
pub fn map_dispatch_error(err: &ProductionDispatchError, platform_label: &str) -> PublisherError {
  let code = err.code.as_deref().unwrap_or("UNKNOWN");
  let message = err.message.as_deref().unwrap_or("Extension returned an error with no message");
  let retryable = err.retryable.unwrap_or(false);

  if code.contains("AUTH") || code.contains("LOGIN") || code.contains("GROK_NOT_LOGGED_IN")
    || message.contains("login") || message.contains("AUTH_REQUIRED")
  {
    PublisherError::auth_required(format!("[{platform_label}] {code}: {message}"))
  } else if code.contains("CAPABILITY") || code.contains("PROTOCOL_MISMATCH")
    || code.contains("METHOD_NOT_SUPPORTED") || code.contains("METHOD_UNSUPPORTED")
    || message.contains("not supported") || message.contains("not implemented")
  {
    PublisherError::new(
      PublisherErrorCode::CapabilityUnavailable,
      format!("[{platform_label}] {code}: {message}"),
      false,
    )
  } else if code.contains("TARGET_AMBIGUOUS") {
    PublisherError::target_ambiguous(format!("[{platform_label}] {code}: {message}"))
  } else if code.contains("BRIDGE_TIMEOUT") || code.contains("NETWORK_TIMEOUT")
    || message.contains("timed out after")
  {
    PublisherError::new(
      PublisherErrorCode::VerifyFailed,
      format!("[{platform_label}] {code}: {message} — outcome unknown, verification required"),
      false,
    )
  } else if code.contains("PROFILE_OFFLINE") || code.contains("NO_AVAILABLE_WORKER")
    || code.contains("INVALID_PROFILE")
  {
    PublisherError::profile_offline(format!("[{platform_label}] {code}: {message}"))
  } else if code.contains("WORKER_BUSY") {
    PublisherError::upload_failed(format!("[{platform_label}] {code}: {message}"), true)
  } else {
    PublisherError::upload_failed(
      format!("[{platform_label}] {code}: {message}"),
      retryable,
    )
  }
}

/// Parse the raw JSON response body from a 2xx dispatch call.
///
/// Returns:
/// - `Ok(response)` with the typed struct on success.
/// - `Err(PublisherError)` if JSON fails to parse (protocol/invalid-response error).
pub fn parse_dispatch_body(
  raw: serde_json::Value,
  platform_label: &str,
) -> Result<ProductionDispatchResponse, PublisherError> {
  serde_json::from_value::<ProductionDispatchResponse>(raw.clone()).map_err(|e| {
    PublisherError::new(
      PublisherErrorCode::PlatformRejected,
      format!("[{platform_label}] Failed to parse dispatch response (protocol violation): {e}"),
      false,
    )
  })
}

/// Full semantic validation of a parsed dispatch response.
///
/// Called after `parse_dispatch_body`. Returns the `result` field on success, or
/// a correctly classified `PublisherError` on any failure condition.
pub fn validate_dispatch_response(
  resp: ProductionDispatchResponse,
  platform_label: &str,
) -> Result<Option<serde_json::Value>, PublisherError> {
  match resp.ok {
    None => {
      // ok field missing — treat as protocol violation
      Err(PublisherError::new(
        PublisherErrorCode::PlatformRejected,
        format!("[{platform_label}] Extension response missing 'ok' field (protocol violation)"),
        false,
      ))
    }
    Some(false) => {
      // Extension reported failure
      match resp.error {
        Some(err) => Err(map_dispatch_error(&err, platform_label)),
        None => Err(PublisherError::upload_failed(
          format!("[{platform_label}] Extension returned ok=false with no error details"),
          false,
        )),
      }
    }
    Some(true) => Ok(resp.result),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn test_dispatch_error_protocol_mismatch() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_123",
      "jobId": "job_456",
      "stepId": "publish_facebook_789",
      "attemptId": "attempt_1",
      "leaseId": "lease_abc",
      "profileId": "profile_1",
      "ok": false,
      "error": {
        "code": "PROTOCOL_MISMATCH",
        "message": "Command method social.facebook.reels.publish is not supported",
        "retryable": false
      }
    });

    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CapabilityUnavailable);
    assert!(!err.retryable);
    assert!(err.message.contains("PROTOCOL_MISMATCH"));
  }

  #[test]
  fn test_dispatch_missing_ok_field() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "result": { "postId": "123" }
    });

    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::PlatformRejected);
  }

  #[test]
  fn test_dispatch_valid_evidence() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_123",
      "ok": true,
      "result": {
        "postId": "fb_post_999",
        "postUrl": "https://facebook.com/watch?v=999"
      }
    });

    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let result = validate_dispatch_response(parsed, "Facebook").expect("Should succeed");
    let res_val = result.expect("Should have result");
    assert_eq!(res_val.get("postId").and_then(|v| v.as_str()), Some("fb_post_999"));
  }

  #[test]
  fn test_dispatch_auth_required_error() {
    let raw = json!({
      "ok": false,
      "error": {
        "code": "AUTH_REQUIRED",
        "message": "Session expired, please login",
        "retryable": false
      }
    });

    let parsed = parse_dispatch_body(raw, "TikTok").expect("Should parse");
    let err = validate_dispatch_response(parsed, "TikTok").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::AuthRequired);
    assert!(!err.retryable);
  }
}

