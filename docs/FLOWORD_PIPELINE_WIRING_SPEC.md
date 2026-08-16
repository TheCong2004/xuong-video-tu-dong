# Floword Pipeline Wiring Specification

## Canonical runtime

| Concern | Owner | Rule |
|---|---|---|
| Orchestration/state machine | ArtCraft Rust pipeline worker | No second orchestrator |
| Job persistence | Existing SQLite `pipeline_jobs` | Keep global `TaskStatus` |
| Artifact persistence/validation | Existing Rust `ArtifactStore` | No second artifact store |
| Service access | Unified Backend/adapters | No competing pipeline state machine |
| UI | Floword React | Enqueue, observe, cancel/retry, render only |

`PipelineStage` remains the backward-compatible low-level worker state for the current OmniRoute → CapCut implementation. `StageId` is the canonical seven-stage business contract exposed to future stage state/events.

## Business-stage contract

| Order | `stage_id` | Dependencies | Optional | Canonical service ownership | Expected handoff |
|---:|---|---|---|---|---|
| 1 | `input` | — | No | Rust pipeline | validated `PipelineContext` |
| 2 | `ingest_analyze` | `input` | No | Youwee, Vynaro | `source_video`, `source_metadata`, `scenes`, `source_audio` |
| 3 | `research` | `input` | Yes | MediaCrawler | `research` or `skipped`; source artifacts are optional context |
| 4 | `story_script` | `ingest_analyze`; `research` when completed | No | Story Studio, OmniRoute | `story`, `script_request`, `script` |
| 5 | `voice` | `story_script` | Workflow-dependent | TTS | `voice_audio`, `voice_timing` |
| 6 | `media_timeline` | `voice` or explicit voice skip | No | OpenMontage | `timeline`, `captions` |
| 7 | `capcut` | `media_timeline` | No | CapCut Backend | `capcut_draft`, optional `rendered_video` |

Canonical `StageStatus` values: `pending`, `running`, `retrying`, `completed`, `skipped`, `failed`, `cancelled`.

`StageState` fields:

| Field | Contract |
|---|---|
| `stage_id` | One canonical business `StageId` |
| `status` | Canonical `StageStatus` |
| `attempt` | Starts at `0`; increments when entering `running` |
| `started_at`, `finished_at` | ISO-8601 strings; absent until applicable |
| `input_artifact_ids`, `output_artifact_ids` | Canonical ArtifactStore IDs |
| `service` | Runtime implementation detail; never the stage ID |
| `error` | `{code, message, retryable}` only; no secret or stack trace |

Valid lifecycle transitions:

| Action | From | To |
|---|---|---|
| start | `pending`, `retrying` | `running` |
| complete | `running` | `completed` |
| skip | `pending` | `skipped` |
| fail | `running`, `retrying` | `failed` |
| retry | retryable `failed` | `retrying` |
| cancel | `pending`, `running`, `retrying` | `cancelled` |

## Pipeline context

The typed Rust `PipelineContext` is the single logical handoff context for future phases:

| Category | Fields |
|---|---|
| Identity | `job_id`, optional `project_id` |
| Workflow | `workflow_mode`, `content_source` (`auto \| prompt_only \| trend_research \| web_story \| video_url \| local_media`), `prompt`, optional `model_id`, optional `voice_id`, `language`, `target_duration_seconds`, `output_mode` |
| Source | optional `source_url`, optional `local_file`, optional `story_url` |
| Research | `research_enabled`, optional `research_platform`, optional `research_query`, optional `research_mode`, optional `xhs_variant` |
| State | `artifact_refs`, seven `stage_states` |

Context is persisted through the existing pipeline job/output mechanism when runtime wiring begins. No separate database or frontend-owned context is permitted.

### Content Source Resolution (Deterministic)

When `content_source = auto` (or omitted), resolution order is deterministic:
1. `local_file` is present -> `local_media`
2. `story_url` or non-media web URL -> `web_story`
3. Direct video URL (`is_direct_video_or_media_url` = true) -> `video_url`
4. `research_enabled = true` and `research_query` is present -> `trend_research`
5. Otherwise -> `prompt_only`

## Artifact contract

| `ArtifactKind` | Producer stage | Minimum validation |
|---|---|---|
| `source_video` | `ingest_analyze` | file exists, regular, size > 0 |
| `source_metadata` | `ingest_analyze` | non-empty valid JSON |
| `source_audio` | `ingest_analyze` | file exists, size > 0; duration when probe exists |
| `source_text` | `ingest_analyze` | non-empty valid JSON (url, title, author, text, retrieved_at) |
| `scenes` | `ingest_analyze` | non-empty valid JSON |
| `research` | `research` | non-empty valid JSON |
| `story` | `story_script` | non-empty valid JSON |
| `script_request` | `story_script` | non-empty valid JSON |
| `script` | `story_script` | non-empty valid JSON |
| `voice_audio` | `voice` | file exists, size > 0; duration when probe exists |
| `voice_timing` | `voice` | non-empty valid JSON |
| `captions` | `media_timeline` | non-empty valid JSON |
| `timeline` | `media_timeline` | non-empty valid JSON |
| `capcut_draft` | `capcut` | draft save and verify pass; registered manifest exists |
| `rendered_video` | `capcut` | file exists, regular, size > 0 |

`ArtifactRef` contains `artifact_id`, `kind`, `produced_by_stage`, `location`, optional `mime_type`, and metadata. Stage handoff uses `artifact_id`; adapters resolve local paths internally. Existing `FlowordArtifact` remains the canonical physical record and maps to this typed reference.

## Dependency-Aware Orchestration (No Global Preflight Hard-Block)

Global LLM preflight during startup is removed. Each business stage is responsible for checking only its owned dependencies:
- **Research Stage**: MediaCrawler session and browser/crawler readiness.
- **Ingest/Analyze Stage**: Youwee / Vynaro / Web story reader.
- **Story / Script Stage**: Story Studio and OmniRoute readiness (`READY | DEGRADED | UNAVAILABLE`). If OmniRoute fails, `story_script` enters `failed`, while downstream stages (`voice`, `media_timeline`, `capcut`) remain `pending`.
- **Voice Stage**: TTS synthesis dependency.
- **Media Timeline Stage**: OpenMontage and required visual artifacts.
- **CapCut Stage**: CapCut backend and draft verification.

## Job State Isolation and Reset

On workflow enqueue, frontend and backend ensure clean stage state isolation:
- `setStepRuns` resets all step states to clean initial values.
- New jobs return initial stage states (`pending`), preventing stale failures or successes from earlier jobs from polluting Pipeline Progress.

## Retry, cancellation, resume, idempotency

| Concern | Rule |
|---|---|
| Retry | Per stage; small configured maximum; only structured `retryable=true` failures |
| Non-retryable | Invalid input/file/config, authentication failure, unsupported format |
| Cancellation | Check before stage, after external call, inside polling, and before retry; stop child process when safely supported |
| Resume | Find first incomplete stage whose required artifacts are absent/invalid; reuse valid upstream artifacts |
| Invalidation | Invalid upstream artifact invalidates dependent downstream state/artifacts |
| Idempotency | Reuse valid output where policy permits; otherwise replace/version through ArtifactStore semantics |
| Secrets | Never serialize credentials, provider secrets, or stack traces into context/events |

## Research session and preflight

- Account lifecycle is owned by the unified MediaCrawler adapter and the existing MediaCrawler CDP profile.
- `login` and `verify` worker actions reuse each platform crawler/client and its real `pong()` check, then stop before crawling.
- Floword persists only safe metadata: platform, auth method, deterministic profile reference, status, and verification time. Browser cookies remain in the MediaCrawler profile.
- When research is enabled, the adapter verifies the explicit platform session before starting a crawl. Failure returns `RESEARCH_AUTH_REQUIRED`; the crawler is not called.
- `Login Now` opens Configure for the job's research platform. Retry resumes the same job. `Skip Research` disables research in the persisted pipeline context, marks the stage skipped, and resumes the same job without a fabricated artifact.
- Clear Session deletes only the exact local MediaCrawler profile and safe metadata; it does not claim remote account logout.

## Stage events

Canonical event data supports:

```json
{
  "job_id": "…",
  "stage": { "stage_id": "voice", "status": "running", "attempt": 1 },
  "progress": 60,
  "message": "Generating narration"
}
```

The UI displays the business `stage_id`; `service` is optional detail only.

## Phase wiring and tests

| Phase | Wiring | Required tests | Completion gate |
|---:|---|---|---|
| 0 | Contracts only | lifecycle, skip, failure, retry, cancel, artifact dependency, serde, existing worker tests | contracts compile and current pipeline unchanged |
| 1 | Youwee → Vynaro | local fixture; supported URL runtime when available | real source, metadata, scenes/audio artifacts |
| 2 | MediaCrawler | disabled skip; enabled result; auth classification | real `research` or `BLOCKED_AUTH` |
| 3 | Story Studio → OmniRoute | structured inputs; OmniRoute-only provider call | real story, request, script artifacts |
| 4 | TTS | real synthesis and validation; explicit skip | real audio and timing |
| 5 | OpenMontage | minimum video/voice/caption timeline | valid timeline and captions |
| 6 | CapCut | current draft regression; optional render | verified draft; optional real rendered video |
| 7 | hardening/E2E | retry/cancel/resume/idempotency plus two canonical E2E cases | production lifecycle passes |

## Branches

| Condition | Behavior |
|---|---|
| `source_url` | Youwee acquisition, then Vynaro |
| `local_file` | Validate/register source; skip Youwee acquisition; run Vynaro |
| `research_enabled=false` | `research=skipped`; `story_script` continues |
| `research_enabled=true`, no source | MediaCrawler runs from explicit platform/query/mode and may produce a research artifact before prompt-only generation |
| TTS explicitly disabled | `voice=skipped`; downstream must accept the declared branch |
| `output_mode=draft_only` | verified draft → job `draft_ready`; no render |
| `output_mode=render_video` | verified draft → render → job `completed` |

## Canonical E2E acceptance

Case 1: URL or local fixture, Vietnamese 30-second storytelling prompt, research disabled, CapCut draft. All required artifacts pass; `research=skipped`; final job is `draft_ready`.

Case 2: same workflow with research enabled. A real `research` artifact passes, or missing MediaCrawler login is reported as `BLOCKED_AUTH`; never fake completion.

## Completion matrix

| Capability | Phase | Contract | Runtime | E2E |
|---|---:|---|---|---|
| Business stage lifecycle | 0 | Required | Later phases | Phase 7 |
| Artifact reference/dependency | 0 | Required | Later phases | Phase 7 |
| Stage serialization/event payload | 0 | Required | Later phases | Phase 7 |
| Youwee/Vynaro | 1 | Specified | Pending | Pending |
| MediaCrawler | 2 | Specified | Pending | Pending |
| Story Studio/OmniRoute | 3 | Specified | Pending | Pending |
| TTS | 4 | Specified | Pending | Pending |
| OpenMontage | 5 | Specified | Pending | Pending |
| CapCut | 6 | Existing path preserved | Existing partial | Pending |
| Retry/cancel/resume hardening | 7 | Specified | Pending | Pending |
