# ArtCraft / Floword Studio Architecture

## 1. Mục tiêu

ArtCraft được tổ chức theo hướng:

- `Floword Studio` là UI trung tâm để điều phối AI, media, voice, workflow, job, artifact và các service phụ.
- `CapCut Automation` giữ UI riêng để chỉnh sâu draft, timeline, track, caption và render.
- Toàn bộ UI gọi qua một `Unified Backend Gateway`.
- `OmniRoute` là trung tâm quản lý AI provider, API key, model, routing và usage.
- Các engine như FFmpeg, TTS, Playwright/CDP, CapCut Mate chạy phía sau backend.
- Không để frontend gọi trực tiếp từng port/service con.

---

## 2. Lệnh chạy

### Dev — FE + Tauri + BE process riêng

```powershell
cd D:\capcutpolot\artcraft
.\script\artcraft\windows_capcut_dev.ps1
```

### Build `.exe` + backend

```powershell
cd D:\capcutpolot\artcraft
.\script\artcraft\windows_build.ps1
```

Mục tiêu là người dùng chỉ cần 1 lệnh cho dev và 1 lệnh cho build, không phải tự mở nhiều terminal.

---

## 3. Kiến trúc tổng thể
INPUT
  │
  ▼
ingest_analyze
  │
  ├─ local file ───────────────┐
  │                            │
  └─ URL → Youwee              │
               │               │
               └→ source_video │
                               ▼
                            Vynaro
                               │
                    ┌──────────┼──────────┐
                    ▼          ▼          ▼
            source_metadata   scenes   source_audio
                    │
                    ▼
                 research
                    │
        research_enabled=false
             → SKIPPED
                    │
                    └── true → MediaCrawler → research
                                             │
                                             ▼
                                        story_script
                                             │
           prompt + metadata + scenes + optional research
                                             │
                                             ▼
                                        Story Studio
                                             │
                                   story + script_request
                                             │
                                             ▼
                                          OmniRoute
                                             │
                                             ▼
                                           script
                                             │
                                             ▼
                                            voice
                                             │
                              script + voice + language
                                             │
                                             ▼
                                             TTS
                                             │
                                voice_audio + voice_timing
                                             │
                                             ▼
                                      media_timeline
                                             │
          source_video + scenes + script + voice_audio + timing
                                             │
                                             ▼
                                        OpenMontage
                                             │
                                   timeline + captions
                                             │
                                             ▼
                                            capcut
                                             │
        source_video + voice_audio + timeline + captions
                                             │
                                             ▼
                                      CapCut Backend
                                             │
                                      capcut_draft
                                      ├─ draft_only
                                      │    → draft_ready
                                      │
                                      └─ render_video
                                           → rendered_video
                                           → completed