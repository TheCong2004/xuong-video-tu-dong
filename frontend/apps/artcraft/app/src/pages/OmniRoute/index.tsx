import { useCallback, useEffect, useRef, useState } from "react";

// Vite proxies `/omniroute/*` to http://127.0.0.1:20128 and strips the prefix,
// so the readiness probe reuses the same route as the iframe itself.
const OMNIROUTE_PATH = "http://127.0.0.1:20128/?embed=true";
const HEALTH_PATH = "http://127.0.0.1:20128/api/health/ping";

const PROBE_INTERVAL_MS = 1000;
const READINESS_TIMEOUT_MS = 90_000;
const IFRAME_LOAD_TIMEOUT_MS = 60_000;

type Phase = "checking" | "loading" | "loaded" | "error";

export function PageOmniRoute() {
  const [phase, setPhase] = useState<Phase>("checking");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [iframeKey, setIframeKey] = useState(0);

  // Guards the probe loop so a retry cancels any in-flight polling.
  const runIdRef = useRef(0);

  const retry = useCallback(() => {
    setErrorMessage(null);
    setPhase("checking");
    setAttempt((prev) => prev + 1);
  }, []);

  const resetToDashboard = useCallback(() => {
    setErrorMessage(null);
    setIframeKey((prev) => prev + 1);
    setPhase("loading");
  }, []);

  // Readiness probe: never mount the iframe before OmniRoute answers on :20128.
  useEffect(() => {
    if (phase !== "checking") return;

    const runId = ++runIdRef.current;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let cancelled = false;
    const startedAt = Date.now();

    console.log("[OmniRoute] checking readiness");

    const probe = async () => {
      if (cancelled || runId !== runIdRef.current) return;

      let ready = false;
      try {
        const response = await fetch(HEALTH_PATH, {
          method: "GET",
          cache: "no-store",
        });
        // Only the health route's own statuses count as ready: 200 = ok,
        // 503 = server up but DB ping failed. A dead upstream surfaces as a
        // proxy-generated 500/502/504, which must NOT pass as ready.
        ready = response.status === 200 || response.status === 503;
      } catch {
        ready = false;
      }

      if (cancelled || runId !== runIdRef.current) return;

      if (ready) {
        const elapsed = ((Date.now() - startedAt) / 1000).toFixed(1);
        console.log(`[OmniRoute] ready after ${elapsed}s`);
        console.log("[OmniRoute] mounting iframe");
        setIframeKey((prev) => prev + 1);
        setPhase("loading");
        return;
      }

      if (Date.now() - startedAt >= READINESS_TIMEOUT_MS) {
        console.warn("[OmniRoute] readiness timeout");
        setErrorMessage(
          "Không kết nối được OmniRoute trên 127.0.0.1:20128. Dịch vụ có thể chưa khởi động xong.",
        );
        setPhase("error");
        return;
      }

      timer = setTimeout(probe, PROBE_INTERVAL_MS);
    };

    void probe();

    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [phase, attempt]);

  // Bound the iframe load itself so a stalled document can never spin forever.
  useEffect(() => {
    if (phase !== "loading") return;

    const timer = setTimeout(() => {
      console.warn("[OmniRoute] iframe load timeout");
      setErrorMessage("OmniRoute tải quá lâu và đã bị hủy. Vui lòng thử lại.");
      setPhase("error");
    }, IFRAME_LOAD_TIMEOUT_MS);

    return () => clearTimeout(timer);
  }, [phase, iframeKey]);

  return (
    <div className="flex h-[calc(100vh-56px)] w-full flex-col bg-[#0f1015]">
      <div className="relative flex-1 w-full overflow-hidden bg-[#0f1015]">
        {phase === "loading" || phase === "loaded" ? (
          <button
            type="button"
            onClick={resetToDashboard}
            data-testid="omniroute-dashboard-back-button"
            title="Quay về trang chính OmniRoute"
            className="absolute left-4 top-4 z-20 rounded-xl border border-white/10 bg-[#171922]/95 px-4 py-2 text-sm font-semibold text-white shadow-lg backdrop-blur transition hover:border-white/20 hover:bg-[#222531]"
          >
            ← Về OmniRoute
          </button>
        ) : null}

        {phase === "error" ? (
          <div className="flex h-full w-full flex-col items-center justify-center bg-[#121318] p-8 text-center text-slate-200">
            <div className="max-w-md space-y-4 rounded-2xl border border-red-500/20 bg-[#1c1e26] p-8 shadow-xl">
              <h3 className="text-xl font-bold text-white">
                Không tải được OmniRoute
              </h3>
              <p className="text-xs text-slate-400">
                {errorMessage ?? "Lỗi không xác định"}
              </p>
              <button
                onClick={retry}
                data-testid="omniroute-retry-button"
                className="rounded-xl bg-blue-600 px-5 py-2.5 text-sm font-medium text-white transition hover:bg-blue-500"
              >
                Thử lại
              </button>
            </div>
          </div>
        ) : null}

        {phase === "checking" || phase === "loading" ? (
          <div
            data-testid="omniroute-loading-state"
            className="absolute inset-0 z-10 flex items-center justify-center bg-[#0f1015]"
          >
            <div className="flex flex-col items-center gap-3">
              <div className="h-8 w-8 animate-spin rounded-full border-2 border-indigo-500/20 border-t-indigo-500" />
              <div className="text-xs text-slate-500">
                {phase === "checking"
                  ? "Đang kết nối OmniRoute (127.0.0.1:20128)..."
                  : "Đang tải OmniRoute..."}
              </div>
            </div>
          </div>
        ) : null}

        {phase === "loading" || phase === "loaded" ? (
          <iframe
            key={iframeKey}
            src={OMNIROUTE_PATH}
            className="h-full w-full border-none bg-transparent"
            title="OmniRoute AI Router"
            onLoad={() => {
              console.log("[OmniRoute] iframe loaded");
              setPhase("loaded");
            }}
            onError={() => {
              console.error("[OmniRoute] iframe error");
              setErrorMessage(
                "Iframe OmniRoute không tải được nội dung từ 127.0.0.1:20128.",
              );
              setPhase("error");
            }}
          />
        ) : null}
      </div>
    </div>
  );
}
