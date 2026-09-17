use anyhow::{Context, Result};
use std::path::Path;

/// Mirrors `reference/src/decrypt.ts:detectEncryption` semantics.
///
/// Returns the raw bytes to be written for a draft file:
/// - If the bytes look like JSON (starts with `{` and parses), returns them as-is.
/// - If the file is missing, bails.
/// - Otherwise (binary / non-JSON — JianYing 6.0+ encrypted payload), bails with
///   actionable guidance instead of attempting to decrypt (legal posture / flux).
pub fn decrypt_draft_bytes(input: &Path) -> Result<Vec<u8>> {
    let buf = std::fs::read(input)
        .with_context(|| format!("read draft file {}", input.display()))?;
    if buf.is_empty() {
        anyhow::bail!("draft file is empty: {}", input.display());
    }
    let head = buf.iter().copied().skip_while(|b| b.is_ascii_whitespace()).next();
    if head == Some(b'{') {
        if serde_json::from_slice::<serde_json::Value>(&buf).is_ok() {
            return Ok(buf);
        }
        anyhow::bail!(
            "draft file at {} starts with '{{' but is not valid JSON — likely corrupted, not encrypted. Try opening + saving in CapCut/JianYing to repair, or restore from .bak",
            input.display()
        );
    }
    anyhow::bail!(
        "decrypt requires JianYing 6.0+ handling — file at {} does not start with '{{' and is not parseable as JSON (likely AES-encrypted payload). \
Workarounds: 1) pin JianYing to 5.9.x and block auto-update, 2) use CapCut International (not encrypted), \
3) see https://github.com/GuanYixuan/pyJianYingDraft/issues/142 and https://github.com/duoec/duo-video for community decryption references. \
capcut-cli detects but does not decrypt by design — see docs/jianying-encryption.md",
        input.display()
    );
}

/// Convenience: decrypt file `input` and write result to `output`.
pub fn decrypt_draft_file(input: &Path, output: &Path) -> Result<()> {
    let bytes = decrypt_draft_bytes(input)?;
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create output dir {}", parent.display()))?;
        }
    }
    std::fs::write(output, bytes)
        .with_context(|| format!("write output {}", output.display()))?;
    Ok(())
}

/// Alias expected by CLI (`decrypt::decrypt_file`).
pub fn decrypt_file(input: &Path, output: &Path) -> Result<()> {
    decrypt_draft_file(input, output)
}
