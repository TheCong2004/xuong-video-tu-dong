use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssStyleSeed {
    pub font_size: Option<u32>,
    pub color: Option<String>,
    pub alignment: Option<u8>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssSpanStyle {
    pub start: usize,
    pub end: usize,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub color: Option<String>,
    pub size: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssCue {
    pub index: usize,
    pub start_us: i64,
    pub end_us: i64,
    pub text: String,
    pub style: Option<String>,
    pub spans: Option<Vec<AssSpanStyle>>,
    pub style_seed: Option<AssStyleSeed>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssStyleDef {
    pub name: String,
    pub font_name: String,
    pub font_size: f64,
    pub primary_colour: String,
    pub secondary_colour: String,
    pub outline_colour: String,
    pub back_colour: String,
    pub bold: i32,
    pub italic: i32,
    pub underline: i32,
    pub strikeout: i32,
    pub scale_x: f64,
    pub scale_y: f64,
    pub spacing: f64,
    pub angle: f64,
    pub border_style: i32,
    pub outline: f64,
    pub shadow: f64,
    pub alignment: i32,
    pub margin_l: i32,
    pub margin_r: i32,
    pub margin_v: i32,
    pub encoding: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssWord {
    pub word: String,
    pub start_us: i64,
    pub end_us: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssEvent {
    pub start_us: i64,
    pub end_us: i64,
    pub style: String,
    pub text: String,
    pub spans: Vec<AssSpanStyle>,
    pub words: Option<Vec<AssWord>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssDocument {
    pub styles: Vec<AssStyleDef>,
    pub events: Vec<AssEvent>,
    pub play_res_x: u32,
    pub play_res_y: u32,
}

#[derive(Debug, Error)]
pub enum AssError {
    #[error("invalid ASS timestamp: {0}")]
    InvalidTimestamp(String),
    #[error("missing required columns: {0}")]
    MissingColumns(String),
}

// ---------- helpers ----------

fn time_to_us(s: &str) -> Result<i64, AssError> {
    let s = s.trim();
    // regex: (\d+):(\d{2}):(\d{2})[.,](\d{1,3})
    let dot = s.find(|c| c == '.' || c == ',').ok_or_else(|| AssError::InvalidTimestamp(s.to_string()))?;
    let left = &s[..dot];
    let frac = &s[dot + 1..];
    if frac.is_empty() || frac.len() > 3 || !frac.chars().all(|c| c.is_ascii_digit()) {
        return Err(AssError::InvalidTimestamp(s.to_string()));
    }
    let mut parts = left.split(':');
    let h: i64 = parts.next().ok_or_else(|| AssError::InvalidTimestamp(s.to_string()))?.parse().map_err(|_| AssError::InvalidTimestamp(s.to_string()))?;
    let m: i64 = parts.next().ok_or_else(|| AssError::InvalidTimestamp(s.to_string()))?.parse().map_err(|_| AssError::InvalidTimestamp(s.to_string()))?;
    let sec: i64 = parts.next().ok_or_else(|| AssError::InvalidTimestamp(s.to_string()))?.parse().map_err(|_| AssError::InvalidTimestamp(s.to_string()))?;
    if parts.next().is_some() {
        return Err(AssError::InvalidTimestamp(s.to_string()));
    }
    // centisecond logic from TS: frac.padEnd(2,"0").slice(0,2), ms = cs*10
    let mut tmp = frac.to_string();
    while tmp.len() < 2 {
        tmp.push('0');
    }
    let cs: i64 = tmp[..2].parse().unwrap_or(0);
    let ms = cs * 10;
    Ok((h * 3600 + m * 60 + sec) * 1_000_000 + ms * 1000)
}

#[derive(Debug, Clone, Default, PartialEq)]
struct InlineState {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    color: Option<String>,
    size: Option<f64>,
}

fn same_state(a: &InlineState, b: &InlineState) -> bool {
    a.bold == b.bold && a.italic == b.italic && a.underline == b.underline && a.color == b.color && a.size == b.size
}

fn parse_ass_color(raw: &str) -> Option<(String, Option<f64>)> {
    // Accept &HBBGGRR&, &HAABBGGRR, &H..., or bare hex
    let s = raw.trim();
    let hex = if s.starts_with("&H") || s.starts_with("&h") {
        let inner = &s[2..];
        let inner = inner.trim_end_matches('&');
        inner
    } else if s.starts_with('#') {
        &s[1..]
    } else {
        s
    };
    if hex.is_empty() {
        return None;
    }
    let hex = hex.trim();
    if hex.len() == 6 {
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let r = &hex[4..6];
            let g = &hex[2..4];
            let b = &hex[0..2];
            return Some((format!("#{r}{g}{b}").to_uppercase().replace('#', "#"), None));
        }
        // Actually BBGGRR order: &HBBGGRR -> BB=blue, GG=green, RR=red
        // We built above b=hex[0..2] is BB, etc -> reassemble RRGGBB
    }
    if hex.len() == 8 {
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let a = &hex[0..2];
            let b = &hex[2..4];
            let g = &hex[4..6];
            let r = &hex[6..8];
            let alpha = u8::from_str_radix(a, 16).ok()? as f64;
            let opacity = 1.0 - alpha / 255.0;
            return Some((format!("#{r}{g}{b}").to_uppercase(), Some(opacity)));
        }
    }
    // bare 6 hex digits without prefix already handled
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some((format!("#{hex}").to_uppercase(), None));
    }
    None
}

pub fn parse_ass_color_pub(raw: &str) -> Option<(String, Option<f64>)> {
    let s = raw.trim();
    // Try to extract hex part robustly
    let cleaned = s.trim_matches(|c| c == '&' || c == 'H' || c == 'h');
    // Use helper
    if s.contains("&H") || s.contains("&h") {
        // byte reversed
        let hex = cleaned;
        return parse_ass_color(hex);
    }
    parse_ass_color(s)
}

fn apply_overrides(block: &str, state: &InlineState) -> InlineState {
    let mut next = state.clone();
    for tag in block.split('\\') {
        if tag.is_empty() {
            continue;
        }
        if let Some(rest) = tag.strip_prefix('b') {
            let s: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !s.is_empty() {
                if let Ok(n) = s.parse::<i32>() {
                    next.bold = Some(n == 1 || n >= 700);
                    continue;
                }
            }
        }
        if let Some(rest) = tag.strip_prefix('i') {
            if rest.starts_with('0') || rest.starts_with('1') {
                next.italic = Some(rest.starts_with('1'));
                continue;
            }
        }
        if let Some(rest) = tag.strip_prefix('u') {
            if rest.starts_with('0') || rest.starts_with('1') {
                next.underline = Some(rest.starts_with('1'));
                continue;
            }
        }
        if let Some(rest) = tag.strip_prefix("fs") {
            let num: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            if let Ok(v) = num.parse::<f64>() {
                next.size = Some(v);
                continue;
            }
        }
        if tag.starts_with("c&") || tag.starts_with("1c&") || tag.starts_with("c&H") || tag.starts_with("1c&H") {
            // find &H...&
            if let Some(start) = tag.find("&H").or_else(|| tag.find("&h")) {
                let hex_part = &tag[start..];
                if let Some((col, _)) = parse_ass_color(hex_part) {
                    next.color = Some(col);
                    continue;
                }
            }
            if let Some((col, _)) = parse_ass_color(tag) {
                next.color = Some(col);
                continue;
            }
        }
        // handle \c without &H? already
        if tag.starts_with('c') || tag.starts_with("1c") {
            // try parse after c
            let after = if tag.starts_with("1c") { &tag[2..] } else { &tag[1..] };
            if let Some((col, _)) = parse_ass_color(after) {
                next.color = Some(col);
                continue;
            }
        }
        if tag.starts_with('r') {
            next = InlineState::default();
        }
    }
    next
}

fn parse_dialogue_text(raw: &str) -> (String, Vec<AssSpanStyle>) {
    let mut text = String::new();
    let mut spans: Vec<AssSpanStyle> = Vec::new();
    let mut state = InlineState::default();
    let mut span_start: usize = 0;

    let flush = |end: usize, state: &InlineState, span_start: usize, spans: &mut Vec<AssSpanStyle>| {
        if end <= span_start {
            return;
        }
        let has = state.bold.is_some() || state.italic.is_some() || state.underline.is_some() || state.color.is_some() || state.size.is_some();
        if !has {
            return;
        }
        let mut span = AssSpanStyle { start: span_start, end, bold: None, italic: None, underline: None, color: None, size: None };
        if let Some(v) = state.bold { span.bold = Some(v); }
        if let Some(v) = state.italic { span.italic = Some(v); }
        if let Some(v) = state.underline { span.underline = Some(v); }
        if let Some(ref v) = state.color { span.color = Some(v.clone()); }
        if let Some(v) = state.size { span.size = Some(v); }
        spans.push(span);
    };

    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            if n == 'N' || n == 'n' {
                text.push('\n');
            } else if n == 'h' {
                text.push(' ');
            } else if n == '{' || n == '}' {
                text.push(n);
            } else {
                text.push(ch);
                text.push(n);
            }
            i += 2;
            continue;
        }
        if ch == '{' {
            if let Some(close) = raw[i + 1..].find('}') {
                let block = &raw[i + 1..i + 1 + close];
                let next_state = apply_overrides(block, &state);
                if !same_state(&next_state, &state) {
                    let len = text.encode_utf16().count();
                    flush(len, &state, span_start, &mut spans);
                    state = next_state;
                    span_start = len;
                }
                i += close + 2; // skip { ... }
                continue;
            } else {
                text.push(ch);
                i += 1;
                continue;
            }
        }
        text.push(ch);
        i += 1;
    }
    let len = text.encode_utf16().count();
    flush(len, &state, span_start, &mut spans);

    // trim and adjust offsets like TS (trim start/end, shift)
    let trimmed = text.trim().to_string();
    let lead = text.len() - text.trim_start().len();
    // lead in chars not code units; compute code units of leading trim
    let lead_units = text[..lead].encode_utf16().count();
    let trimmed_units = trimmed.encode_utf16().count();
    let mut adjusted = Vec::new();
    for mut s in spans {
        let start = s.start.saturating_sub(lead_units);
        let end = (s.end.saturating_sub(lead_units)).min(trimmed_units);
        if end > start {
            s.start = start;
            s.end = end;
            adjusted.push(s);
        }
    }
    (trimmed, adjusted)
}

fn prune_against_style(spans: Vec<AssSpanStyle>, seed: Option<&AssStyleSeed>) -> Vec<AssSpanStyle> {
    let bold = seed.and_then(|s| s.bold).unwrap_or(false);
    let italic = seed.and_then(|s| s.italic).unwrap_or(false);
    let mut out = Vec::new();
    for s in spans {
        let mut ns = AssSpanStyle { start: s.start, end: s.end, bold: None, italic: None, underline: None, color: None, size: None };
        if let Some(v) = s.bold { if v != bold { ns.bold = Some(v); } }
        if let Some(v) = s.italic { if v != italic { ns.italic = Some(v); } }
        if let Some(v) = s.underline { if v { ns.underline = Some(v); } }
        if let Some(ref c) = s.color {
            let seed_col = seed.and_then(|x| x.color.as_ref());
            if Some(c) != seed_col { ns.color = Some(c.clone()); }
        }
        if let Some(v) = s.size {
            if Some(v as u32) != seed.and_then(|x| x.font_size) { ns.size = Some(v); }
        }
        let has = ns.bold.is_some() || ns.italic.is_some() || ns.underline.is_some() || ns.color.is_some() || ns.size.is_some();
        if has { out.push(ns); }
    }
    out
}

fn parse_style_line(line: &str, format: &[String]) -> Option<(String, AssStyleSeed)> {
    let colon = line.find(':')?;
    let values: Vec<String> = line[colon + 1..].split(',').map(|s| s.trim().to_string()).collect();
    let col = |name: &str| {
        format.iter().position(|x| x == name).and_then(|at| values.get(at).cloned())
    };
    let name = col("name")?;
    if name.is_empty() { return None; }
    let mut seed = AssStyleSeed::default();
    if let Some(v) = col("fontsize") {
        if let Ok(n) = v.parse::<u32>() { if n > 0 { seed.font_size = Some(n); } }
    }
    if let Some(v) = col("primarycolour") {
        if let Some((c, _)) = parse_ass_color(&v) { seed.color = Some(c); }
    }
    if let Some(v) = col("alignment") {
        if let Ok(n) = v.parse::<i32>() { if (1..=9).contains(&n) { seed.alignment = Some(((n - 1) % 3) as u8); } }
    }
    if let Some(v) = col("bold") {
        seed.bold = Some(v == "-1" || v == "1");
    }
    if let Some(v) = col("italic") {
        seed.italic = Some(v == "-1" || v == "1");
    }
    Some((name, seed))
}

pub fn parse_ass(content: &str) -> Result<Vec<AssCue>, AssError> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut section = "other";
    let mut format: Option<Vec<String>> = None;
    let mut style_format: Option<Vec<String>> = None;
    let mut style_seeds: std::collections::HashMap<String, AssStyleSeed> = std::collections::HashMap::new();
    let mut cues = Vec::new();
    let mut idx: usize = 0;

    for raw in lines {
        let line = raw.trim();
        if line.is_empty() { continue; }
        if line.starts_with('[') {
            let lower = line.to_lowercase();
            if lower.starts_with("[events]") { section = "events"; }
            else if lower.starts_with("[v4+ styles]") || lower.starts_with("[v4 styles]") { section = "styles"; }
            else { section = "other"; }
            continue;
        }
        if section == "styles" {
            if line.to_lowercase().starts_with("format") {
                if let Some(colon) = line.find(':') {
                    style_format = Some(line[colon+1..].split(',').map(|s| s.trim().to_lowercase()).collect());
                }
            } else if line.to_lowercase().starts_with("style") {
                if let Some(ref sf) = style_format.clone() {
                    if let Some((name, seed)) = parse_style_line(line, sf) {
                        style_seeds.insert(name, seed);
                    }
                }
            }
            continue;
        }
        if section != "events" { continue; }
        if line.to_lowercase().starts_with("format") {
            if let Some(colon) = line.find(':') {
                format = Some(line[colon+1..].split(',').map(|s| s.trim().to_lowercase()).collect());
            }
            continue;
        }
        if !line.to_lowercase().starts_with("dialogue") { continue; }
        if format.is_none() {
            format = Some(vec!["layer","start","end","style","name","marginl","marginr","marginv","effect","text"].into_iter().map(|s| s.to_string()).collect());
        }
        let fmt = format.as_ref().unwrap();
        let colon = line.find(':').unwrap();
        let rest = line[colon+1..].trim();
        let start_col = fmt.iter().position(|x| x=="start");
        let end_col = fmt.iter().position(|x| x=="end");
        let text_col = fmt.iter().position(|x| x=="text");
        let style_col = fmt.iter().position(|x| x=="style");
        if start_col.is_none() || end_col.is_none() || text_col.is_none() {
            return Err(AssError::MissingColumns(fmt.join(",")));
        }
        let sc = start_col.unwrap();
        let ec = end_col.unwrap();
        let tc = text_col.unwrap();
        // split into fmt.len()-1 commas, last is text
        let mut parts: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut col_idx = 0usize;
        for ch in rest.chars() {
            if ch == ',' && col_idx < fmt.len()-1 {
                parts.push(cur.clone());
                cur.clear();
                col_idx += 1;
            } else {
                cur.push(ch);
            }
        }
        parts.push(cur);
        if parts.len() < fmt.len() { continue; }
        let start_us = time_to_us(parts[sc].trim())?;
        let end_us = time_to_us(parts[ec].trim())?;
        if end_us <= start_us { continue; }
        let (text, spans) = parse_dialogue_text(&parts[tc]);
        if text.is_empty() { continue; }
        let style_name = style_col.and_then(|c| {
            let s = parts[c].trim().trim_start_matches('*').to_string();
            if s.is_empty() { None } else { Some(s) }
        });
        let seed = style_name.as_ref().and_then(|n| style_seeds.get(n).cloned());
        let kept = prune_against_style(spans, seed.as_ref());
        idx += 1;
        cues.push(AssCue {
            index: idx,
            start_us,
            end_us,
            text,
            style: style_name,
            spans: if kept.is_empty() { None } else { Some(kept) },
            style_seed: seed,
        });
    }
    Ok(cues)
}

pub fn ass_time(us: i64) -> String {
    let total_cs = (us as f64 / 10_000.0).round() as i64;
    let sign = if total_cs < 0 { "-" } else { "" };
    let abs = total_cs.abs();
    let cs = abs % 100;
    let total_s = abs / 100;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{sign}{h}:{m:02}:{s:02}.{cs:02}")
}

pub fn serialize_ass(cues: &[AssCue]) -> String {
    let mut out = String::new();
    out.push_str("[Script Info]\nTitle: Exported\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n");
    out.push_str("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    out.push_str("Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n");
    out.push_str("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for c in cues {
        let text = c.text.replace('\n', "\\N");
        let style = c.style.as_deref().unwrap_or("Default");
        out.push_str(&format!("Dialogue: 0,{}, {},{},{},0,0,0,,{}\n", ass_time(c.start_us), ass_time(c.end_us), style, "", text));
        // Actually need 10 columns; use empty name/margins/effect
    }
    // Fix above line: ensure correct commas
    // Rebuild properly if needed - the above string has extra comma; redo with correct format
    // We'll reconstruct events section correctly
    // For simplicity, regen events
    let events: String = cues.iter().map(|c| {
        let text = c.text.replace('\n', "\\N");
        let style = c.style.as_deref().unwrap_or("Default");
        format!("Dialogue: 0,{},{},{},,0,0,0,,{}\n", ass_time(c.start_us), ass_time(c.end_us), style, text)
    }).collect();
    // Replace events section
    let prefix = "[Script Info]\nTitle: Exported\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";
    format!("{prefix}{events}")
}

pub fn format_ass_color(hex: &str, alpha: f64) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 { return "&H00FFFFFF".to_string(); }
    let r = &h[0..2]; let g = &h[2..4]; let b = &h[4..6];
    let a = ((1.0 - alpha.clamp(0.0,1.0)) * 255.0).round() as u8;
    format!("&H{a:02X}{b}{g}{r}")
}

pub fn default_ass_style(name: &str) -> AssStyleDef {
    AssStyleDef {
        name: name.to_string(),
        font_name: "Arial".to_string(),
        font_size: 48.0,
        primary_colour: "&H00FFFFFF".to_string(),
        secondary_colour: "&H000000FF".to_string(),
        outline_colour: "&H00000000".to_string(),
        back_colour: "&H00000000".to_string(),
        bold: 0, italic: 0, underline: 0, strikeout: 0,
        scale_x: 100.0, scale_y: 100.0, spacing: 0.0, angle: 0.0,
        border_style: 1, outline: 2.0, shadow: 2.0,
        alignment: 2, margin_l: 10, margin_r: 10, margin_v: 10, encoding: 1,
    }
}
