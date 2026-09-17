use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Namespace
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    #[serde(rename = "capcut")]
    CapCut,
    #[serde(rename = "jianying")]
    JianYing,
}

impl Namespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CapCut => "capcut",
            Self::JianYing => "jianying",
        }
    }
}

impl std::str::FromStr for Namespace {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "capcut" => Ok(Self::CapCut),
            "jianying" | "jian_ying" | "jy" => Ok(Self::JianYing),
            _ => Err(format!("unknown namespace: {s}")),
        }
    }
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Category
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    #[serde(rename = "transitions")]
    Transitions,
    #[serde(rename = "masks")]
    Masks,
    #[serde(rename = "image_intros")]
    ImageIntros,
    #[serde(rename = "image_outros")]
    ImageOutros,
    #[serde(rename = "image_combos")]
    ImageCombos,
    #[serde(rename = "text_intros")]
    TextIntros,
    #[serde(rename = "text_outros")]
    TextOutros,
    #[serde(rename = "text_loop_anims")]
    TextLoopAnims,
    #[serde(rename = "scene_effects")]
    SceneEffects,
    #[serde(rename = "character_effects")]
    CharacterEffects,
    #[serde(rename = "audio_effects")]
    AudioEffects,
    #[serde(rename = "fonts")]
    Fonts,
    #[serde(rename = "filters")]
    Filters,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transitions => "transitions",
            Self::Masks => "masks",
            Self::ImageIntros => "image_intros",
            Self::ImageOutros => "image_outros",
            Self::ImageCombos => "image_combos",
            Self::TextIntros => "text_intros",
            Self::TextOutros => "text_outros",
            Self::TextLoopAnims => "text_loop_anims",
            Self::SceneEffects => "scene_effects",
            Self::CharacterEffects => "character_effects",
            Self::AudioEffects => "audio_effects",
            Self::Fonts => "fonts",
            Self::Filters => "filters",
        }
    }

    pub fn all() -> &'static [Category] {
        &[
            Self::Transitions,
            Self::Masks,
            Self::ImageIntros,
            Self::ImageOutros,
            Self::ImageCombos,
            Self::TextIntros,
            Self::TextOutros,
            Self::TextLoopAnims,
            Self::SceneEffects,
            Self::CharacterEffects,
            Self::AudioEffects,
            Self::Fonts,
            Self::Filters,
        ]
    }
}

impl std::str::FromStr for Category {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "transitions" => Ok(Self::Transitions),
            "masks" => Ok(Self::Masks),
            "image_intros" => Ok(Self::ImageIntros),
            "image_outros" => Ok(Self::ImageOutros),
            "image_combos" => Ok(Self::ImageCombos),
            "text_intros" => Ok(Self::TextIntros),
            "text_outros" => Ok(Self::TextOutros),
            "text_loop_anims" => Ok(Self::TextLoopAnims),
            "scene_effects" => Ok(Self::SceneEffects),
            "character_effects" => Ok(Self::CharacterEffects),
            "audio_effects" => Ok(Self::AudioEffects),
            "fonts" => Ok(Self::Fonts),
            "filters" => Ok(Self::Filters),
            _ => Err(format!("unknown category: {s}")),
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// EnumEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumEntry {
    pub member: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_duration: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_overlap: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_vip: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_aspect_ratio: Option<f64>,
}

impl EnumEntry {
    pub fn display_name(&self) -> Option<&str> {
        self.name.as_deref().or(self.title.as_deref())
    }
}

// ---------------------------------------------------------------------------
// Embedded enums.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct EnumsFile {
    #[serde(default)]
    capcut: HashMap<String, Vec<EnumEntry>>,
    #[serde(default)]
    jianying: HashMap<String, Vec<EnumEntry>>,
}
static ENUMS_JSON: &str = include_str!("../assets/enums.json");

static ENUMS: LazyLock<EnumsFile> = LazyLock::new(|| {
    serde_json::from_str::<EnumsFile>(ENUMS_JSON).expect("embedded enums.json must be valid JSON")
});

fn enums_file() -> &'static EnumsFile {
    &ENUMS
}

fn bundled_entries(category: Category, namespace: Namespace) -> &'static [EnumEntry] {
    let file = enums_file();
    let map = match namespace {
        Namespace::CapCut => &file.capcut,
        Namespace::JianYing => &file.jianying,
    };
    map.get(category.as_str())
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// Public lookup API (mirrors enums.ts)
// ---------------------------------------------------------------------------

/// List all entries for a category/namespace, appending user catalogue entries
/// (namespace-agnostic) after the bundled table so bundled wins on collision.
pub fn list_enum(category: Category, namespace: Namespace) -> Vec<EnumEntry> {
    let bundled = bundled_entries(category, namespace);
    let user = user_entries_for_category(category, None);
    if user.is_empty() {
        return bundled.to_vec();
    }
    let mut out = Vec::with_capacity(bundled.len() + user.len());
    out.extend_from_slice(bundled);
    out.extend(user);
    out
}

/// Convenience: list without specifying namespace (defaults to CapCut).
pub fn list_enum_default(category: Category) -> Vec<EnumEntry> {
    list_enum(category, Namespace::CapCut)
}

/// Look up an entry by slug or Python member name (case-insensitive).
/// Optional alias map lets legacy slugs resolve (e.g. "linear" -> "Split").
pub fn find_enum(
    category: Category,
    slug_or_member: &str,
    namespace: Namespace,
    aliases: Option<&HashMap<String, String>>,
) -> Option<EnumEntry> {
    let want = aliases
        .and_then(|m| m.get(slug_or_member))
        .map(|s| s.as_str())
        .unwrap_or(slug_or_member)
        .to_ascii_lowercase();
    for e in list_enum(category, namespace) {
        if !e.slug.is_empty() && e.slug.to_ascii_lowercase() == want {
            return Some(e);
        }
        if e.member.to_ascii_lowercase() == want {
            return Some(e);
        }
    }
    None
}

/// All non-empty slugs for a category/namespace.
pub fn slugs_for(category: Category, namespace: Namespace) -> Vec<String> {
    list_enum(category, namespace)
        .into_iter()
        .filter_map(|e| if e.slug.is_empty() { None } else { Some(e.slug) })
        .collect()
}

// ---------------------------------------------------------------------------
// Typed convenience accessors (spec: get_transitions etc.)
// ---------------------------------------------------------------------------

pub fn transitions(namespace: Namespace) -> Vec<EnumEntry> {
    list_enum(Category::Transitions, namespace)
}

pub fn get_transitions(namespace: Namespace) -> Vec<EnumEntry> {
    transitions(namespace)
}

pub fn get_masks(namespace: Namespace) -> Vec<EnumEntry> {
    list_enum(Category::Masks, namespace)
}

pub fn masks(namespace: Namespace) -> Vec<EnumEntry> {
    get_masks(namespace)
}

pub fn get_text_anims(namespace: Namespace) -> Vec<EnumEntry> {
    let mut out = Vec::new();
    out.extend(list_enum(Category::TextIntros, namespace));
    out.extend(list_enum(Category::TextOutros, namespace));
    out.extend(list_enum(Category::TextLoopAnims, namespace));
    out
}

pub fn get_image_anims(namespace: Namespace) -> Vec<EnumEntry> {
    let mut out = Vec::new();
    out.extend(list_enum(Category::ImageIntros, namespace));
    out.extend(list_enum(Category::ImageOutros, namespace));
    out.extend(list_enum(Category::ImageCombos, namespace));
    out
}

pub fn get_scene_effects(namespace: Namespace) -> Vec<EnumEntry> {
    list_enum(Category::SceneEffects, namespace)
}

pub fn get_character_effects(namespace: Namespace) -> Vec<EnumEntry> {
    list_enum(Category::CharacterEffects, namespace)
}

pub fn get_audio_effects(namespace: Namespace) -> Vec<EnumEntry> {
    list_enum(Category::AudioEffects, namespace)
}

pub fn get_filters(namespace: Namespace) -> Vec<EnumEntry> {
    list_enum(Category::Filters, namespace)
}

pub fn get_fonts(namespace: Namespace) -> Vec<EnumEntry> {
    list_enum(Category::Fonts, namespace)
}

/// Slug -> entry map for a category/namespace (only entries with non-empty slug).
pub fn slug_map(category: Category, namespace: Namespace) -> HashMap<String, EnumEntry> {
    let mut map = HashMap::new();
    for e in list_enum(category, namespace) {
        if !e.slug.is_empty() && !map.contains_key(&e.slug) {
            map.insert(e.slug.clone(), e);
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Catalogue (mirrors catalogue.ts)
// ---------------------------------------------------------------------------

/// Every category `catalogue` searches: enums.json categories plus bubbles.
pub const SEARCHABLE_CATEGORIES: &[&str] = &[
    "transitions",
    "masks",
    "image_intros",
    "image_outros",
    "image_combos",
    "text_intros",
    "text_outros",
    "text_loop_anims",
    "scene_effects",
    "character_effects",
    "audio_effects",
    "fonts",
    "filters",
    "bubbles",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogueSource {
    #[serde(rename = "bundled")]
    Bundled,
    #[serde(rename = "user")]
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueMatch {
    pub category: String,
    pub slug: String,
    pub member: String,
    pub name: Option<String>,
    pub effect_id: Option<String>,
    pub resource_id: Option<String>,
    pub resource_type: Option<String>,
    pub source: CatalogueSource,
}

fn score_entry(entry: &EnumEntry, q: &str) -> Option<u32> {
    let mut texts: Vec<String> = Vec::new();
    for s in [&entry.slug, &entry.member] {
        if !s.is_empty() { texts.push(s.to_ascii_lowercase()); }
    }
    if let Some(n) = &entry.name { if !n.is_empty() { texts.push(n.to_ascii_lowercase()); } }
    if let Some(t) = &entry.title { if !t.is_empty() { texts.push(t.to_ascii_lowercase()); } }
    if texts.iter().any(|t| t == q) {
        return Some(0);
    }
    if entry.resource_id.as_deref() == Some(q) || entry.effect_id.as_deref() == Some(q) {
        return Some(0);
    }
    if texts.iter().any(|t| t.starts_with(q)) {
        return Some(1);
    }
    if texts.iter().any(|t| t.contains(q)) {
        return Some(2);
    }
    None
}

fn to_match(entry: &EnumEntry, category: &str, source: CatalogueSource) -> CatalogueMatch {
    CatalogueMatch {
        category: category.to_string(),
        slug: entry.slug.clone(),
        member: entry.member.clone(),
        name: entry
            .name
            .clone()
            .or_else(|| entry.title.clone()),
        effect_id: entry.effect_id.clone(),
        resource_id: entry.resource_id.clone(),
        resource_type: entry.resource_type.clone(),
        source,
    }
}

fn category_entries(
    category: &str,
    namespace: Namespace,
) -> Vec<(EnumEntry, CatalogueSource)> {
    // bubbles live in code catalogue (not in enums.json) — we treat as empty
    // bundled here; caller may extend if needed.
    if category == "bubbles" {
        return Vec::new();
    }
    let cat = match category.parse::<Category>() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let combined = list_enum(cat, namespace);
    let user_count = user_entries_for_category(cat, None).len();
    let bundled_count = combined.len().saturating_sub(user_count);
    combined
        .into_iter()
        .enumerate()
        .map(|(idx, entry)| {
            let source = if idx < bundled_count {
                CatalogueSource::Bundled
            } else {
                CatalogueSource::User
            };
            (entry, source)
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct CatalogueOptions {
    pub namespace: Option<Namespace>,
    pub kind: Option<String>,
    pub limit: Option<usize>,
}

/// Cross-category search (mirrors `searchCatalogue` in TS).
pub fn search_catalogue(query: &str, opts: CatalogueOptions) -> Vec<CatalogueMatch> {
    let namespace = opts.namespace.unwrap_or(Namespace::CapCut);
    let limit = opts.limit.unwrap_or(20);
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let categories: Vec<String> = if let Some(k) = opts.kind {
        vec![k]
    } else {
        SEARCHABLE_CATEGORIES.iter().map(|s| s.to_string()).collect()
    };
    let mut ranked: Vec<(CatalogueMatch, u32)> = Vec::new();
    for category in &categories {
        for (entry, source) in category_entries(category, namespace) {
            if let Some(score) = score_entry(&entry, &q) {
                ranked.push((to_match(&entry, category, source), score));
            }
        }
    }
    ranked.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| a.0.category.cmp(&b.0.category))
            .then_with(|| a.0.slug.cmp(&b.0.slug))
    });
    ranked.truncate(limit);
    ranked.into_iter().map(|(m, _)| m).collect()
}

// ---------------------------------------------------------------------------
// User enums (mirrors user-enums.ts)
// ---------------------------------------------------------------------------

pub const USER_ENUMS_ENV: &str = "CAPCUT_CLI_USER_ENUMS";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HarvestKind {
    #[serde(rename = "video_effects")]
    VideoEffects,
    #[serde(rename = "filters")]
    Filters,
    #[serde(rename = "transitions")]
    Transitions,
    #[serde(rename = "masks")]
    Masks,
    #[serde(rename = "audio_effects")]
    AudioEffects,
    #[serde(rename = "animations")]
    Animations,
    #[serde(rename = "bubbles")]
    Bubbles,
    #[serde(rename = "fonts")]
    Fonts,
}

impl HarvestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VideoEffects => "video_effects",
            Self::Filters => "filters",
            Self::Transitions => "transitions",
            Self::Masks => "masks",
            Self::AudioEffects => "audio_effects",
            Self::Animations => "animations",
            Self::Bubbles => "bubbles",
            Self::Fonts => "fonts",
        }
    }
}

impl std::str::FromStr for HarvestKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "video_effects" => Ok(Self::VideoEffects),
            "filters" => Ok(Self::Filters),
            "transitions" => Ok(Self::Transitions),
            "masks" => Ok(Self::Masks),
            "audio_effects" => Ok(Self::AudioEffects),
            "animations" => Ok(Self::Animations),
            "bubbles" => Ok(Self::Bubbles),
            "fonts" => Ok(Self::Fonts),
            _ => Err(format!("unknown harvest kind: {s}")),
        }
    }
}

pub const HARVEST_KINDS: &[HarvestKind] = &[
    HarvestKind::VideoEffects,
    HarvestKind::Filters,
    HarvestKind::Transitions,
    HarvestKind::Masks,
    HarvestKind::AudioEffects,
    HarvestKind::Animations,
    HarvestKind::Bubbles,
    HarvestKind::Fonts,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserEnumEntry {
    pub kind: HarvestKind,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harvested_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harvested_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserEnumsFile {
    version: u32,
    entries: Vec<UserEnumEntry>,
}

fn kind_to_category(kind: HarvestKind) -> Option<Category> {
    match kind {
        HarvestKind::VideoEffects => Some(Category::SceneEffects),
        HarvestKind::Filters => Some(Category::Filters),
        HarvestKind::Transitions => Some(Category::Transitions),
        HarvestKind::Masks => Some(Category::Masks),
        HarvestKind::AudioEffects => Some(Category::AudioEffects),
        HarvestKind::Animations | HarvestKind::Bubbles | HarvestKind::Fonts => None,
    }
}

pub fn user_enums_path(override_path: Option<&str>) -> PathBuf {
    if let Some(p) = override_path {
        return PathBuf::from(p);
    }
    if let Ok(env) = std::env::var(USER_ENUMS_ENV) {
        if !env.is_empty() {
            return PathBuf::from(env);
        }
    }
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_fallback_config_home()
        });
    config_home.join("capcut-cli").join("user-enums.json")
}

fn dirs_fallback_config_home() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config");
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        return PathBuf::from(up).join(".config");
    }
    PathBuf::from(".config")
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

pub fn load_user_enums(path: Option<&Path>) -> (Vec<UserEnumEntry>, Option<String>) {
    let p = match path {
        Some(p) => p.to_path_buf(),
        None => user_enums_path(None),
    };
    if !p.exists() {
        return (Vec::new(), None);
    }
    let raw = match std::fs::read_to_string(&p) {
        Ok(c) => c,
        Err(e) => return (Vec::new(), Some(format!("failed to read user enums: {e}"))),
    };
    let raw = strip_bom(&raw);
    match serde_json::from_str::<UserEnumsFile>(raw) {
        Ok(f) => (f.entries, None),
        Err(_) => {
            // Try loose parse: object with entries array
            match serde_json::from_str::<serde_json::Value>(raw) {
                Ok(v) => {
                    if let Some(arr) = v.get("entries").and_then(|e| e.as_array()) {
                        let entries: Vec<UserEnumEntry> = arr
                            .iter()
                            .filter_map(|e| serde_json::from_value::<UserEnumEntry>(e.clone()).ok())
                            .collect();
                        (entries, None)
                    } else {
                        (Vec::new(), Some("user enum catalogue has no entries[] array".to_string()))
                    }
                }
                Err(e) => (Vec::new(), Some(format!("user enum catalogue did not parse: {e}"))),
            }
        }
    }
}

/// User entries that belong to a bundled category as EnumEntry objects.
/// Only slug-carrying entries are returned.
pub fn user_entries_for_category(
    category: Category,
    path: Option<&Path>,
) -> Vec<EnumEntry> {
    let (entries, _) = load_user_enums(path);
    let mut out = Vec::new();
    for e in entries {
        if kind_to_category(e.kind) != Some(category) || e.slug.is_empty() {
            continue;
        }
        out.push(EnumEntry {
            member: if e.name.is_empty() { e.slug.clone() } else { e.name.clone() },
            slug: e.slug,
            name: Some(e.name),
            effect_id: e.effect_id,
            resource_id: e.resource_id,
            resource_type: e.resource_type,
            ..Default::default()
        });
    }
    out
}

impl Default for EnumEntry {
    fn default() -> Self {
        Self {
            member: String::new(),
            slug: String::new(),
            name: None,
            title: None,
            effect_id: None,
            resource_id: None,
            md5: None,
            default_duration: None,
            duration: None,
            is_overlap: None,
            is_vip: None,
            resource_type: None,
            default_aspect_ratio: None,
        }
    }
}

/// Every effect_id/resource_id in the user catalogue.
pub fn all_user_enum_ids(path: Option<&Path>) -> HashSet<String> {
    let (entries, _) = load_user_enums(path);
    let mut ids = HashSet::new();
    for e in entries {
        if let Some(id) = e.effect_id { ids.insert(id); }
        if let Some(id) = e.resource_id { ids.insert(id); }
    }
    ids
}

pub fn slugify(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_dash = true;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    // trim leading/trailing dashes
    let trimmed = out.trim_matches('-').to_string();
    trimmed
}

/// Merge candidates into the catalogue file (created if absent).
pub fn merge_user_enums(
    path: &Path,
    candidates: Vec<UserEnumEntry>,
) -> std::io::Result<(usize, usize, usize)> {
    let (existing, _) = load_user_enums(Some(path));
    let mut seen: HashSet<String> = existing
        .iter()
        .map(|e| format!("{}|{}", e.effect_id.as_deref().unwrap_or(""), e.resource_id.as_deref().unwrap_or("")))
        .collect();
    let mut merged = existing;
    let mut added = 0usize;
    let mut duplicates = 0usize;
    let stamp = chrono_stamp();
    for mut c in candidates {
        let key = format!("{}|{}", c.effect_id.as_deref().unwrap_or(""), c.resource_id.as_deref().unwrap_or(""));
        if seen.contains(&key) {
            duplicates += 1;
            continue;
        }
        seen.insert(key);
        c.harvested_at = Some(stamp.clone());
        merged.push(c);
        added += 1;
    }
    if added > 0 {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = UserEnumsFile { version: 1, entries: merged.clone() };
        let json = serde_json::to_string_pretty(&file).unwrap();
        // atomic write
        let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&tmp, format!("{json}\n"))?;
        std::fs::rename(&tmp, path)?;
    }
    let total = merged.len();
    Ok((added, duplicates, total))
}

fn chrono_stamp() -> String {
    // Cheap ISO8601 without extra dep: use SystemTime
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as seconds since epoch string (good enough; TS uses toISOString)
    // Use a simple RFC3339-like string
    format!("{now}")
}

/// Validate a hand-registered entry (--add).
pub fn plan_manual_entry(
    kind: &str,
    slug: &str,
    resource_id: &str,
    effect_id: Option<&str>,
    path: Option<&Path>,
) -> Result<UserEnumEntry, String> {
    let hk: HarvestKind = kind.parse().map_err(|_| {
        let writable: Vec<_> = [HarvestKind::VideoEffects, HarvestKind::Filters, HarvestKind::Transitions, HarvestKind::Masks, HarvestKind::AudioEffects]
            .iter().map(|k| k.as_str()).collect();
        format!("Unknown kind \"{kind}\". Writable kinds: {}.", writable.join(", "))
    })?;
    // id-only kinds refuse
    match hk {
        HarvestKind::Animations => return Err("Kind \"animations\" is id-only: intro/outro/combo and image/text animations cannot be told apart, so a slug could write the wrong material shape. Harvest a draft that uses it instead.".to_string()),
        HarvestKind::Bubbles => return Err("Kind \"bubbles\" is id-only: bubbles live in materials.filters but are not filters — a bubble slug would write the wrong material shape. Harvest a draft instead.".to_string()),
        HarvestKind::Fonts => return Err("Kind \"fonts\" is id-only: font ids are nameless and have no writable category. Harvest a draft instead.".to_string()),
        _ => {}
    }
    if slug.is_empty() || slugify(slug) != slug {
        let cleaned = slugify(slug);
        if cleaned.is_empty() {
            return Err(format!("Slug \"{slug}\" has no ascii-kebab form. Pick an ascii slug (e.g. \"snow-fly\")."));
        } else {
            return Err(format!("Slug \"{slug}\" does not slugify clean. Use \"{cleaned}\"."));
        }
    }
    if resource_id.is_empty() {
        return Err("Missing <resource-id>.".to_string());
    }
    let ids: Vec<&str> = if let Some(eid) = effect_id { vec![resource_id, eid] } else { vec![resource_id] };
    let (existing, _) = load_user_enums(path);
    for ex in &existing {
        for id in &ids {
            if ex.effect_id.as_deref() == Some(*id) || ex.resource_id.as_deref() == Some(*id) {
                return Err(format!(
                    "Id {id} is already registered to {}/{} in {}.",
                    ex.kind.as_str(),
                    if ex.slug.is_empty() { "(id-only)" } else { &ex.slug },
                    path.map(|p| p.display().to_string()).unwrap_or_else(|| user_enums_path(None).display().to_string())
                ));
            }
        }
    }
    Ok(UserEnumEntry {
        kind: hk,
        slug: slug.to_string(),
        name: slug.to_string(),
        effect_id: effect_id.map(|s| s.to_string()),
        resource_id: Some(resource_id.to_string()),
        resource_type: None,
        harvested_from: Some("manual".to_string()),
        harvested_at: None,
    })
}
