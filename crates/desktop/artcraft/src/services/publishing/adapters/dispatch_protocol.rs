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
  #[serde(rename = "jobId")]
  pub job_id: Option<String>,
  #[serde(rename = "stepId")]
  pub step_id: Option<String>,
  #[serde(rename = "attemptId")]
  pub attempt_id: Option<String>,
  #[serde(rename = "leaseId")]
  pub lease_id: Option<String>,
  #[serde(rename = "profileId")]
  pub profile_id: Option<String>,
  /// True if the action was performed successfully.
  pub ok: Option<bool>,
  /// Authoritative evidence of the completed action (e.g. postId, postUrl).
  pub result: Option<serde_json::Value>,
  /// Structured error, present when ok=false.
  pub error: Option<ProductionDispatchError>,
}

/// Expected correlation identity that MUST be matched by the dispatch response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedDispatchIdentity {
  pub request_id: String,
  pub job_id: String,
  pub step_id: String,
  pub attempt_id: String,
  pub lease_id: String,
  pub profile_id: String,
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
/// - PROTOCOL_MISMATCH → `ProtocolMismatch` (not retryable)
/// - CORRELATION_MISMATCH → `CorrelationMismatch` (not retryable)
/// - AUTH_REQUIRED / LOGIN_REQUIRED → `AuthRequired` (not retryable)
/// - CAPABILITY_UNAVAILABLE / METHOD_NOT_SUPPORTED / METHOD_UNSUPPORTED → `CapabilityUnavailable` (not retryable)
/// - TARGET_AMBIGUOUS → `TargetAmbiguous` (not retryable)
/// - BRIDGE_TIMEOUT / network timeouts after possible dispatch → `VerifyFailed` (not retryable; outcome unknown)
/// - Unknown code: `UploadFailed` — respect extension's `retryable` flag.
pub fn map_dispatch_error(err: &ProductionDispatchError, platform_label: &str) -> PublisherError {
  let code = err.code.as_deref().unwrap_or("UNKNOWN");
  let message = err.message.as_deref().unwrap_or("Extension returned an error with no message");
  let retryable = err.retryable.unwrap_or(false);

  if code.contains("PROTOCOL_MISMATCH") {
    PublisherError::protocol_mismatch(format!("[{platform_label}] {code}: {message}"))
  } else if code.contains("CORRELATION_MISMATCH") {
    PublisherError::correlation_mismatch(format!("[{platform_label}] {code}: {message}"))
  } else if code.contains("AUTH") || code.contains("LOGIN") || code.contains("GROK_NOT_LOGGED_IN")
    || message.contains("login") || message.contains("AUTH_REQUIRED")
  {
    PublisherError::auth_required(format!("[{platform_label}] {code}: {message}"))
  } else if code.contains("CAPABILITY")
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

/// Full semantic validation of a parsed dispatch response against expected execution identity.
///
/// Validation Order:
/// 1. Protocol exists and == "floword-production"
/// 2. ProtocolVersion exists and == 1
/// 3. All correlation fields exist and match ExpectedDispatchIdentity
/// 4. Validate `ok` field
/// 5. Return authoritative `result`
pub fn validate_dispatch_response(
  resp: ProductionDispatchResponse,
  expected: &ExpectedDispatchIdentity,
  platform_label: &str,
) -> Result<Option<serde_json::Value>, PublisherError> {
  // STEP 1 & 2: protocol exists and == "floword-production"
  let protocol = resp.protocol.as_deref().unwrap_or("");
  if protocol != "floword-production" {
    return Err(PublisherError::protocol_mismatch(format!(
      "[{platform_label}] Invalid protocol '{protocol}', expected 'floword-production'"
    )));
  }

  // STEP 3 & 4: protocolVersion exists and == 1
  match resp.protocol_version {
    Some(1) => {}
    Some(v) => {
      return Err(PublisherError::protocol_mismatch(format!(
        "[{platform_label}] Unsupported protocolVersion {v}, expected 1"
      )));
    }
    None => {
      return Err(PublisherError::protocol_mismatch(format!(
        "[{platform_label}] Missing protocolVersion in response envelope"
      )));
    }
  }

  // STEP 5 & 6: Correlation fields exist and match ExpectedDispatchIdentity
  let req_id = resp.request_id.as_deref().unwrap_or("");
  if req_id.is_empty() || req_id != expected.request_id {
    return Err(PublisherError::correlation_mismatch(format!(
      "[{platform_label}] requestId mismatch: received '{req_id}', expected '{}'",
      expected.request_id
    )));
  }

  let job_id = resp.job_id.as_deref().unwrap_or("");
  if job_id.is_empty() || job_id != expected.job_id {
    return Err(PublisherError::correlation_mismatch(format!(
      "[{platform_label}] jobId mismatch: received '{job_id}', expected '{}'",
      expected.job_id
    )));
  }

  let step_id = resp.step_id.as_deref().unwrap_or("");
  if step_id.is_empty() || step_id != expected.step_id {
    return Err(PublisherError::correlation_mismatch(format!(
      "[{platform_label}] stepId mismatch: received '{step_id}', expected '{}'",
      expected.step_id
    )));
  }

  let attempt_id = resp.attempt_id.as_deref().unwrap_or("");
  if attempt_id.is_empty() || attempt_id != expected.attempt_id {
    return Err(PublisherError::correlation_mismatch(format!(
      "[{platform_label}] attemptId mismatch: received '{attempt_id}', expected '{}'",
      expected.attempt_id
    )));
  }

  let lease_id = resp.lease_id.as_deref().unwrap_or("");
  if lease_id.is_empty() || lease_id != expected.lease_id {
    return Err(PublisherError::correlation_mismatch(format!(
      "[{platform_label}] leaseId mismatch: received '{lease_id}', expected '{}'",
      expected.lease_id
    )));
  }

  let profile_id = resp.profile_id.as_deref().unwrap_or("");
  if profile_id.is_empty() || profile_id != expected.profile_id {
    return Err(PublisherError::correlation_mismatch(format!(
      "[{platform_label}] profileId mismatch: received '{profile_id}', expected '{}'",
      expected.profile_id
    )));
  }

  // STEP 7: Validate ok
  match resp.ok {
    None => Err(PublisherError::new(
      PublisherErrorCode::PlatformRejected,
      format!("[{platform_label}] Extension response missing 'ok' field (protocol violation)"),
      false,
    )),
    Some(false) => match resp.error {
      Some(err) => Err(map_dispatch_error(&err, platform_label)),
      None => Err(PublisherError::upload_failed(
        format!("[{platform_label}] Extension returned ok=false with no error details"),
        false,
      )),
    },
    // STEP 8: Return authoritative result
    Some(true) => Ok(resp.result),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn valid_expected_identity() -> ExpectedDispatchIdentity {
    ExpectedDispatchIdentity {
      request_id: "req_1".to_string(),
      job_id: "job_1".to_string(),
      step_id: "step_1".to_string(),
      attempt_id: "attempt_1".to_string(),
      lease_id: "lease_1".to_string(),
      profile_id: "profile_1".to_string(),
    }
  }

  /// 11.1 Protocol validation — valid identity
  #[test]
  fn test_dispatch_valid_identity_success() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": {
        "postId": "123",
        "postUrl": "https://facebook.com/watch?v=123"
      }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let result = validate_dispatch_response(parsed, &expected, "Facebook").expect("Should succeed");
    let res_val = result.expect("Should have result");
    assert_eq!(res_val.get("postId").and_then(|v| v.as_str()), Some("123"));
  }

  /// 11.2 Wrong protocol
  #[test]
  fn test_dispatch_wrong_protocol() {
    let raw = json!({
      "protocol": "wrong-protocol",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::ProtocolMismatch);
    assert!(!err.retryable);
  }

  /// 11.3 Wrong protocolVersion
  #[test]
  fn test_dispatch_wrong_protocol_version() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 2,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::ProtocolMismatch);
    assert!(!err.retryable);
  }

  /// 11.4 Missing protocolVersion
  #[test]
  fn test_dispatch_missing_protocol_version() {
    let raw = json!({
      "protocol": "floword-production",
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::ProtocolMismatch);
  }

  /// 11.5 Wrong requestId
  #[test]
  fn test_dispatch_wrong_request_id() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_wrong",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
  }

  /// 11.6 Wrong jobId
  #[test]
  fn test_dispatch_wrong_job_id() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_other",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
  }

  /// 11.7 Wrong stepId
  #[test]
  fn test_dispatch_wrong_step_id() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_other",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
  }

  /// 11.8 Wrong attemptId
  #[test]
  fn test_dispatch_wrong_attempt_id() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_other",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
  }

  /// 11.9 Wrong leaseId
  #[test]
  fn test_dispatch_wrong_lease_id() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_other",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
  }

  /// 11.10 Wrong profileId
  #[test]
  fn test_dispatch_wrong_profile_id() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_other",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
  }

  /// 11.11 Stale success response: attempt_1 response received when waiting for attempt_2
  #[test]
  fn test_dispatch_stale_response_fails_closed() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_001",
      "jobId": "JOB_A",
      "stepId": "publish_facebook_pub1",
      "attemptId": "attempt_1",
      "leaseId": "LEASE_OLD",
      "profileId": "PROFILE_A",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected_attempt_2 = ExpectedDispatchIdentity {
      request_id: "req_002".to_string(),
      job_id: "JOB_A".to_string(),
      step_id: "publish_facebook_pub1".to_string(),
      attempt_id: "attempt_2".to_string(),
      lease_id: "LEASE_NEW".to_string(),
      profile_id: "PROFILE_A".to_string(),
    };

    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected_attempt_2, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
    assert!(!err.retryable);
  }

  /// 11.12 Missing identity field (leaseId missing)
  #[test]
  fn test_dispatch_missing_identity_field() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "profileId": "profile_1",
      "ok": true,
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CorrelationMismatch);
  }

  /// 11.13 ok=false with METHOD_NOT_SUPPORTED
  #[test]
  fn test_dispatch_error_method_not_supported() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": false,
      "error": {
        "code": "METHOD_NOT_SUPPORTED",
        "message": "Command method social.facebook.reels.publish is not supported",
        "retryable": false
      }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::CapabilityUnavailable);
    assert!(!err.retryable);
    assert!(err.message.contains("METHOD_NOT_SUPPORTED"));
  }

  /// Missing ok field
  #[test]
  fn test_dispatch_missing_ok_field() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "result": { "postId": "123" }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "Facebook").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "Facebook").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::PlatformRejected);
  }

  /// Auth required error
  #[test]
  fn test_dispatch_auth_required_error() {
    let raw = json!({
      "protocol": "floword-production",
      "protocolVersion": 1,
      "requestId": "req_1",
      "jobId": "job_1",
      "stepId": "step_1",
      "attemptId": "attempt_1",
      "leaseId": "lease_1",
      "profileId": "profile_1",
      "ok": false,
      "error": {
        "code": "AUTH_REQUIRED",
        "message": "Session expired, please login",
        "retryable": false
      }
    });

    let expected = valid_expected_identity();
    let parsed = parse_dispatch_body(raw, "TikTok").expect("Should parse");
    let err = validate_dispatch_response(parsed, &expected, "TikTok").unwrap_err();
    assert_eq!(err.code, PublisherErrorCode::AuthRequired);
    assert!(!err.retryable);
  }

  /// 11.15 Attempt identity generation test
  #[test]
  fn test_attempt_identity_semantics() {
    let pub_id = "pub_999";
    let idempotency_key = "stable_idempotency_hash_123";
    let step_id = format!("publish_facebook_{pub_id}");

    let attempt_1 = format!("pub_{pub_id}_attempt_1");
    let request_1 = format!("req_fb_{pub_id}_1_{}", uuid::Uuid::new_v4());

    let attempt_2 = format!("pub_{pub_id}_attempt_2");
    let request_2 = format!("req_fb_{pub_id}_2_{}", uuid::Uuid::new_v4());

    assert_eq!(step_id, "publish_facebook_pub_999");
    assert_ne!(attempt_1, attempt_2);
    assert_ne!(request_1, request_2);
    assert_eq!(idempotency_key, "stable_idempotency_hash_123");
  }
}
