# FLOWORD STUDIO — MASTER ARCHITECTURE & UX REFACTORING BLUEPRINT

> **TÀI LIỆU KIẾN TRÚC & QUY TẮC BẢO TOÀN NĂNG LỰC (CAPABILITY)**
> **Mục tiêu:** Chuyển đổi toàn diện từ mô hình phân mảnh *Service-Centric* sang bảng điều khiển sản xuất tập trung *Job/Page-Centric* (Production Console).
> **Nguyên tắc tối thượng:** Xóa bỏ UI thừa/phân mảnh nhưng **TUYỆT ĐỐI KHÔNG XÓA** Capability, Adapter, và Backend tương ứng.

---

## 1. TỔNG QUAN CHIẾN LƯỢC (EXECUTIVE SUMMARY)

```
                       ARTCRAFT DESKTOP
                              │
  ┌───────────────────────────┼───────────────────────────┐
  │                           │                           │
FLOWORD STUDIO          IMAGE EDITOR                SETTINGS / DEV
(90% Thời gian UX)    (Công cụ chuyên biệt)      (Diagnostics, Engines)
```

* **Vấn đề cũ:** App phơi bày toàn bộ các module con (`OmniRoute`, `MediaCrawler`, `Youwee`, `Vynaro`, `Story Studio`, `OpenMontage`, `CapCut Mate`, `Voice`, `Research`...) thành các menu ngang hàng, biến UI thành một "bảng điều khiển máy móc" gây rối rắm cho khách hàng.
* **Mục tiêu mới:** Khách hàng chỉ cần quản lý theo **Page** và theo dõi theo **Job** từ khâu đầu vào đến khi xuất bản (Publish). Toàn bộ backend phức tạp chạy ngầm bên dưới thông qua cơ chế Adapter/Facade.

---

## 2. NGUYÊN TẮC BẤT BIẾN (CARDINAL RULES)

1. **Bảo tồn Năng lực (Zero Capability Loss):** Backend và Adapter phải được bảo toàn 100%. Không bao giờ xóa logic xử lý, API endpoint, hoặc IPC commands của backend khi dọn dẹp UI.
2. **Nguyên tắc Facade UX:** Người dùng không cần biết và không được bắt chọn backend nào đang chạy (`Vynaro`, `MediaCrawler`, `OmniRoute`...). Người dùng chỉ tương tác với **Tính năng Nghiệp vụ** (Business Features).
3. **Một "Ngôi nhà" Duy nhất cho Mỗi Dữ liệu (Single Home Principle):**
   * Cấu hình mặc định $\rightarrow$ Nằm trong `Pages -> Page Settings`.
   * Trạng thái chi tiết $\rightarrow$ Nằm trong `Jobs -> Job Inspector`.
   * Trạng thái Worker $\rightarrow$ Nằm trong `Settings -> Workers` (Top bar chỉ hiện tóm tắt).
4. **Màu sắc theo Trạng thái (State-Driven Colors):** Không tô màu theo thương hiệu backend. Dùng màu phản ánh tiến độ sản xuất:
   * `QUEUED`: Xám/Neutral
   * `RUNNING`: Xanh dương/Primary
   * `WAITING / REVIEW`: Vàng/Warning
   * `DONE`: Xanh lá/Success
   * `ERROR`: Đỏ/Destructive
5. **Cảnh báo theo Tác động Nghiệp vụ (Business Impact Alerts):** Không báo lỗi kỹ thuật hoảng loạn toàn màn hình (`MEDIACRAWLER OFFLINE`). Chỉ hiển thị ngữ cảnh khi hành động liên quan bị ảnh hưởng (ví dụ: *0/10 Grok Workers available* $\rightarrow$ *Generation Paused*).

---

## 3. MÔ HÌNH ÁNH XẠ UX ĐẾN BACKEND (FACADE PATTERN)

```
Khách hàng thực hiện Hành động (User Action)
                     │
                     ▼
          Tính năng Nghiệp vụ (Business Feature)
                     │
                     ▼
           Tầng Điều phối (Adapter / Orchestrator)
                     │
     ┌───────────────┼───────────────┬───────────────┐
     ▼               ▼               ▼               ▼
MediaCrawler /    Story + Omni/    Grok Extension /  CapCut Mate /
   Youwee          Browser AI        OpenMontage     Playwright
(Research)        (Nội dung)       (Tạo Video)      (Xuất bản/Draft)
```

---

## 4. PHÂN LOẠI VÀ XỬ LÝ GIAO DIỆN CŨ (UI CLASSIFICATION MATRIX)

| Nhóm | Danh sách Module / UI cũ | Định hướng Xử lý | Ghi chú & Điều kiện |
| :--- | :--- | :--- | :--- |
| **NHÓM A** *(Loại khỏi UI độc lập)* | • OmniRoute UI<br>• MediaCrawler UI<br>• Youwee UI<br>• Vynaro UI<br>• Story Studio UI<br>• OpenMontage UI<br>• Voice / TTS Standalone<br>• Service Registry UI<br>• Standalone Research UI | **Ẩn navigation $\rightarrow$ Chuyển tính năng vào Floword $\rightarrow$ Xóa giao diện cũ sau khi đạt 100% Parity** | **Backend giữ nguyên 100%**. Chỉ xóa component giao diện cũ khi Floword đã bao phủ trọn vẹn nghiệp vụ. |
| **NHÓM B** *(Hạ cấp xuống Settings/Dev)* | • Service Health & Ports<br>• Backend Console Logs<br>• Raw Provider & Model List<br>• Adapter Tests & Diagnostics<br>• Process Manager & Browser Runtime | **Đưa vào `Settings -> System` (Mặc định Collapsed)** | Khách hàng thông thường không nhìn thấy; Developer/Kỹ thuật viên vẫn mở ra debug được. |
| **NHÓM C** *(UI Chuyên biệt giữ riêng)* | • **Image Editor** (Canvas, Layers, Tooling)<br>• **CapCut Editor** (Chỉnh sửa Draft thủ công chuyên sâu) | **Giữ thành công cụ riêng biệt hoặc gọi qua action "Open in..."** | Không nhồi toàn bộ Canvas Editor vào Floword Dashboard để tránh làm phình to và rối giao diện. |

---

## 5. CẤU TRÚC SHELL & 6 MỤC NAVIGATION CHÍNH

Thanh Sidebar chỉ giữ đúng **6 mục cốt lõi**:

```
FLOWORD STUDIO
├── ▣ 1. Dashboard  : Tổng quan năng suất, KPI, Pipeline stats, Jobs gần đây.
├── ▤ 2. Jobs       : Danh sách toàn bộ Jobs, Bộ lọc đa chiều, Job Inspector.
├── ◫ 3. Pages      : Quản lý theo Kênh/Page, Cấu hình Prompt/Preset mặc định, Lịch đăng.
├── ▶ 4. Studio     : Khởi tạo/Chỉnh sửa 1 Job (4-Pane: Simple Mode & Advanced Mode).
├── ↑ 5. Publish    : Hộp thư chờ duyệt bài (Review Before Post), Duyệt/Sửa/Hẹn giờ đăng.
└── ⚙ 6. Settings   : Cấu hình hệ thống, Worker pools, Engines, Storage, Diagnostics.
```

### Top Bar Tối Giản
`[Floword Studio] | [Page: All ▼] | Queue: 42 | Workers: 8/10 ● | [+ New Job]` (Bên phải: Notifications, Settings).

---

## 6. ĐẶC TẢ CHI TIẾT CÁC MÀN HÌNH CHÍNH

### 6.1. Production Dashboard
```
┌───────────────────────────────────────────────────────────────┐
│ Floword Studio                           Production ● Healthy │
├───────────────────────────────────────────────────────────────┤
│  Total Jobs      Running       Queued       Error             │
│     128             10            32           3              │
├───────────────────────────────────────────────────────────────┤
│ Production Pipeline                                           │
│  Generate Image       3                                       │
│  Convert 9:16         2                                       │
│  Generate Video       4                                       │
│  Download             1                                       │
│  Ready To Post        8                                       │
│  Posting              2                                       │
├───────────────────────────────────────────────────────────────┤
│ Recent Jobs                                                   │
│  JOB029  Movie Feed       Generate Video       67%            │
│  JOB028  Celebrity Feed   Posting              92%            │
│  JOB027  Music Feed       Done                100%            │
└───────────────────────────────────────────────────────────────┘
```

### 6.2. Jobs Management & Job Inspector
* **Màn hình Jobs:**
  * Bảng điều khiển tập trung: Hỗ trợ `+ New Job`, `Bulk Import`, lọc theo Page / Trạng thái / Nền tảng / Ngày.
  * Mỗi thẻ Job thể hiện trực quan đường ray tiến độ: `[IMAGE ✓] -> [9:16 ✓] -> [VIDEO ●] -> [DOWNLOAD ○] -> [POST ○]`.
  * Hỗ trợ nút `Retry` trực tiếp khi một công đoạn gặp sự cố.
* **Job Inspector (Panel trượt bên phải hoặc Modal lớn):**
  * **Lineage:** Input Image $\rightarrow$ Image Prompt $\rightarrow$ Generated Image $\rightarrow$ 9:16 Convert $\rightarrow$ Video Prompt $\rightarrow$ Video Preview.
  * **Publishing Status:** Trạng thái từng kênh (Facebook: READY, TikTok: NOT STARTED, YouTube: NOT STARTED).
  * **Audit History:** Toàn bộ mốc thời gian thực hiện từng bước của Job (Timestamp, Worker ID, Logs).

### 6.3. Pages Hub & Page Preset Settings
Mỗi **Page** là một đơn vị kinh doanh độc lập. Khi tạo Job thuộc Page nào, toàn bộ cấu hình sau sẽ tự động điền:
* **General:** Tên Page, Mô tả.
* **Generation Defaults:** Prompt ảnh mặc định, Prompt 9:16, Prompt Video, Preferred Worker Pool.
* **Storage:** Thư mục lưu trữ tài nguyên riêng (VD: `D:\Movie Feed\`).
* **Browser Runtime:** Gắn với Browser Profile cố định (VD: `PROFILE_MOVIE_01`).
* **Publishing Accounts:** Tài khoản Facebook Page, TikTok Channel, YouTube Channel.
* **Posting Rules:** Chế độ đăng (`Auto` hoặc `Review`), Khung giờ vàng mặc định (`08:30`, `10:00`, `17:00`, `22:00`).

### 6.4. Floword Studio (Tạo & Chỉnh Sửa Job)
Bố cục 4 phân vùng rõ ràng:
1. **Pipeline Rail:** Theo dõi trạng thái từng bước (`Input` $\rightarrow$ `Image AI` $\rightarrow$ `9:16` $\rightarrow$ `Video` $\rightarrow$ `Download` $\rightarrow$ `Publish`).
2. **Content Editor:** Nhập Prompt ảnh, Prompt video, Tiêu đề, Caption, Lịch trình.
3. **Live Preview:** Xem trước kết quả ảnh/video của từng bước ngay lập tức.
4. **Activity Logs:** Nhật ký xử lý thời gian thực của Job.
* *Hai chế độ linh hoạt:* **Simple Mode** (Mặc định cho người dùng phổ thông) & **Advanced Mode** (Mở rộng cho tùy chỉnh model, retry count, worker pool, fallback strategy).

### 6.5. Publish Inbox (Review Before Post)
Giao diện thẻ duyệt trực quan trước khi đăng:
* Preview Video thành phẩm.
* Tên Page, Caption, Hashtags đính kèm.
* Lịch đăng dự kiến theo từng nền tảng (Facebook, TikTok, YouTube).
* Nút thao tác nhanh: `[Reject]` `[Edit]` `[Approve]`.

---

## 7. CẤU TRÚC THƯ MỤC FRONTEND CHUẨN (`frontend/.../FlowordStudio`)

```
pages/
└── FlowordStudio/
    ├── FlowordApp.tsx             # Main App Router & State Provider
    │
    ├── shell/                     # Khung giao diện dùng chung
    │   ├── FlowordSidebar.tsx     # 6 mục menu tinh gọn
    │   ├── FlowordHeader.tsx      # Top bar (Page filter, stats, quick action)
    │   └── FlowordLayout.tsx      # Wrapper layout
    │
    ├── dashboard/                 # Màn hình 1: Dashboard
    │   ├── DashboardPage.tsx
    │   ├── ProductionStats.tsx
    │   ├── ActiveJobs.tsx
    │   └── ErrorSummary.tsx
    │
    ├── jobs/                      # Màn hình 2: Jobs
    │   ├── JobsPage.tsx
    │   ├── JobTable.tsx
    │   ├── JobInspector.tsx
    │   ├── JobProgress.tsx
    │   └── JobHistory.tsx
    │
    ├── pages/                     # Màn hình 3: Pages
    │   ├── PagesPage.tsx
    │   ├── PageOverview.tsx
    │   └── PageSettings.tsx
    │
    ├── studio/                    # Màn hình 4: Studio
    │   ├── StudioPage.tsx
    │   ├── InputStep.tsx
    │   ├── ImageStep.tsx
    │   ├── VerticalStep.tsx
    │   ├── VideoStep.tsx
    │   └── PublishStep.tsx
    │
    ├── publish/                   # Màn hình 5: Publish Inbox
    │   ├── PublishQueue.tsx
    │   ├── ReviewCard.tsx
    │   └── PublishHistory.tsx
    │
    ├── settings/                  # Màn hình 6: Settings & Developer
    │   ├── General.tsx
    │   ├── Workers.tsx
    │   ├── Engines.tsx            # OmniRoute / Vynaro / MediaCrawler configs
    │   ├── Storage.tsx
    │   └── Diagnostics.tsx        # Backend logs, health check, ports
    │
    ├── components/                # UI Components tái sử dụng
    │   ├── StatusBadge.tsx
    │   ├── ArtifactPreview.tsx
    │   ├── ProgressRail.tsx
    │   └── ErrorPanel.tsx
    │
    └── api/                       # API Facade Client
        └── flowordClient.ts       # Kết nối tập trung đến Unified Backend
```

---

## 8. LỘ TRÌNH 4 BƯỚC THI CÔNG AN TOÀN (SAFE MIGRATION ROADMAP)

```
              TIẾN TRÌNH XỬ LÝ GIAO DIỆN
                         │
        [ Đợt 1: Kiểm kê & Khóa Kiến trúc ]
       (Inventory routes, lập ma trận Parity)
                         │
                         ▼
        [ Đợt 2: Tái cấu trúc Floword Studio ]
        (Xây 6 màn hình chuẩn, gắn API Facade)
                         │
                         ▼
        [ Đợt 3: Ẩn Navigation cũ & Test Parity ]
         (Ẩn menu, kiểm tra E2E, giữ Dev route)
                         │
                         ▼
        [ Đợt 4: Xóa triệt để Dead UI Code ]
       (Chỉ xóa khi Parity = 100% & Build PASS)
```

### Tiêu chuẩn Nghiệm thu để được phép xóa UI cũ (Definition of Done):
Một giao diện/route cũ chỉ được phép xóa bỏ khỏi mã nguồn khi thỏa mãn **tất cả các điều kiện sau**:
1. [x] **Capability Parity = 100%:** Toàn bộ năng lực của UI cũ đã có vị trí thực hiện tương ứng trong Floword.
2. [x] **No Navigation Dependency:** Không còn liên kết hay redirect nào trỏ đến route cũ.
3. [x] **No Shared Component Break:** Không làm gãy các component dùng chung.
4. [x] **No Backend IPC Break:** Các lệnh gọi Backend/Sidecar không bị ảnh hưởng.
5. [x] **Backend Chạy Độc Lập:** Unified Server và các sidecar vẫn hoạt động bình thường.
6. [x] **Targeted Tests & E2E Pass:** Tất cả unit tests và luồng sản xuất thực tế chạy thành công.
7. [x] **Production Build Pass:** Bản dựng ứng dụng không có lỗi biên dịch.

---
*Tài liệu này là căn cứ kiến trúc chuẩn cho toàn bộ các đợt cập nhật, refactor và mở rộng của dự án Floword / ArtCraft.*
