# Floword capability retention audit

Status vocabulary: `RETAINED`, `PARTIAL`, `MISSING`, `BLOCKED`, `INTERACTIVE_ONLY`.

| Domain | Capability | Status | New Floword access | Runtime status / evidence |
|---|---|---|---|---|
| Workspace | One workspace without service navigation | RETAINED | Floword main screen + Configure drawer | Source inspection: no service-specific page added |
| Content Acquisition | Five acquisition modes (Prompt, Trend Research, Web Story, Video URL, Local Media) with deterministic auto-resolution | RETAINED | Project Brief -> Content Source | Fully typed Rust `ContentSource` enum with deterministic resolution, input validations, and unit tests passing |
| Content Acquisition | Web Story / Article extraction | RETAINED | Project Brief -> Web Story / Article | Generic URL article fetcher + sanitizer producing `source_text` artifact registered in `ArtifactStore` and consumed by `StoryScript` |
| Pipeline Orchestration | Dependency-scoped checks (removal of global LLM preflight) | RETAINED | Automatic pipeline worker | Service preflight moved into business stages; OmniRoute failure isolates to `story_script=failed` while downstream stays `pending` |
| OmniRoute | Live model selection & degraded catalog state | RETAINED | Project Brief -> AI Model | `check_omniroute` reports degraded on timeout without crashing; UI shows degraded notice with retry button |
| Pipeline State | Per-job stage state isolation & reset | RETAINED | Pipeline Progress | Initial stage states returned for new jobs; frontend resets `stepRuns` on enqueue without stale stage pollution |
| Research | Explicit enabled/platform/query/mode/variant contract | RETAINED | Project Brief -> Research; Configure -> Research Session | Xiaohongshu Mainland (default) and RedNote International configured in UI, persisted in PipelineContext, DTOs, and session states; Rust tests (13/13) and Python adapter tests (11/11) pass |
| Research | Research independent of source URL | RETAINED | Automatic research stage | Live CDP authenticated search on RedNote international produced 20 content notes + 424 comments; partial enrichment handled on timeout; real artifact registered in ArtifactStore and handed off to StoryScript stage on 2026-08-10 |
| Research | Supported platform/mode truth | RETAINED | Project Brief -> Research | Live MediaCrawler `/api/config/platforms` + `/api/config/options` catalogs; Python tests verify TikTok rejection, mode validation, and variant isolation (11/11 pass) |
| Research | Structured errors and recovery | RETAINED | Run Console -> Errors -> Login Now / Skip Research | `RESEARCH_AUTH_REQUIRED` blocks before crawl; same-job retry and explicit skip are wired; live platform classifications remain externally blocked |
| Research | Browser login, real verify, persisted CDP profile, reconnect, clear | RETAINED | Configure -> Research Session | Verified at runtime on 2026-08-10 with real CDP session (RedNote international); pong verify returned CONNECTED; live crawl executed and validated |
| Research | QR login lifecycle | PARTIAL | Configure -> Research Session -> QR Code | Reuses core QR/CDP login and polls lifecycle status; browser handles QR display, but a normalized QR image payload is not returned to Floword |
| Research | Phone login | BLOCKED | Configure explains unavailability | Core requires the external Redis SMS-code workflow and crawler manager does not expose a usable verification-code exchange |
| Research | Cookie compatibility fallback | PARTIAL | Configure -> Research Session -> Cookie compatibility fallback | Optional `MEDIACRAWLER_COOKIES` fallback verifies and persists into canonical profile; raw value is never returned/stored by Floword and command logging is redacted; secure interactive import is not implemented |
| Youwee | Single URL acquisition with browser/file cookie settings | RETAINED | Project Brief source + Configure settings + automatic ingest | Existing canonical downloader/ArtifactStore path preserved |
| Youwee | Playlist/format/audio/channel/history breadth | PARTIAL | None or legacy UI only | Not yet mapped into business-domain Configure controls |
| Vynaro | Probe, scenes, audio extraction | RETAINED | Automatic ingest/analyze | Existing typed artifacts and fixture test retained |
| Vynaro | Advanced analysis options | PARTIAL | Configure status only | Threshold/options not exposed |
| Story | Story plan -> script request -> OmniRoute script | RETAINED | Automatic story/script stage | Existing real artifact chain preserved for source-based workflow |
| Story | Advanced planning/revision/project controls | PARTIAL | None | Required subset still needs product mapping |
| Voice | OmniRoute TTS audio and timing | PARTIAL | Project Brief/automatic voice | Runtime path exists; configured live voice not verified in this change |
| OpenMontage | Timeline and captions | RETAINED | Automatic media/timeline stage | Existing adapter and artifact validation preserved |
| OpenMontage | Advanced composition controls | PARTIAL | Configure status only | Music/effects/transitions/template controls not mapped |
| Original creation | Prompt-only script | RETAINED | Project Brief -> Original creation | A source-free Original request produced and persisted a valid scene plan in the Rust runtime on 2026-08-10 |
| Original creation | Real generated visual assets into timeline | BLOCKED | Automatic Media / Timeline substage; Configure -> AI / Visual Generation | Existing ArtCraft account login is reused from the Tauri HTTP cookie store. Provider authentication and catalog discovery passed, and the request reached `handle_artcraft_video.rs`; ArtCraft rejected task submission with HTTP 402 Payment Required, so no output file or ArtifactStore registration exists yet |
| CapCut | Verified desktop draft publication | BLOCKED | Automatic final stage / CapCut app | Voice, captions, OpenMontage, and CapCut precondition were not executed because the required real visual artifact was blocked by the external ArtCraft account billing state |
| Observability | Structured stage error in job state | RETAINED | Run Console -> Errors | Existing `StageError` contract and polling preserved |

## Retention gate

- Safe to hide: none.
- Do not hide: OmniRoute, MediaCrawler, Youwee, Vynaro, Story Studio, OpenMontage legacy entry points until every required `PARTIAL`/`MISSING` row has runtime evidence.
