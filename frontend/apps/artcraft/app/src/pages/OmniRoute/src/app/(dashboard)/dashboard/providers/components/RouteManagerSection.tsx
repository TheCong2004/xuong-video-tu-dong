"use client";

import { useState, useEffect, useCallback } from "react";
import Card from "@/shared/components/Card";
import Badge from "@/shared/components/Badge";
import Button from "@/shared/components/Button";
import Toggle from "@/shared/components/Toggle";
import Input from "@/shared/components/Input";
import Modal from "@/shared/components/Modal";
import ProviderIcon from "@/shared/components/ProviderIcon";

export interface ComboStepItem {
  model: string;
  provider: string;
  connectionId?: string | null;
  priority?: number;
  weight?: number;
  enabled?: boolean;
}

export interface ComboItem {
  id: string;
  name: string;
  strategy?: string;
  models?: ComboStepItem[];
  isActive?: boolean;
  config?: Record<string, unknown>;
  created_at?: string;
  updated_at?: string;
}

interface ProviderOption {
  id: string;
  name: string;
}

interface RouteManagerSectionProps {
  availableProviders?: ProviderOption[];
  onRoutesChange?: (routes: ComboItem[]) => void;
}

const STRATEGY_LABELS: Record<string, { label: string; desc: string; color: string }> = {
  priority: {
    label: "Ưu tiên (Fallback)",
    desc: "Gọi provider chính trước, tự động chuyển sang provider dự phòng nếu lỗi",
    color: "bg-blue-500/10 text-blue-400 border-white/10",
  },
  "round-robin": {
    label: "Luân phiên (Round-robin)",
    desc: "Phân phối đều các yêu cầu lần lượt giữa các provider/model",
    color: "bg-emerald-500/10 text-emerald-400 border-white/10",
  },
  weighted: {
    label: "Trọng số (Weighted)",
    desc: "Chia tỉ lệ lưu lượng truy cập theo trọng số được cấu hình",
    color: "bg-amber-500/10 text-amber-400 border-white/10",
  },
};

export default function RouteManagerSection({
  availableProviders = [],
  onRoutesChange,
}: RouteManagerSectionProps) {
  const ROUTES_CACHE_KEY = "omniroute_custom_routes";

  const getCachedRoutes = (): ComboItem[] => {
    try {
      const stored = localStorage.getItem(ROUTES_CACHE_KEY);
      if (stored) return JSON.parse(stored);
    } catch {}
    return [];
  };

  const saveCachedRoutes = (list: ComboItem[]) => {
    try {
      localStorage.setItem(ROUTES_CACHE_KEY, JSON.stringify(list));
    } catch {}
  };

  const [routes, setRoutes] = useState<ComboItem[]>(() => getCachedRoutes());
  const [loading, setLoading] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [editingRoute, setEditingRoute] = useState<ComboItem | null>(null);

  // Modal Form State
  const [formName, setFormName] = useState("");
  const [formStrategy, setFormStrategy] = useState("priority");
  const [formSteps, setFormSteps] = useState<ComboStepItem[]>([
    { provider: availableProviders[0]?.id || "gemini", model: "", priority: 1 },
  ]);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const fetchRoutes = useCallback(async () => {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 3500);

    try {
      const res = await fetch("/api/combos", {
        cache: "no-store",
        signal: controller.signal,
      });
      clearTimeout(timeoutId);

      if (res.ok) {
        const data = await res.json();
        const serverList: ComboItem[] = Array.isArray(data.combos) ? data.combos : [];
        if (serverList.length > 0) {
          setRoutes(serverList);
          saveCachedRoutes(serverList);
          onRoutesChange?.(serverList);
          return;
        }
      }
    } catch (err) {
      console.warn("[RouteManager] API check completed or offline:", err);
    } finally {
      clearTimeout(timeoutId);
      setLoading(false);
    }

    // Keep cached routes if server returned empty or offline
    const cached = getCachedRoutes();
    setRoutes(cached);
    onRoutesChange?.(cached);
  }, [onRoutesChange]);

  useEffect(() => {
    fetchRoutes();
  }, [fetchRoutes]);

  const openCreateModal = () => {
    setEditingRoute(null);
    setFormName("");
    setFormStrategy("priority");
    setFormSteps([
      {
        provider: availableProviders[0]?.id || "gemini",
        model: "",
        priority: 1,
      },
    ]);
    setFormError(null);
    setModalOpen(true);
  };

  const openEditModal = (route: ComboItem) => {
    setEditingRoute(route);
    setFormName(route.name);
    setFormStrategy(route.strategy || "priority");
    const steps = Array.isArray(route.models) && route.models.length > 0
      ? route.models.map((s, idx) => ({
          provider: s.provider || availableProviders[0]?.id || "gemini",
          model: s.model || "",
          priority: s.priority || idx + 1,
          weight: s.weight || 1,
        }))
      : [{ provider: availableProviders[0]?.id || "gemini", model: "", priority: 1 }];
    setFormSteps(steps);
    setFormError(null);
    setModalOpen(true);
  };

  const handleToggleRoute = async (route: ComboItem) => {
    const nextActive = route.isActive === false ? true : false;
    const updatedList = routes.map((r) =>
      r.id === route.id ? { ...r, isActive: nextActive } : r
    );
    setRoutes(updatedList);
    saveCachedRoutes(updatedList);
    onRoutesChange?.(updatedList);

    try {
      await fetch(`/api/combos/${route.id}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: route.name,
          isActive: nextActive,
        }),
      });
    } catch (err) {
      console.warn("[RouteManager] Server sync warning:", err);
    }
  };

  const handleDeleteRoute = async (route: ComboItem) => {
    if (!window.confirm(`Bạn có chắc chắn muốn xóa đường kết nối "${route.name}"?`)) {
      return;
    }
    const updatedList = routes.filter((r) => r.id !== route.id);
    setRoutes(updatedList);
    saveCachedRoutes(updatedList);
    onRoutesChange?.(updatedList);

    try {
      await fetch(`/api/combos/${route.id}`, {
        method: "DELETE",
      });
    } catch (err) {
      console.warn("[RouteManager] Server sync warning:", err);
    }
  };

  const handleSaveRoute = async () => {
    if (!formName.trim()) {
      setFormError("Vui lòng nhập tên đường kết nối");
      return;
    }

    const validSteps = formSteps.filter((s) => s.provider && s.model.trim());
    if (validSteps.length === 0) {
      setFormError("Vui lòng cấu hình ít nhất một provider và model đích");
      return;
    }

    setSaving(true);
    setFormError(null);

    const routeData: ComboItem = {
      id: editingRoute ? editingRoute.id : `route-${Date.now()}`,
      name: formName.trim(),
      strategy: formStrategy,
      isActive: editingRoute ? editingRoute.isActive !== false : true,
      models: validSteps.map((s, idx) => ({
        provider: s.provider,
        model: s.model.trim(),
        priority: s.priority || idx + 1,
        weight: s.weight || 1,
      })),
      updated_at: new Date().toISOString(),
    };

    // Update state and cache immediately (optimistic UI)
    let updatedList: ComboItem[];
    if (editingRoute) {
      updatedList = routes.map((r) => (r.id === editingRoute.id ? routeData : r));
    } else {
      updatedList = [...routes, routeData];
    }
    setRoutes(updatedList);
    saveCachedRoutes(updatedList);
    onRoutesChange?.(updatedList);

    // Sync to backend
    try {
      const payload = {
        name: routeData.name,
        strategy: routeData.strategy,
        models: routeData.models,
      };

      if (editingRoute) {
        await fetch(`/api/combos/${editingRoute.id}`, {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        });
      } else {
        await fetch("/api/combos", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        });
      }
    } catch (err) {
      console.warn("[RouteManager] Backend sync note:", err);
    } finally {
      setSaving(false);
      setModalOpen(false);
    }
  };

  const addStep = () => {
    setFormSteps((prev) => [
      ...prev,
      {
        provider: availableProviders[0]?.id || "gemini",
        model: "",
        priority: prev.length + 1,
        weight: 1,
      },
    ]);
  };

  const removeStep = (index: number) => {
    setFormSteps((prev) => prev.filter((_, i) => i !== index));
  };

  const updateStep = (index: number, field: keyof ComboStepItem, val: unknown) => {
    setFormSteps((prev) =>
      prev.map((step, i) => (i === index ? { ...step, [field]: val } : step))
    );
  };

  return (
    <section aria-labelledby="omniroute-routes-heading" className="space-y-4">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2
            id="omniroute-routes-heading"
            className="text-xl font-semibold text-white flex items-center gap-2"
          >
            <span>Quản lý đường kết nối (Routes)</span>
            <span className="text-xs font-normal text-slate-400 bg-[#0b0f17] px-2 py-0.5 rounded-full border border-white/10">
              {routes.length} đường kết nối
            </span>
          </h2>
          <p className="text-xs text-slate-400 mt-1">
            Điều phối các tác vụ hệ thống (Tạo video, sinh kịch bản, xử lý ảnh...) qua OmniRouter đến các Provider và Model đích với chuỗi ưu tiên và dự phòng (Fallback).
          </p>
        </div>
        <Button
          onClick={openCreateModal}
          size="sm"
          className="self-start sm:self-auto bg-indigo-600 text-white hover:bg-indigo-500/90"
        >
          + Thêm đường kết nối
        </Button>
      </div>

      {loading && routes.length === 0 ? (
        <Card padding="md" className="flex items-center justify-center py-10 text-slate-400">
          <div className="flex flex-col items-center gap-2">
            <div className="h-6 w-6 animate-spin rounded-full border-2 border-white/10 border-t-primary" />
            <span className="text-xs">Đang tải danh sách đường kết nối...</span>
          </div>
        </Card>
      ) : routes.length === 0 ? (
        <Card padding="lg" className="border-dashed border-white/10 flex flex-col items-center justify-center py-12 text-center">
          <div className="size-12 rounded-2xl bg-[#0b0f17] flex items-center justify-center text-slate-400 mb-3 border border-white/10">
            <span className="material-symbols-outlined text-2xl">alt_route</span>
          </div>
          <h3 className="text-sm font-semibold text-white mb-1">Chưa cấu hình đường kết nối nào</h3>
          <p className="text-xs text-slate-400 max-w-md mb-4">
            Tạo đường kết nối để chỉ định chức năng của hệ thống sẽ sử dụng Provider và Model AI nào, kèm cơ chế tự động chuyển sang mô hình dự phòng khi gặp lỗi.
          </p>
          <Button onClick={openCreateModal} size="sm">
            + Tạo đường kết nối đầu tiên
          </Button>
        </Card>
      ) : (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {routes.map((route) => {
            const isEnabled = route.isActive !== false;
            const strategyInfo = STRATEGY_LABELS[route.strategy || "priority"] || STRATEGY_LABELS.priority;
            const steps = Array.isArray(route.models) ? route.models : [];

            return (
              <Card
                key={route.id}
                padding="md"
                className={`relative flex flex-col justify-between border transition-all ${
                  isEnabled
                    ? "border-white/10 hover:border-slate-700 bg-bg"
                    : "border-white/10 bg-[#0b0f17]/40 opacity-75"
                }`}
              >
                <div>
                  {/* Top Bar: Route Name, Strategy, Active Toggle */}
                  <div className="flex items-start justify-between gap-2 mb-3">
                    <div className="space-y-1">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="text-base font-bold text-white font-mono">
                          {route.name}
                        </span>
                        <span
                          className={`text-[10px] px-2 py-0.5 rounded-md font-medium border ${strategyInfo.color}`}
                        >
                          {strategyInfo.label}
                        </span>
                        <Badge
                          variant={isEnabled ? "success" : "neutral"}
                          size="xs"
                        >
                          {isEnabled ? "Đang hoạt động" : "Tạm dừng"}
                        </Badge>
                      </div>
                      <p className="text-[11px] text-slate-400">
                        ID: {route.id}
                      </p>
                    </div>

                    <div className="flex items-center gap-3">
                      <Toggle
                        checked={isEnabled}
                        onChange={() => handleToggleRoute(route)}
                        title={isEnabled ? "Tắt đường kết nối" : "Bật đường kết nối"}
                      />
                    </div>
                  </div>

                  {/* Flow Diagram Representation */}
                  <div className="my-3 p-3 rounded-xl bg-slate-800/40 border border-white/10 space-y-2">
                    <div className="text-[11px] font-medium text-slate-400 flex items-center justify-between">
                      <span>Luồng kết nối đích:</span>
                      <span className="text-[10px] text-slate-400">{steps.length} bước đích</span>
                    </div>

                    <div className="space-y-2">
                      {steps.map((step, idx) => (
                        <div
                          key={`${route.id}-step-${idx}`}
                          className="flex items-center gap-2 text-xs bg-[#131926] p-2 rounded-lg border border-white/10"
                        >
                          <span className="size-5 rounded-full bg-indigo-500/10 text-indigo-400 text-[10px] font-bold flex items-center justify-center shrink-0">
                            #{step.priority || idx + 1}
                          </span>

                          <span className="text-slate-400 font-medium shrink-0">
                            {idx === 0 ? "Chính:" : "Dự phòng:"}
                          </span>

                          <div className="flex items-center gap-1.5 shrink-0">
                            <ProviderIcon providerId={step.provider} className="size-4" />
                            <span className="font-semibold text-white capitalize">
                              {step.provider}
                            </span>
                          </div>

                          <span className="text-slate-400">→</span>

                          <span className="font-mono text-white truncate max-w-[200px] bg-[#0b0f17] px-1.5 py-0.5 rounded border border-white/10">
                            {step.model || "(Mặc định)"}
                          </span>

                          {step.weight && step.weight > 1 && (
                            <span className="text-[10px] text-slate-400 ml-auto">
                              Trọng số: {step.weight}
                            </span>
                          )}
                        </div>
                      ))}
                    </div>
                  </div>
                </div>

                {/* Footer Controls */}
                <div className="flex items-center justify-end gap-2 pt-2 border-t border-white/10">
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={() => openEditModal(route)}
                    className="text-xs"
                  >
                    Chỉnh sửa
                  </Button>
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={() => handleDeleteRoute(route)}
                    className="text-xs text-red-400 hover:text-red-300 hover:bg-red-500/10"
                  >
                    Xóa
                  </Button>
                </div>
              </Card>
            );
          })}
        </div>
      )}

      {/* Modal: Thêm / Sửa đường kết nối */}
      <Modal
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        title={editingRoute ? `Chỉnh sửa đường kết nối "${editingRoute.name}"` : "Tạo đường kết nối mới"}
      >
        <div className="space-y-4 py-2">
          {formError && (
            <div className="p-3 rounded-lg bg-red-500/10 border border-white/10 text-xs text-red-400">
              {formError}
            </div>
          )}

          <div>
            <label className="block text-xs font-semibold text-white mb-1">
              Tên đường kết nối / Chức năng *
            </label>
            <Input
              value={formName}
              onChange={(e) => setFormName(e.target.value)}
              placeholder="VD: video-generation, text-generation, script-writing..."
              className="w-full text-xs font-mono"
            />
            <p className="text-[11px] text-slate-400 mt-1">
              Tên định danh được hệ thống hoặc caller gọi tới qua OmniRouter.
            </p>
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-200 mb-1.5">
              Chiến lược điều phối (Routing Strategy)
            </label>
            <select
              value={formStrategy}
              onChange={(e) => setFormStrategy(e.target.value)}
              className="w-full rounded-xl border border-white/10 bg-[#10141e] px-3.5 py-2.5 text-xs text-white font-medium focus:outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/20"
            >
              <option value="priority" className="bg-[#10141e] text-white">Ưu tiên (Fallback) — Chuyển dự phòng khi lỗi</option>
              <option value="round-robin" className="bg-[#10141e] text-white">Luân phiên (Round-robin) — Phân phối đều</option>
              <option value="weighted" className="bg-[#10141e] text-white">Trọng số (Weighted) — Tỉ lệ lưu lượng</option>
            </select>
          </div>

          <div>
            <div className="flex items-center justify-between mb-2">
              <label className="text-xs font-semibold text-slate-200">
                Danh sách Provider & Model đích ({formSteps.length})
              </label>
              <Button size="xs" variant="ghost" onClick={addStep} className="text-xs text-indigo-400 hover:text-indigo-300">
                + Thêm bước dự phòng
              </Button>
            </div>

            <div className="space-y-3 max-h-[260px] overflow-y-auto pr-1">
              {formSteps.map((step, idx) => (
                <div
                  key={`modal-step-${idx}`}
                  className="p-3.5 rounded-xl bg-[#10141e] border border-white/10 space-y-2.5 relative"
                >
                  <div className="flex items-center justify-between text-xs">
                    <span className="font-semibold text-white flex items-center gap-1.5">
                      <span className="size-4 rounded-full bg-indigo-500/20 text-indigo-400 text-[10px] flex items-center justify-center font-bold">
                        {idx + 1}
                      </span>
                      {idx === 0 ? "Mục tiêu chính" : `Mục tiêu dự phòng #${idx}`}
                    </span>
                    {formSteps.length > 1 && (
                      <button
                        type="button"
                        onClick={() => removeStep(idx)}
                        className="text-[11px] font-medium text-rose-400 hover:text-rose-300"
                      >
                        Gỡ bỏ
                      </button>
                    )}
                  </div>

                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                    <div>
                      <span className="text-[11px] font-medium text-slate-300 block mb-1">Nhà cung cấp (Provider):</span>
                      <select
                        value={step.provider}
                        onChange={(e) => updateStep(idx, "provider", e.target.value)}
                        className="w-full rounded-lg border border-white/10 bg-[#161f2e] px-2.5 py-2 text-xs text-white font-medium focus:outline-none focus:border-indigo-500 capitalize"
                      >
                        {availableProviders.map((p) => (
                          <option key={p.id} value={p.id} className="bg-[#10141e] text-white">
                            {p.name} ({p.id})
                          </option>
                        ))}
                      </select>
                    </div>

                    <div>
                      <span className="text-[11px] font-medium text-slate-300 block mb-1">Mô hình đích (Model):</span>
                      <Input
                        value={step.model}
                        onChange={(e) => updateStep(idx, "model", e.target.value)}
                        placeholder="VD: gemini-1.5-pro, gpt-4o..."
                        className="w-full text-xs font-mono"
                      />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 pt-4 border-t border-white/10">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setModalOpen(false)}
              disabled={saving}
            >
              Hủy
            </Button>
            <Button
              size="sm"
              onClick={handleSaveRoute}
              disabled={saving}
              className="bg-indigo-600 hover:bg-indigo-500 text-white font-semibold shadow-md shadow-indigo-600/20"
            >
              {saving ? "Đang lưu..." : editingRoute ? "Cập nhật đường kết nối" : "Tạo đường kết nối"}
            </Button>
          </div>
        </div>
      </Modal>
    </section>
  );
}
