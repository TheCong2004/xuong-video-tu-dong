"use client";

import { useState } from "react";
import ProviderIcon from "@/shared/components/ProviderIcon";
import { type ComboItem } from "./RouteManagerSection";

export interface ConfiguredConnectionItem {
  id: string;
  provider: string;
  name?: string;
  authType?: string;
  priority?: number;
  isActive?: boolean;
  testStatus?: string;
  lastTested?: string;
  lastLatencyMs?: number;
  apiKey?: string;
  lastError?: string;
  lastErrorAt?: string;
  providerSpecificData?: Record<string, unknown>;
  createdAt?: string;
  updatedAt?: string;
}

interface ConfiguredConnectionsSectionProps {
  connections: ConfiguredConnectionItem[];
  routes?: ComboItem[];
  onAddKey: () => void;
  onEditConnection: (conn: ConfiguredConnectionItem) => void;
  onToggleConnection: (conn: ConfiguredConnectionItem, active: boolean) => Promise<void>;
  onDeleteConnection: (conn: ConfiguredConnectionItem) => Promise<void>;
  onTestConnection: (conn: ConfiguredConnectionItem) => Promise<void>;
  testingProviderId?: string | null;
}

const AUTH_TYPE_LABELS: Record<string, { label: string; badgeClass: string }> = {
  apikey: { label: "API Key", badgeClass: "bg-amber-500/10 text-amber-400 border-white/10" },
  oauth: { label: "OAuth", badgeClass: "bg-blue-500/10 text-blue-400 border-white/10" },
  "web-cookie": { label: "Web Session", badgeClass: "bg-purple-500/10 text-purple-400 border-white/10" },
  webcookie: { label: "Web Session", badgeClass: "bg-purple-500/10 text-purple-400 border-white/10" },
  compatible: { label: "Tương thích", badgeClass: "bg-orange-500/10 text-orange-400 border-orange-500/30" },
  cli: { label: "CLI Token", badgeClass: "bg-cyan-500/10 text-cyan-400 border-white/10" },
  free: { label: "Miễn phí", badgeClass: "bg-emerald-500/10 text-emerald-400 border-white/10" },
};

export default function ConfiguredConnectionsSection({
  connections = [],
  routes = [],
  onAddKey,
  onEditConnection,
  onToggleConnection,
  onDeleteConnection,
  onTestConnection,
  testingProviderId,
}: ConfiguredConnectionsSectionProps) {
  const [actionLoadingId, setActionLoadingId] = useState<string | null>(null);

  const getRoutesUsingProvider = (providerId: string) => {
    const pid = (providerId || "").toLowerCase();
    return routes
      .filter((r) =>
        r.isActive !== false &&
        Array.isArray(r.models) &&
        r.models.some((m) => m.provider?.toLowerCase() === pid)
      )
      .map((r) => r.name);
  };

  const handleToggle = async (conn: ConfiguredConnectionItem) => {
    try {
      setActionLoadingId(conn.id);
      await onToggleConnection(conn, conn.isActive === false);
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleDelete = async (conn: ConfiguredConnectionItem) => {
    const label = conn.name || conn.provider;
    if (window.confirm(`Bạn có chắc muốn xóa kết nối "${label}" (${conn.provider})?`)) {
      try {
        setActionLoadingId(conn.id);
        await onDeleteConnection(conn);
      } finally {
        setActionLoadingId(null);
      }
    }
  };

  return (
    <section className="space-y-4">
      {/* Header */}
      <div className="bg-[#131926] p-5 rounded-xl border border-white/10 shadow-lg flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 border border-white/10 flex items-center justify-center">
            <svg className="w-5 h-5 text-indigo-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path strokeLinecap="round" strokeLinejoin="round" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
            </svg>
          </div>
          <div>
            <h2 className="text-lg font-bold text-white tracking-wide flex items-center gap-2">
              Trung tâm kết nối AI
              <span className="text-[11px] font-medium text-slate-400 bg-black/40 px-2 py-0.5 rounded border border-white/10">
                {connections.length} kết nối
              </span>
            </h2>
            <p className="text-xs text-slate-400">
              Danh sách các tài khoản và API Key đã được kết nối với OmniRouter.
            </p>
          </div>
        </div>

        <button
          onClick={onAddKey}
          className="flex items-center gap-2 px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-bold shadow transition active:scale-95"
        >
          <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
          </svg>
          + Thêm API Key
        </button>
      </div>

      {/* Empty State */}
      {connections.length === 0 ? (
        <div className="bg-[#131926] p-8 rounded-xl border border-dashed border-white/10 text-center flex flex-col items-center justify-center">
          <div className="w-14 h-14 rounded-2xl bg-slate-800/80 border border-white/10 flex items-center justify-center mb-4">
            <svg className="w-7 h-7 text-indigo-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path strokeLinecap="round" strokeLinejoin="round" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
            </svg>
          </div>
          <h3 className="text-sm font-bold text-white mb-1">Chưa có kết nối AI nào</h3>
          <p className="text-xs text-slate-400 max-w-md mb-4">
            Nhấn <strong className="text-indigo-400">+ Thêm API Key</strong> để chọn nhà cung cấp AI (Grok, Gemini, OpenAI, Claude...) và nhập thông tin xác thực.
          </p>
          <button
            onClick={onAddKey}
            className="flex items-center gap-2 px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-bold shadow transition active:scale-95"
          >
            + Thêm API Key
          </button>
        </div>
      ) : (
        /* Connections Grid */
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {connections.map((conn) => {
            const isEnabled = conn.isActive !== false;
            const authInfo = AUTH_TYPE_LABELS[conn.authType || "apikey"] || AUTH_TYPE_LABELS.apikey;
            const isTesting = testingProviderId === conn.id || testingProviderId === conn.provider;
            const isActionLoading = actionLoadingId === conn.id;
            const associatedRoutes = getRoutesUsingProvider(conn.provider);
            const hasError = Boolean(conn.lastError) || conn.testStatus === "error";
            const isOnline = isEnabled && !hasError;
            const latencyMs = conn.lastLatencyMs || (conn.providerSpecificData?.lastLatencyMs as number | undefined);

            return (
              <div
                key={conn.id}
                className={`bg-[#131926] p-4 rounded-xl border shadow-lg flex flex-col justify-between transition-all ${
                  isEnabled
                    ? "border-white/10 hover:border-slate-700"
                    : "border-white/10 opacity-60"
                }`}
              >
                <div>
                  {/* Card Header */}
                  <div className="flex items-start justify-between gap-2 mb-3">
                    <div className="flex items-center gap-2.5 min-w-0">
                      <div className="w-9 h-9 rounded-lg bg-slate-800/80 flex items-center justify-center shrink-0 border border-white/10">
                        <ProviderIcon providerId={conn.provider} size={20} />
                      </div>
                      <div className="min-w-0">
                        <div className="flex items-center gap-1.5 flex-wrap">
                          <h4 className="text-sm font-bold text-white capitalize truncate">
                            {conn.name || conn.provider}
                          </h4>
                          <span className={`text-[10px] px-1.5 py-0.5 rounded font-bold border ${authInfo.badgeClass}`}>
                            {authInfo.label}
                          </span>
                        </div>
                        <p className="text-[11px] text-slate-500 capitalize">{conn.provider}</p>
                      </div>
                    </div>

                    {/* Toggle */}
                    <button
                      onClick={() => handleToggle(conn)}
                      disabled={isActionLoading}
                      className={`relative w-9 h-5 rounded-full transition-colors shrink-0 ${
                        isEnabled ? "bg-indigo-600" : "bg-slate-700"
                      }`}
                      title={isEnabled ? "Tắt kết nối" : "Bật kết nối"}
                    >
                      <span
                        className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
                          isEnabled ? "translate-x-4" : "translate-x-0.5"
                        }`}
                      />
                    </button>
                  </div>

                  {/* Status Info */}
                  <div className="space-y-1.5 text-xs mb-3">
                    <div className="flex items-center justify-between">
                      <span className="text-slate-500 text-[11px]">Trạng thái:</span>
                      {hasError ? (
                        <span className="flex items-center gap-1 text-[11px] font-bold text-rose-400 bg-rose-500/10 px-2 py-0.5 rounded border border-white/10">
                          <span className="w-1.5 h-1.5 rounded-full bg-rose-400" />
                          Lỗi kết nối
                        </span>
                      ) : isOnline ? (
                        <div className="flex items-center gap-1.5">
                          {latencyMs && (
                            <span className="text-[10px] font-mono text-emerald-400/80">
                              {latencyMs}ms
                            </span>
                          )}
                          <span className="flex items-center gap-1 text-[11px] font-bold text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-white/10">
                            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                            Đã kết nối
                          </span>
                        </div>
                      ) : (
                        <span className="flex items-center gap-1 text-[11px] font-bold text-slate-400 bg-black/40 px-2 py-0.5 rounded border border-white/10">
                          <span className="w-1.5 h-1.5 rounded-full bg-slate-500" />
                          Tạm dừng
                        </span>
                      )}
                    </div>

                    {conn.apiKey && (
                      <div className="flex items-center justify-between">
                        <span className="text-slate-500 text-[11px]">API Key:</span>
                        <span className="font-mono text-[11px] text-slate-400 bg-black/40 px-2 py-0.5 rounded border border-white/10">
                          {conn.apiKey}
                        </span>
                      </div>
                    )}

                    <div className="flex items-center justify-between">
                      <span className="text-slate-500 text-[11px]">Thứ tự ưu tiên:</span>
                      <span className="text-[11px] font-mono text-slate-300">#{conn.priority || 1}</span>
                    </div>

                    {hasError && conn.lastError && (
                      <div className="mt-1.5 p-2 rounded bg-rose-950/40 border border-white/10 text-[11px] text-rose-300 break-words leading-tight">
                        {conn.lastError}
                      </div>
                    )}

                    {associatedRoutes.length > 0 && (
                      <div className="flex items-center justify-between pt-1 border-t border-white/10">
                        <span className="text-slate-500 text-[11px]">Đang phục vụ:</span>
                        <div className="flex gap-1 flex-wrap justify-end max-w-[180px]">
                          {associatedRoutes.map((rName) => (
                            <span
                              key={rName}
                              className="text-[10px] font-mono bg-purple-500/10 text-purple-400 border border-white/10 px-1.5 py-0.5 rounded"
                            >
                              {rName}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                </div>

                {/* Actions */}
                <div className="flex items-center justify-between pt-2.5 border-t border-white/10">
                  <button
                    onClick={() => onTestConnection(conn)}
                    disabled={isTesting || !isEnabled}
                    className="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded-lg text-slate-300 hover:text-white hover:bg-slate-800 transition disabled:opacity-40"
                    title="Kiểm tra kết nối"
                  >
                    {isTesting ? (
                      <div className="w-3.5 h-3.5 border-2 border-white/10 border-t-indigo-400 rounded-full animate-spin" />
                    ) : (
                      <svg className="w-3.5 h-3.5 text-blue-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <polygon points="5 3 19 12 5 21 5 3" />
                      </svg>
                    )}
                    {isTesting ? "Đang test..." : "Kiểm tra"}
                  </button>

                  <div className="flex items-center gap-1">
                    <button
                      onClick={() => onEditConnection(conn)}
                      className="px-2 py-1 text-xs font-medium text-slate-400 hover:text-white rounded-lg hover:bg-slate-800/80 transition"
                    >
                      Sửa
                    </button>
                    <button
                      onClick={() => handleDelete(conn)}
                      disabled={isActionLoading}
                      className="px-2 py-1 text-xs font-medium text-rose-400 hover:text-rose-300 rounded-lg hover:bg-rose-500/10 transition"
                    >
                      Xóa
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
