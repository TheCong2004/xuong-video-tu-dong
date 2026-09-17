use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Stdin,
    Argv,
}

impl std::fmt::Display for Delivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdin => write!(f, "stdin"),
            Self::Argv => write!(f, "argv"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TtsRunResult {
    pub out_path: PathBuf,
    pub bytes: u64,
    pub delivery: Delivery,
}

#[derive(Debug, Clone)]
pub struct BuildTtsArgvResult {
    pub argv: Vec<String>,
    pub delivery: Delivery,
}

/// Split a command template into argv tokens: whitespace-separated, with single
/// or double quotes grouping (quote chars dropped). No escapes/globs.
pub fn split_command_template(template: &str) -> Result<Vec<String>> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut in_token = false;
    for ch in template.chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
            in_token = true;
        } else if ch == ' ' || ch == '\t' {
            if in_token {
                tokens.push(std::mem::take(&mut current));
                in_token = false;
            }
        } else {
            current.push(ch);
            in_token = true;
        }
    }
    if quote.is_some() {
        bail!("Unbalanced {} quote in --tts-cmd template: {template}", quote.unwrap());
    }
    if in_token {
        tokens.push(current);
    }
    Ok(tokens)
}

/// Substitute `{out}` / `{text}` into tokenized template.
pub fn build_tts_argv(text: &str, template: &str, out_path: &Path) -> Result<BuildTtsArgvResult> {
    let tokens = split_command_template(template)?;
    if tokens.is_empty() {
        bail!("--tts-cmd template is empty.");
    }
    if !tokens.iter().any(|t| t.contains("{out}")) {
        bail!(
            "--tts-cmd template has no {{out}} placeholder, so there is no way to know where the tool writes its audio. \
             Add {{out}} where the tool expects its output path, e.g. --tts-cmd \"piper --output_file {{out}}\"."
        );
    }
    let delivery = if tokens.iter().any(|t| t.contains("{text}")) {
        Delivery::Argv
    } else {
        Delivery::Stdin
    };
    let out_str = out_path.to_string_lossy().to_string();
    let argv = tokens
        .into_iter()
        .map(|t| t.replace("{out}", &out_str).replace("{text}", text))
        .collect();
    Ok(BuildTtsArgvResult { argv, delivery })
}

/// First free `<stem><ext>`, `<stem>-2<ext>`, ... path in `dir` (created if needed).
pub fn collision_safe_out_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let _ = std::fs::create_dir_all(dir);
    let mut candidate = dir.join(format!("{stem}{ext}"));
    let mut n: u32 = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}{ext}"));
        n += 1;
    }
    candidate
}

const STDERR_TAIL_CHARS: usize = 2000;

/// Run the TTS template and verify it produced a non-empty file at `out_path`.
/// Requires an external binary; spawns without a shell.
pub fn synthesize_speech(text: &str, template: &str, out_path: &Path) -> Result<TtsRunResult> {
    let BuildTtsArgvResult { argv, delivery } = build_tts_argv(text, template, out_path)?;
    let (cmd, args) = argv.split_first().expect("non-empty checked above");

    let mut child = std::process::Command::new(cmd);
    child.args(args);
    if delivery == Delivery::Stdin {
        child.stdin(std::process::Stdio::piped());
    }
    child.stdout(std::process::Stdio::piped());
    child.stderr(std::process::Stdio::piped());

    let mut child = child.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("TTS command not found: '{cmd}'. Install it or point --tts-cmd at an existing binary.")
        } else {
            anyhow::anyhow!("TTS spawn failed: {e}")
        }
    })?;

    if delivery == Delivery::Stdin {
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
    }

    let output = child.wait_with_output()?;
    let stderr_tail = {
        let s = String::from_utf8_lossy(&output.stderr);
        let tail = if s.len() > STDERR_TAIL_CHARS {
            &s[s.len() - STDERR_TAIL_CHARS..]
        } else {
            &s
        };
        tail.trim().to_string()
    };
    let fail_msg = |reason: String| -> String {
        let _ = std::fs::remove_file(out_path);
        if stderr_tail.is_empty() {
            reason
        } else {
            format!("{reason}\nstderr: {stderr_tail}")
        }
    };

    if !output.status.success() {
        bail!("{}", fail_msg(format!("TTS command failed (exited {:?}): {cmd} {}", output.status.code(), args.join(" "))));
    }
    let meta = std::fs::metadata(out_path).map_err(|_| {
        anyhow::anyhow!("{}", fail_msg(format!("TTS command succeeded but wrote no audio at {}. Check the template's {{out}} placeholder.", out_path.display())))
    })?;
    if meta.len() == 0 {
        let _ = std::fs::remove_file(out_path);
        bail!("TTS command succeeded but wrote no audio at {}. Check the template's {{out}} placeholder sits where the tool expects its output path.", out_path.display());
    }
    Ok(TtsRunResult { out_path: out_path.to_path_buf(), bytes: meta.len(), delivery })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_basic() {
        assert_eq!(split_command_template("a b c").unwrap(), vec!["a","b","c"]);
    }
    #[test]
    fn split_quoted() {
        assert_eq!(split_command_template(r#"cmd "hello world" 'foo bar'"#).unwrap(), vec!["cmd","hello world","foo bar"]);
    }
    #[test]
    fn build_delivery_argv() {
        let r = build_tts_argv("hi", "piper --out {out} --text {text}", Path::new("/tmp/out.wav")).unwrap();
        assert_eq!(r.delivery, Delivery::Argv);
        assert!(r.argv.iter().any(|a| a.contains("hi")));
    }
    #[test]
    fn build_delivery_stdin() {
        let r = build_tts_argv("hi", "piper --output_file {out}", Path::new("/tmp/out.wav")).unwrap();
        assert_eq!(r.delivery, Delivery::Stdin);
    }
}
