import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

const { openMock, listJobsMock, listenProgressMock, cancelJobMock, startJobMock, previewJobMock, removeJobMock, retryJobMock } = vi.hoisted(() => ({
  openMock: vi.fn(),
  listJobsMock: vi.fn(),
  listenProgressMock: vi.fn(),
  cancelJobMock: vi.fn(),
  startJobMock: vi.fn(),
  previewJobMock: vi.fn(),
  removeJobMock: vi.fn(),
  retryJobMock: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock }));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
}));
vi.mock("../../app/src/pages/PageCapCutAutomation/api/pipelineClient", () => ({
  cancelNativeJob: cancelJobMock,
  listNativeAutomationJobs: listJobsMock,
  listenNativeAutomationProgress: listenProgressMock,
  previewNativeAutomation: previewJobMock,
  removeNativeJob: removeJobMock,
  retryNativeJob: retryJobMock,
  startNativeAutomationJob: startJobMock,
}));

import { NativeAutomationWorkspace } from "../../app/src/pages/PageCapCutAutomation/panels/auto-render/NativeAutomationWorkspace";

describe("NativeAutomationWorkspace", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    openMock.mockResolvedValue([]);
    listJobsMock.mockResolvedValue([]);
    listenProgressMock.mockResolvedValue(() => undefined);
    previewJobMock.mockResolvedValue({ path: "C:\\preview\\first.mp4", cacheKey: "preview-key" });
    removeJobMock.mockResolvedValue(undefined);
  });
  afterEach(() => cleanup());

  test("renders native controls and imports multiple videos into one queue", async () => {
    openMock.mockResolvedValue(["C:\\media\\first.mp4", "C:\\media\\second.mp4"]);
    render(<NativeAutomationWorkspace />);

    expect(screen.getByRole("heading", { name: "Tự động hóa nội bộ" })).toBeInTheDocument();
    expect(screen.getByText("Video / Hàng đợi")).toBeInTheDocument();
    expect(screen.getByText("Phụ đề / Hook mở đầu")).toBeInTheDocument();
    expect(screen.getByText("OCR / Làm mờ / Nhãn dán")).toBeInTheDocument();
    expect(screen.getByText("Xem trước")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Thêm video" }));

    await waitFor(() => {
      expect(screen.getByText("first.mp4")).toBeInTheDocument();
      expect(screen.getByText("second.mp4")).toBeInTheDocument();
    });
    expect(openMock).toHaveBeenCalledWith(expect.objectContaining({ multiple: true }));
  });

  test("shows backend progress state and delegates cancel to the active job", async () => {
    const renderingJob = {
      jobId: "job-1",
      requestId: "req-1",
      attempt: 1,
      inputPath: "C:\\media\\rendering.mp4",
      pageName: "ArtCraft",
      state: "RENDERING",
      progress: 0.42,
      stage: "RENDERING",
      createdAt: 1,
      startedAt: 2,
      finishedAt: null,
      processedMs: 420,
      expectedDurationMs: 1000,
      ffmpegPid: 1234,
      errorCode: null,
      errorMessage: null,
      error: null,
      receipt: null,
    };
    const cancelledJob = { ...renderingJob, state: "CANCELLED", stage: "CANCELLED", progress: 0 };
    listJobsMock.mockResolvedValue([renderingJob]);
    cancelJobMock.mockResolvedValue(cancelledJob);

    render(<NativeAutomationWorkspace />);

    await waitFor(() => expect(screen.getByText("rendering.mp4")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Hủy" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Hủy" }));

    await waitFor(() => expect(cancelJobMock).toHaveBeenCalledWith("job-1"));
    await waitFor(() => expect(screen.getByText("CANCELLED")).toBeInTheDocument());
  });

  test("submits every queued video through the backend FIFO API", async () => {
    openMock.mockResolvedValue(["C:\\media\\first.mp4", "C:\\media\\second.mp4"]);
    startJobMock.mockImplementation(async (request: { jobId: string; inputPath: string }) => ({
      jobId: request.jobId,
      requestId: request.requestId,
      inputPath: request.inputPath,
      pageName: "ArtCraft",
      state: "QUEUED",
      progress: 0,
      stage: "QUEUED",
      attempt: 1,
      receipt: null,
    }));
    render(<NativeAutomationWorkspace />);
    fireEvent.click(screen.getByRole("button", { name: "Thêm video" }));
    await waitFor(() => expect(screen.getByText("first.mp4")).toBeInTheDocument());
    // Production dispatch requires an explicit source-language contract.
    fireEvent.change(screen.getByDisplayValue("Tự động phát hiện"), {
      target: { value: "en" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Chạy hàng đợi" }));

    await waitFor(() => expect(startJobMock).toHaveBeenCalledTimes(2));
    expect(startJobMock.mock.calls.map(([request]) => request.inputPath)).toEqual([
      "C:\\media\\first.mp4",
      "C:\\media\\second.mp4",
    ]);
  });

  test("delegates preview and displays the cached output", async () => {
    openMock.mockResolvedValue(["C:\\media\\first.mp4"]);
    render(<NativeAutomationWorkspace />);
    fireEvent.click(screen.getByRole("button", { name: "Thêm video" }));
    await waitFor(() => expect(screen.getByText("first.mp4")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Xem trước 5 giây" }));

    await waitFor(() => expect(previewJobMock).toHaveBeenCalledWith("C:\\media\\first.mp4", expect.objectContaining({ schemaVersion: 1 })));
    await waitFor(() => expect(document.querySelectorAll("video")).toHaveLength(2));
  });

  test("exposes retry and remove actions for terminal jobs", async () => {
    const failedJob = {
      jobId: "job-failed",
      requestId: "req-failed",
      attempt: 1,
      inputPath: "C:\\media\\failed.mp4",
      pageName: "ArtCraft",
      state: "FAILED",
      progress: 0.2,
      stage: "FAILED",
      error: "CAPCUT_RENDER_FAILED",
      receipt: null,
    };
    listJobsMock.mockResolvedValue([failedJob]);
    retryJobMock.mockResolvedValue({ ...failedJob, state: "QUEUED", stage: "QUEUED", error: null });
    render(<NativeAutomationWorkspace />);
    await waitFor(() => expect(screen.getByText("failed.mp4")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Thử lại với cấu hình cũ" }));
    await waitFor(() => expect(retryJobMock).toHaveBeenCalledWith("job-failed"));
    cleanup();
    listJobsMock.mockResolvedValue([failedJob]);
    render(<NativeAutomationWorkspace />);
    await waitFor(() => expect(screen.getByText("failed.mp4")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Xóa" }));
    await waitFor(() => expect(removeJobMock).toHaveBeenCalledWith("job-failed"));
  });

  test("edits manual subtitle cues and blur regions in native mode", async () => {
    render(<NativeAutomationWorkspace />);
    fireEvent.click(screen.getByRole("button", { name: "Thêm mốc" }));
    expect(screen.getByLabelText("Nội dung mốc 1")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Thêm vùng" }));
    expect(screen.getByText("Bật xử lý chữ")).toBeInTheDocument();
    expect(screen.getAllByText("Bắt đầu (ms)").length).toBeGreaterThan(0);
  });
});
