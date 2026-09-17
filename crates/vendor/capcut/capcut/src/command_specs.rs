//! Command specs shared between CLI and MCP — mirrors reference/src/command-specs.ts
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    pub name: String,
    pub usage: String,
    pub summary: String,
    pub category: String,
}

fn spec(name: &str, usage: &str, summary: &str, category: &str) -> CommandSpec {
    CommandSpec { name: name.into(), usage: usage.into(), summary: summary.into(), category: category.into() }
}

pub fn all_specs() -> Vec<CommandSpec> {
    vec![
        // draft
        spec("init", "capcut init <name> [--template <dir>] [--drafts <dir>] [--ratio <r> | --width <px> --height <px>]", "Create a new empty draft from template", "draft"),
        spec("info", "capcut info <project>", "Project overview + material summary", "draft"),
        spec("validate", "capcut validate <project>", "Validate timeline (alias for lint)", "draft"),
        spec("lint", "capcut lint <project> [options]", "Schema-aware checks", "draft"),
        spec("fix", "capcut fix <project> [--dry-run]", "Auto-repair fixable lint issues", "draft"),
        spec("version", "capcut version <project>", "Detect CapCut/JianYing version + schema flags", "draft"),
        spec("quickstart", "capcut quickstart <name> [--video <f>] [--audio <f>] [--srt <f>] [--drafts <dir>] [--ratio <r>]", "One-command first draft", "draft"),
        spec("compile", "capcut compile <spec.json> [--out <draftdir>] [--data <rows.jsonl|->]", "Build draft from declarative JSON spec", "draft"),
        // browse
        spec("tracks", "capcut tracks <project>", "List all tracks", "browse"),
        spec("segments", "capcut segments <project> [--track <type>]", "List segments with timing", "browse"),
        spec("texts", "capcut texts <project>", "List all text/subtitle content", "browse"),
        spec("segment", "capcut segment <project> <id>", "Full detail for one segment + its material", "browse"),
        spec("material", "capcut material <project> <id>", "Full detail for one material", "browse"),
        spec("materials", "capcut materials <project> [--type <type>]", "List materials", "browse"),
        spec("timeline", "capcut timeline <project> [--cols <n>]", "Pretty timeline view", "browse"),
        spec("projects", "capcut projects [query] [--drafts <path>]", "List projects in draft store", "browse"),
        spec("diff", "capcut diff <project-a> <project-b>", "Diff two projects", "browse"),
        spec("concat", "capcut concat <project-a> <project-b> [--out <path>]", "Concatenate two timelines", "browse"),
        // track ops
        spec("add-video", "capcut add-video <project> <file> <start> [duration] [options]", "Add video/image segment", "track"),
        spec("add-audio", "capcut add-audio <project> <file> <start> [duration] [options]", "Add audio segment", "track"),
        spec("add-text", "capcut add-text <project> <start> <duration> <text> [options]", "Add text segment", "track"),
        spec("add-sticker", "capcut add-sticker <project> <resource-id> <start> <duration>", "Add sticker", "track"),
        spec("add-cover", "capcut add-cover <project> <image> [--time <ms>]", "Add cover image", "track"),
        spec("trim", "capcut trim <project> <id> <start> <duration>", "Trim segment source range", "track"),
        spec("cut", "capcut cut <project> <start> <end> --out <path>", "Cut range to new draft", "track"),
        spec("speed", "capcut speed <project> <id> <multiplier>", "Set segment playback speed", "track"),
        spec("volume", "capcut volume <project> <id> <level>", "Set segment volume", "track"),
        spec("opacity", "capcut opacity <project> <id> <alpha>", "Set segment opacity", "track"),
        spec("crop", "capcut crop <project> <segment-id> [--ratio <r> | --rect <x,y,w,h> | --reset]", "Crop video segment", "track"),
        spec("shift", "capcut shift <project> <id> <offset>", "Shift single segment", "track"),
        spec("shift-all", "capcut shift-all <project> <offset> [--track <type>]", "Shift all segments", "track"),
        spec("duplicate", "capcut duplicate <project> <segment-id> [--track <name>] [--new-track]", "Duplicate segment", "track"),
        spec("remove", "capcut remove <project> <segment-id> [--keep-track] [--keep-materials]", "Remove segment", "track"),
        spec("replace-media", "capcut replace-media <project> <segment-id> <new-file> [--retime]", "Replace segment media", "track"),
        spec("prune", "capcut prune <project>", "Remove orphan materials", "track"),
        spec("relink", "capcut relink <project> (--dir <path> | --from <prefix> --to <prefix>)", "Relink media paths", "track"),
        spec("rename", "capcut rename <project> <new-name>", "Rename project", "track"),
        // text
        spec("set-text", "capcut set-text <project> <id> <text>", "Update text material content", "text"),
        spec("text-style", "capcut text-style <project> <id> [options]", "Update text style", "text"),
        spec("text-ranges", "capcut text-ranges <project> <id> --styles <json>", "Set per-range text styles", "text"),
        spec("text-anim", "capcut text-anim <project> <id> [options]", "Apply text animation", "text"),
        spec("image-anim", "capcut image-anim <project> <id> [options]", "Apply image animation", "text"),
        spec("bubble-text", "capcut bubble-text <project> <id> --bubble <slug>", "Apply bubble style", "text"),
        // media
        spec("probe", "capcut probe <file>", "Probe media via ffprobe", "media"),
        spec("render", "capcut render <project> [--out <preview.mp4>] [options]", "Render ffmpeg proxy preview", "media"),
        spec("export", "capcut export <drafts-dir> --batch [options]", "Batch export drafts", "media"),
        spec("export-timeline", "capcut export-timeline <project> [--out <file.otio>]", "Export OTIO timeline", "media"),
        spec("import-timeline", "capcut import-timeline <file.otio> (--out <new-project> | --into <project>)", "Import OTIO timeline", "media"),
        spec("detect-scenes", "capcut detect-scenes <video> [options]", "Detect scene changes", "media"),
        spec("detect-silence", "capcut detect-silence <media> [options]", "Detect silence spans", "media"),
        spec("detect-retakes", "capcut detect-retakes <project> [--window <s>] [--similarity <n>] | --srt <file>", "Find repeated takes", "media"),
        spec("batch", "capcut batch <project> < operations.jsonl", "Apply JSONL operations", "media"),
        // effects
        spec("add-filter", "capcut add-filter <project> <slug> (<start> <duration> | --full)", "Add filter", "effects"),
        spec("add-effect", "capcut add-effect <project> <slug> (<start> <duration> | --full) [options]", "Add effect", "effects"),
        spec("transition", "capcut transition <project> <id> <slug> [--duration <time>]", "Set transition", "effects"),
        spec("mask", "capcut mask <project> <id> <slug> | --off", "Apply mask", "effects"),
        spec("bg-blur", "capcut bg-blur <project> <id> <level> | --off", "Background blur", "effects"),
        spec("chroma", "capcut chroma <project> <id> (--color <hex> | --off)", "Chroma key", "effects"),
        spec("matting", "capcut matting <project> <id> [--off]", "AI matting", "effects"),
        spec("mix-mode", "capcut mix-mode <project> <id> <mode>", "Set blend mode", "effects"),
        spec("audio-fade", "capcut audio-fade <project> <id> [--in <s>] [--fade-out <s>]", "Audio fade", "effects"),
        spec("keyframe", "capcut keyframe <project> <id> <property> <time> <value>", "Add keyframe", "effects"),
        spec("make-preset", "capcut make-preset <project> <text-segment-id> --out <preset.json>", "Save text preset", "effects"),
        spec("save-template", "capcut save-template <project> <id> <name> --out <path>", "Save template", "effects"),
        spec("apply-template", "capcut apply-template <project> <template> <start> <duration>", "Apply template", "effects"),
        // srt/ass
        spec("import-srt", "capcut import-srt <project> <srt-or-> [options]", "Import SRT captions", "srt"),
        spec("export-srt", "capcut export-srt <project> [--out <file.srt>]", "Export SRT", "srt"),
        spec("import-ass", "capcut import-ass <project> <ass-or-> [options]", "Import ASS", "srt"),
        spec("export-ass", "capcut export-ass <project> [--karaoke] [--out <file.ass>]", "Export ASS", "srt"),
        spec("caption", "capcut caption <project> (--audio <path> | --from-segment <id>)", "Generate captions", "srt"),
        spec("translate", "capcut translate <project> --to <language> --out <path>", "Translate captions", "srt"),
        // wikimedia/sfx/tts
        spec("add-sfx", "capcut add-sfx <project> <slug> <start> <duration>", "Add sound effect", "media"),
        spec("wikimedia", "capcut wikimedia <query> [--limit <n>]", "Search Wikimedia Commons", "media"),
        spec("tts", "capcut tts <project> [start] [duration] (--text <s> | --text-file <path>) --tts-cmd <template>", "Synthesize voiceover", "media"),
        // ops
        spec("doctor", "capcut doctor", "Check CapCut installation & draft store", "ops"),
        spec("diagnose", "capcut diagnose <project> [--bundle <report.json>]", "Diagnose draft bundle", "ops"),
        spec("decrypt", "capcut decrypt <project-or-file>", "Decrypt CapCut file", "ops"),
        spec("serve", "capcut serve [--queue <path>] [options]", "Start queue server", "ops"),
        spec("register", "capcut register <project-dir> [--apply] [--materials]", "Register draft in store index", "ops"),
        spec("sync-timelines", "capcut sync-timelines <project-dir> [--nested] [--apply]", "Sync nested Timelines/", "ops"),
        spec("restore", "capcut restore <project> [--step <n> | --list]", "Restore from backup", "ops"),
        spec("fixture", "capcut fixture <project> --out <dir> [--check]", "Create fixture bundle", "ops"),
        spec("migrate", "capcut migrate <project> (--from <v> --to <v> | --like <project>)", "Migrate schema version", "ops"),
        spec("describe", "capcut describe <project>", "Describe draft as JSON spec", "ops"),
        // meta
        spec("config", "capcut config", "Show resolved config", "meta"),
        spec("completions", "capcut completions <bash|zsh|fish>", "Shell completions", "meta"),
        spec("enums", "capcut enums <category-flag> [--jianying]", "List enum slugs", "meta"),
        spec("catalogue", "capcut catalogue <query> [--kind <category>] [--limit <n>]", "Search enum catalogue", "meta"),
        spec("harvest-enums", "capcut harvest-enums [<project> | --sync | --add <kind> <slug> <resource-id>]", "Harvest enum resource ids", "meta"),
        spec("probe-alias", "capcut probe <file>", "Probe media (alias)", "media"),
        spec("auto-edit", "capcut auto-edit --video <f> --out-dir <dir> [options]", "LLM auto-edit harness", "ops"),
    ]
}

pub fn command_names() -> Vec<String> {
    all_specs().into_iter().map(|s| s.name).collect()
}
