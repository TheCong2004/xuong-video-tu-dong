use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseClass {
    #[serde(rename = "permissive")]
    Permissive,
    #[serde(rename = "fair-use")]
    FairUse,
    #[serde(rename = "restrictive")]
    Restrictive,
    #[serde(rename = "unknown")]
    Unknown,
}

impl std::fmt::Display for LicenseClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Permissive => write!(f, "permissive"),
            Self::FairUse => write!(f, "fair-use"),
            Self::Restrictive => write!(f, "restrictive"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikimediaAsset {
    pub file_title: String,
    pub direct_url: String,
    pub description_url: String,
    pub mime: String,
    pub size_bytes: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub license: WikimediaLicense,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikimediaLicense {
    pub raw: Option<String>,
    pub class: LicenseClass,
    pub artist: Option<String>,
    pub credit: Option<String>,
}

/// Is this a supported Wikimedia-family URL?
pub fn is_wikimedia_url(u: &str) -> bool {
    let host = host_of(u);
    let h = host.to_ascii_lowercase();
    if h == "commons.wikimedia.org" || h == "upload.wikimedia.org" || h == "meta.wikimedia.org" {
        return true;
    }
    h == "wikipedia.org" || h.ends_with(".wikipedia.org")
}

fn host_of(u: &str) -> String {
    let s = u.trim();
    let s = s.split("://").nth(1).unwrap_or(s);
    let s = s.split('/').next().unwrap_or("");
    let s = s.split('?').next().unwrap_or(s);
    let s = s.split('#').next().unwrap_or(s);
    let s = s.split(':').next().unwrap_or(s);
    s.to_string()
}

fn path_of(u: &str) -> String {
    let s = u.split("://").nth(1).unwrap_or(u);
    let after_host = s.find('/').map(|i| &s[i..]).unwrap_or("/");
    let without_query = after_host.split('?').next().unwrap_or(after_host);
    without_query.split('#').next().unwrap_or(without_query).to_string()
}

fn query_param(u: &str, key: &str) -> Option<String> {
    let q = u.split('?').nth(1)?;
    let q = q.split('#').next().unwrap_or(q);
    for pair in q.split('&') {
        let mut iter = pair.splitn(2, '=');
        let k = iter.next().unwrap_or("");
        let v = iter.next().unwrap_or("");
        if k == key {
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(a), Some(b)) = (h1, h2) {
                if let Ok(byte) = u8::from_str_radix(&format!("{a}{b}"), 16) {
                    out.push(byte as char);
                    continue;
                }
                out.push('%');
                out.push(a);
                out.push(b);
            } else {
                out.push('%');
                if let Some(a) = h1 { out.push(a); }
                if let Some(b) = h2 { out.push(b); }
            }
        } else if c == '+' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' { in_tag = true; continue; }
        if c == '>' { in_tag = false; continue; }
        if !in_tag { out.push(c); }
    }
    out.trim().to_string()
}

/// Classify a LicenseShortName string from extmetadata.LicenseShortName.value.
pub fn classify_license(raw: Option<&str>) -> LicenseClass {
    let Some(raw) = raw else { return LicenseClass::Unknown; };
    if raw.trim().is_empty() { return LicenseClass::Unknown; }
    let s_lower = raw.trim().to_lowercase();
    let is_cc0 = {
        let t = s_lower.trim_start();
        t.starts_with("cc0") && (t.len() == 3 || !t.chars().nth(3).unwrap_or(' ').is_alphanumeric())
            || t.starts_with("cc-0") || t.starts_with("cc 0")
    };
    let is_cc_by_permissive = {
        let t = s_lower.trim_start();
        if !t.starts_with("cc") { false } else {
            let mut rest = &t[2..];
            rest = rest.trim_start_matches(|c: char| c == ' ' || c == '\t' || c == '-');
            if !rest.starts_with("by") { false } else {
                rest = &rest[2..];
                rest = rest.trim_start_matches(|c: char| c == ' ' || c == '\t' || c == '-');
                if rest.starts_with("sa") {
                    rest = &rest[2..];
                    rest = rest.trim_start();
                    rest.is_empty() || rest.chars().next().map(|c| c.is_ascii_digit() || c.is_whitespace()).unwrap_or(true)
                } else {
                    let peek = rest.trim_start();
                    if peek.starts_with("nc") || peek.starts_with("nd") || peek.starts_with("non") {
                        false
                    } else {
                        rest.is_empty() || rest.trim_start().is_empty() || rest.trim_start().chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) || rest.trim().is_empty()
                    }
                }
            }
        }
    };
    let is_pd = s_lower.contains("public domain") || s_lower.contains("pd-") || s_lower.contains("pd ") || s_lower == "pd";
    let is_no_restrictions = s_lower.contains("no restrictions");
    if is_cc0 || is_cc_by_permissive || is_pd || is_no_restrictions {
        return LicenseClass::Permissive;
    }
    let tokens: Vec<&str> = s_lower.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
    if tokens.contains(&"nc") { return LicenseClass::Restrictive; }
    if s_lower.contains("non-commercial") || s_lower.contains("noncommercial") { return LicenseClass::Restrictive; }
    if tokens.contains(&"nd") { return LicenseClass::Restrictive; }
    if s_lower.contains("no-deriv") || s_lower.contains("no deriv") { return LicenseClass::Restrictive; }
    if s_lower.contains("fair use") || s_lower.contains("fair-use") || s_lower.contains("fair dealing") || s_lower.contains("fair-dealing") {
        return LicenseClass::FairUse;
    }
    if tokens.contains(&"fu") { return LicenseClass::FairUse; }
    if s_lower.contains('©') || s_lower.contains("copyright") {
        return LicenseClass::Restrictive;
    }
    LicenseClass::Unknown
}

/// Extract "File:Foo.jpg" from any accepted Wikimedia URL. None if URL is an API call needing different handling.
pub fn extract_file_title(u: &str) -> Option<String> {
    let host = host_of(u).to_ascii_lowercase();
    let path = path_of(u);

    if host == "upload.wikimedia.org" {
        if let Some(idx) = path.to_ascii_lowercase().find("/wikipedia/") {
            let rest = &path[idx + "/wikipedia/".len()..];
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 4 {
                let mut offset = 1;
                if parts.get(1) == Some(&"thumb") { offset = 2; }
                if parts.len() >= offset + 3 {
                    let filename = parts[offset + 2];
                    if !filename.is_empty() {
                        return Some(format!("File:{}", percent_decode(filename)));
                    }
                }
            }
        }
        return None;
    }

    if let Some(idx) = path.to_ascii_lowercase().find("/wiki/") {
        let after = &path[idx + "/wiki/".len()..];
        let decoded = percent_decode(after);
        let lower = decoded.to_ascii_lowercase();
        if lower.starts_with("file:") || lower.starts_with("image:") || lower.starts_with("media:") {
            let colon = decoded.find(':').unwrap();
            let title = &decoded[colon+1..];
            return Some(format!("File:{title}"));
        }
    }

    if path.ends_with("/w/api.php") || path == "/api.php" {
        if let Some(titles) = query_param(u, "titles") {
            let low = titles.to_ascii_lowercase();
            if low.starts_with("file:") || low.starts_with("image:") || low.starts_with("media:") {
                let colon = titles.find(':').unwrap();
                let rest = &titles[colon+1..];
                return Some(format!("File:{rest}"));
            }
        }
    }
    None
}

pub fn build_wikimedia_asset_from_imageinfo(file_title: &str, info: &serde_json::Value) -> WikimediaAsset {
    let url = info.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let description_url = info.get("descriptionurl").and_then(|x| x.as_str()).map(|s| s.to_string())
        .unwrap_or_else(|| format!("https://commons.wikimedia.org/wiki/{}", file_title.replace(' ', "_")));
    let mime = info.get("mime").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let size_bytes = info.get("size").and_then(|x| x.as_i64());
    let width = info.get("width").and_then(|x| x.as_i64());
    let height = info.get("height").and_then(|x| x.as_i64());
    let ext = info.get("extmetadata").and_then(|x| x.as_object());
    let raw_license = ext.and_then(|m| m.get("LicenseShortName")).and_then(|v| v.get("value")).and_then(|x| x.as_str())
        .or_else(|| ext.and_then(|m| m.get("License")).and_then(|v| v.get("value")).and_then(|x| x.as_str()))
        .map(|s| s.to_string());
    let artist_html = ext.and_then(|m| m.get("Artist")).and_then(|v| v.get("value")).and_then(|x| x.as_str()).map(|s| s.to_string());
    let credit_raw = ext.and_then(|m| m.get("Credit")).and_then(|v| v.get("value")).and_then(|x| x.as_str()).map(|s| s.to_string());
    let artist = artist_html.and_then(|a| { let t = strip_html(&a); if t.is_empty() { None } else { Some(t) } });
    let credit = credit_raw.and_then(|c| { let t = strip_html(&c); if t.is_empty() { None } else { Some(t) } });
    let class = classify_license(raw_license.as_deref());
    WikimediaAsset {
        file_title: file_title.to_string(),
        direct_url: url,
        description_url,
        mime,
        size_bytes,
        width,
        height,
        license: WikimediaLicense { raw: raw_license, class, artist, credit },
    }
}

pub fn assert_inside_dest_dir(local_path: &str, dest_dir: &str) -> anyhow::Result<()> {
    let norm = |s: &str| {
        let p = std::path::Path::new(s);
        let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(p) };
        abs.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_string()
    };
    let n_local = norm(local_path);
    let n_dest = norm(dest_dir);
    if !n_local.starts_with(&n_dest) {
        anyhow::bail!("path {} escapes dest dir {}", local_path, dest_dir);
    }
    Ok(())
}
