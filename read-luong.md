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

```text
                    ARTCRAFT / TAURI 2
                           │
            ┌──────────────┴──────────────┐
            │                             │
            ▼                             ▼
     FLOWORD STUDIO              CAPCUT AUTOMATION
       UI TRUNG TÂM                   UI RIÊNG
            │                             │
            └──────────────┬──────────────┘
                           │
                           ▼
                 UNIFIED BACKEND :30000
                           │
          ┌────────────────┼────────────────┐
          │                │                │
          ▼                ▼                ▼
       Pipeline        Infrastructure     Adapters
          │                │                │
          │           Job Manager           ├─ OmniRoute
          │           ArtifactStore         ├─ FFmpeg
          │           EventBus              ├─ TTS
          │           SQLite                ├─ Playwright
          │                                 └─ CapCut Mate
          │
          ▼
      Media → AI → Voice → Caption → Timeline
          │
          ▼
      CapCut Engine
          │
      ┌───┴────┐
      ▼        ▼
    Draft    Render
```

---

## 4. Floword Studio

Floword Studio là một workspace tổng hợp duy nhất. Capability và backend service chạy phía sau pipeline, không phải các mini-app hoặc page navigation riêng.

```text
FLOWORD STUDIO
└── ONE WORKSPACE
    ├── Project Brief
    ├── Pipeline Progress
    ├── Run Console
    │   ├── Progress
    │   ├── Artifacts
    │   ├── Logs
    │   └── History
    └── Configure
        ├── AI
        ├── Voice
        ├── Media
        ├── Research
        ├── Automation
        └── Output
```

Story, Video, Image, Research, Media, Workflow Design, Services, Jobs, Artifacts, Logs, Providers, Models, Voice và Automation không phải navigation page riêng trong Floword. Workflow Design chỉ xuất hiện trong Configure > Advanced khi cần.

Floword chịu trách nhiệm:

- nhập prompt;
- chọn file/video;
- chọn model;
- chọn voice;
- chạy workflow;
- xem progress;
- xem logs;
- quản lý jobs;
- xem artifacts;
- quản lý AI provider qua OmniRoute;
- gọi MediaCrawler, Youwee, Vynaro, OpenMontage, Story Studio, Image tools thông qua backend;
- gửi draft sang CapCut Automation khi cần chỉnh sâu.

Floword không nên chứa business logic nặng trong React.

---

## 5. CapCut Automation

CapCut Automation giữ UI riêng.

```text
CAPCUT AUTOMATION
│
├── Draft
├── Timeline
├── Tracks
├── Video
├── Audio
├── Captions
├── Effects
├── Render
└── Debug / Automation
```

Floword chỉ cần:

```text
Draft Ready
    │
    └── [Open in CapCut Automation]
```

CapCut Automation vẫn dùng chung backend/core khi phù hợp, nhưng không bị gộp giao diện vào Floword.

---

## 6. Unified Backend

Unified Backend là cổng local duy nhất cho UI.

```text
http://127.0.0.1:30000
```

Kiến trúc:

```text
Floword React UI
      │
      ▼
flowordClient.ts
      │
      ▼
Unified Backend :30000
      │
      ├── AI Adapter
      ├── Media Adapter
      ├── TTS Adapter
      ├── Automation Adapter
      ├── CapCut Adapter
      ├── Job Manager
      ├── Artifact Store
      └── Pipeline
```

Không nên làm:

```text
Floword → OmniRoute trực tiếp
Floword → FFmpeg trực tiếp
Floword → Gemini/OpenAI trực tiếp
Floword → MediaCrawler port trực tiếp
Floword → CapCut Mate trực tiếp
```

Luồng đúng:

```text
Floword
   │
   ▼
Unified Backend
   │
   ▼
Adapter
   │
   ▼
Service / Engine thật
```

---

## 7. API namespace

```text
http://127.0.0.1:30000
│
├── /api/system/*
├── /api/services/*
├── /api/ai/*
├── /api/media/*
├── /api/tts/*
├── /api/jobs/*
├── /api/artifacts/*
├── /api/workflows/*
├── /api/automation/*
└── /api/capcut/*
```

### AI

```text
GET    /api/ai/health
GET    /api/ai/models
POST   /api/ai/chat

GET    /api/ai/providers
POST   /api/ai/providers
GET    /api/ai/providers/{id}
PUT    /api/ai/providers/{id}
DELETE /api/ai/providers/{id}

POST   /api/ai/providers/{id}/test
```

### Media

```text
POST /api/media/probe
POST /api/media/extract-audio
POST /api/media/detect-scenes
POST /api/media/split-scenes
POST /api/media/thumbnail
```

### TTS

```text
GET  /api/tts/engines
GET  /api/tts/voices
POST /api/tts/generate
```

### Jobs

```text
POST /api/jobs
GET  /api/jobs/{id}
POST /api/jobs/{id}/cancel
```

### Artifacts

```text
GET /api/artifacts
GET /api/jobs/{id}/artifacts
```

### CapCut

```text
POST /api/capcut/drafts
POST /api/capcut/media
POST /api/capcut/captions
POST /api/capcut/render
GET  /api/capcut/render/{id}/status
```

Tên endpoint thực tế có thể giữ theo implementation hiện tại nếu đã tồn tại; không cần rewrite chỉ để giống tài liệu.

---

## 8. OmniRoute

OmniRoute chạy như AI Gateway riêng.

```text
OmniRoute
http://127.0.0.1:20128
```

Vai trò:

```text
OMNIROUTE
│
├── Provider Credentials
├── API Keys
├── Models
├── Routing
├── Usage
├── Provider Health
└── OpenAI-compatible API
```

Ví dụ provider:

```text
OpenAI
Gemini
Claude
DeepSeek
Qwen
Kimi
Vision providers
Cloud TTS providers
...
```

Luồng AI:

```text
Floword
   │
   ▼
POST /api/ai/chat
   │
   ▼
Unified Backend
   │
   ▼
OmniRoute Adapter
   │
   ▼
OmniRoute :20128
   │
   ▼
OpenAI / Gemini / Claude / DeepSeek / ...
```

---

## 9. API key và secret

Provider key phải do OmniRoute quản lý.

```text
Floword Settings
      │
      ▼
Unified Backend
      │
      ▼
OmniRoute Management API
      │
      ▼
OmniRoute Secret Store
```

Không lưu trong frontend:

```text
OPENAI_API_KEY
GEMINI_API_KEY
ANTHROPIC_API_KEY
DEEPSEEK_API_KEY
QWEN_API_KEY
KIMI_API_KEY
```

Frontend không được:

- ghi raw key vào `localStorage`;
- hardcode key trong React source;
- log raw key;
- gọi trực tiếp provider.

Các API quản lý credential phải giới hạn local desktop.

---

## 10. Floword backend architecture

```text
Floword Core Backend
│
├── Routers
│   ├── system
│   ├── services
│   ├── ai
│   ├── media
│   ├── tts
│   ├── jobs
│   ├── artifacts
│   ├── automation
│   └── capcut
│
├── Adapters
│   ├── OmniRoute
│   ├── Media
│   ├── TTS
│   ├── Playwright
│   └── CapCut
│
├── Core Services
│   ├── Pipeline
│   ├── Job Manager
│   ├── Artifact Store
│   ├── Timeline
│   └── Event Bus
│
└── Infrastructure
    ├── SQLite
    ├── Config
    ├── Logging
    └── Process lifecycle
```

---

## 11. Các app/service cần đưa capability vào Floword

Floword là UI trung tâm cho capability của:

```text
OmniRoute
MediaCrawler
Youwee
OpenMontage
Story Studio
Vynaro
Image Editor / Image tools
FFmpeg
TTS
Playwright / CDP
```

Không copy nguyên UI từng app vào Floword.

Mapping capability phía sau one-workspace:

```text
OmniRoute
    → Project Brief model / Configure AI / Script stage

MediaCrawler
    → Research / Analyze stage

Youwee
    → Source / Media stage

OpenMontage
    → Media / Montage / Timeline stage

Story Studio
    → Script / Story stage

Vynaro
    → Research / Analyze stage

Image Editor
    → Image operation khi workflow yêu cầu

Playwright
    → Configure Automation / pipeline capability

CapCut Automation
    → UI riêng
```

---

## 12. Pipeline video chính

```text
PROJECT INPUT
│
├── Prompt
├── Local File
├── URL
├── Model
├── Voice
├── Duration
├── Platform
└── Output Mode
     │
     ▼
1. INGEST
     │
     ▼
2. MEDIA ANALYSIS
     │
     ├── FFprobe
     ├── FFmpeg
     ├── Metadata
     ├── Audio
     ├── Scene Detection
     └── Thumbnails
     │
     ▼
3. AI GENERATION
     │
     ▼
   OmniRoute
     │
     ▼
   Script / Story
     │
     ▼
4. VOICE
     │
     ▼
   TTS Engine
     │
     ▼
   voice.wav
     │
     ▼
5. CAPTION / SYNC
     │
     ▼
   captions.json
     │
     ▼
6. TIMELINE
     │
     ├── scenes
     ├── voice
     ├── captions
     └── music
     │
     ▼
7. CAPCUT
     │
     ▼
   CapCut Mate
     │
     ├─────────────┐
     ▼             ▼
 DraftReady      Render
     │             │
     ▼             ▼
Open CapCut    output.mp4
Automation
```

---

## 13. Artifact Store

Artifact Store là nơi lưu kết quả của từng bước.

```text
ArtifactStore
│
├── source.mp4
├── metadata.json
├── scenes.json
├── thumbnails/
├── script.json
├── voice.wav
├── captions.json
├── timeline.json
├── capcut_draft/
└── output.mp4
```

Nên truyền giữa pipeline bằng:

```text
project_id
job_id
artifact_id
```

Không để frontend tự giữ và truyền đường dẫn file lung tung.

---

## 14. Job lifecycle

Các tác vụ dài chạy qua Job Manager.

```text
queued
   │
   ▼
running
   │
   ├── analyzing
   ├── generating
   ├── generating_voice
   ├── composing
   ├── creating_draft
   └── rendering
```

Kết thúc:

```text
draft_ready
completed
failed
cancelled
interrupted
```

Floword đọc trạng thái qua backend hoặc event realtime.

---

## 15. Service status

Status chuẩn:

```text
ready
degraded
offline
not_configured
missing
error
```

Không fake `ready`.

Ví dụ:

```text
OmniRoute      ready
MediaCrawler   ready
Youwee         ready
Vynaro         degraded
TTS            not_configured
Playwright     offline
```

---

## 16. Cấu trúc source hướng tới

Không cần move ngay nếu code đang chạy. Đây là logical target.

```text
D:\capcutpolot\artcraft
│
├── frontend/
│   └── apps/
│       └── artcraft/
│           └── app/
│               └── src/
│                   └── pages/
│                       ├── FlowordStudio/
│                       └── CapCutAutomation/
│
├── backend/
│   │
│   ├── routers/
│   │   ├── system_router.py
│   │   ├── services_router.py
│   │   ├── ai_router.py
│   │   ├── media_router.py
│   │   ├── tts_router.py
│   │   ├── jobs_router.py
│   │   ├── artifacts_router.py
│   │   ├── automation_router.py
│   │   └── capcut_router.py
│   │
│   ├── adapters/
│   │   ├── omniroute_adapter.py
│   │   ├── media_adapter.py
│   │   ├── tts_adapter.py
│   │   ├── automation_adapter.py
│   │   └── capcut_adapter.py
│   │
│   ├── services/
│   │   ├── pipeline_service.py
│   │   ├── job_service.py
│   │   ├── artifact_service.py
│   │   ├── timeline_service.py
│   │   └── service_registry.py
│   │
│   └── models/
│
├── services/
│   ├── omniroute/
│   ├── capcut-mate/
│   ├── playwright-sidecar/
│   └── ...
│
├── crates/
│   └── desktop/
│       └── artcraft/
│
├── unified_server.py
│
├── script/
│   └── artcraft/
│       ├── windows_capcut_dev.ps1
│       └── windows_build.ps1
│
└── docs/
```

Không move folder chỉ để đẹp nếu làm ảnh hưởng runtime.

---

## 17. Dev lifecycle

`windows_capcut_dev.ps1` nên chịu trách nhiệm:

```text
windows_capcut_dev.ps1
│
├── check prerequisites
├── start OmniRoute :20128
├── start Unified Backend :30000
├── start required sidecars
├── start frontend dev server
└── start Tauri
```

Mục tiêu:

```text
1 command
→ toàn bộ môi trường dev chạy
```

---

## 18. Build lifecycle

`windows_build.ps1` nên chịu trách nhiệm:

```text
windows_build.ps1
│
├── build frontend
├── build Rust / Tauri
├── prepare backend runtime
├── prepare local engines
├── package required resources
└── produce ArtCraft executable
```

Mục tiêu:

```text
1 command
→ ArtCraft.exe + runtime cần thiết
```

---

## 19. Nguyên tắc bắt buộc

### UI

```text
Floword Studio
= UI trung tâm

CapCut Automation
= UI riêng
```

### Backend

```text
1 Unified Backend Gateway
```

### AI

```text
OmniRoute
= API + Secret + Model + Routing Center
```

### Service

```text
App cũ
→ Capability / Backend Service
→ Floword sử dụng
```

### Security

```text
Provider secrets
→ OmniRoute

Không nằm trong React
Không nằm trong localStorage
Không log raw value
```

### Integration

Một tính năng chỉ được coi là hoàn thành khi:

```text
Floword UI
    ↓
flowordClient
    ↓
Gateway
    ↓
Adapter
    ↓
Backend thật
    ↓
Result thật
    ↓
Artifact / Job / UI result
```

Health check hoặc status card không đủ để coi là hoàn thành.

---

## 20. Canonical architecture

Đây là sơ đồ chuẩn để dùng làm nguồn tham chiếu chính.

```text
                    ARTCRAFT / TAURI 2
                           │
            ┌──────────────┴──────────────┐
            │                             │
            ▼                             ▼
     FLOWORD STUDIO              CAPCUT AUTOMATION
       UI TRUNG TÂM                   UI RIÊNG
            │                             │
            └──────────────┬──────────────┘
                           │
                           ▼
                 UNIFIED BACKEND :30000
                           │
          ┌────────────────┼────────────────┐
          │                │                │
          ▼                ▼                ▼
       Pipeline        Infrastructure     Adapters
          │                │                │
          │           Job Manager           ├─ OmniRoute
          │           ArtifactStore         ├─ FFmpeg
          │           EventBus              ├─ TTS
          │           SQLite                ├─ Playwright
          │                                 └─ CapCut Mate
          │
          ▼
      Media → AI → Voice → Caption → Timeline
          │
          ▼
      CapCut Engine
          │
      ┌───┴────┐
      ▼        ▼
    Draft    Render


                    OMNIROUTE :20128
                           │
          ┌────────────────┼─────────────────┐
          │                │                 │
          ▼                ▼                 ▼
      API Keys          Models           Routing
          │                                  │
          ▼                                  ▼
 Provider Credentials                  External AI
          │                                  │
          ├── OpenAI                         ├── OpenAI
          ├── Gemini                         ├── Gemini
          ├── Claude                         ├── Claude
          ├── DeepSeek                       ├── DeepSeek
          ├── Qwen                           ├── Qwen
          └── Kimi                           └── Kimi
```

---

## 21. Quy tắc chốt

```text
2 UI
│
├── Floword Studio
└── CapCut Automation

1 Backend Gateway
│
└── Unified Backend :30000

N Adapters / Engines
│
├── OmniRoute
├── FFmpeg
├── TTS
├── Playwright
├── MediaCrawler
├── Youwee
├── OpenMontage
├── Story Studio Core
├── Vynaro Core
├── Image Tools
└── CapCut Mate
```

Đây là kiến trúc chuẩn để tiếp tục phát triển ArtCraft.
