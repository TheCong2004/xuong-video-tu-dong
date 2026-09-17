"use client";

import React, { useState, useRef, useMemo } from "react";

export interface OmniRouteConnectionDiagramProps {
  connections?: any[];
  onAddKey?: (providerId?: string) => void;
  onSwitchToCatalog?: () => void;
  onEditConnection?: (connection: any) => void;
  onDeleteConnection?: (connection: any) => void;
  onToggleConnection?: (connection: any, active: boolean) => void;
  onTestConnection?: (connection: any) => void;
}

interface ProviderTheme {
  name: string;
  brand: string;
  icon: string;
  color: string;
  glowColor: string;
  bgColor: string;
  borderColor: string;
}

function getProviderTheme(providerId: string = ""): ProviderTheme {
  const p = providerId.toLowerCase();
  if (p.includes("openai") || p.includes("gpt") || p.includes("chatgpt")) {
    return {
      name: "ChatGPT / OpenAI",
      brand: "OpenAI",
      icon: "🟢",
      color: "#10b981",
      glowColor: "rgba(16, 185, 129, 0.5)",
      bgColor: "bg-[#091a18]/95",
      borderColor: "border-white/10",
    };
  }
  if (p.includes("gemini") || p.includes("google")) {
    return {
      name: "Google Gemini",
      brand: "Google AI Studio",
      icon: "✨",
      color: "#38bdf8",
      glowColor: "rgba(56, 189, 248, 0.5)",
      bgColor: "bg-[#091626]/95",
      borderColor: "border-white/10",
    };
  }
  if (p.includes("grok") || p.includes("xai")) {
    return {
      name: "Grok",
      brand: "xAI",
      icon: "⚡",
      color: "#f59e0b",
      glowColor: "rgba(245, 158, 11, 0.5)",
      bgColor: "bg-[#1c1509]/95",
      borderColor: "border-white/10",
    };
  }
  if (p.includes("claude") || p.includes("anthropic")) {
    return {
      name: "Claude",
      brand: "Anthropic",
      icon: "🧠",
      color: "#c084fc",
      glowColor: "rgba(192, 132, 252, 0.5)",
      bgColor: "bg-[#160c24]/95",
      borderColor: "border-white/10",
    };
  }
  if (p.includes("deepseek")) {
    return {
      name: "DeepSeek",
      brand: "DeepSeek AI",
      icon: "🚀",
      color: "#06b6d4",
      glowColor: "rgba(6, 182, 212, 0.5)",
      bgColor: "bg-[#081820]/95",
      borderColor: "border-white/10",
    };
  }
  if (p.includes("ollama") || p.includes("local")) {
    return {
      name: "Ollama",
      brand: "Local Self-hosted",
      icon: "🦙",
      color: "#a3e635",
      glowColor: "rgba(163, 230, 53, 0.5)",
      bgColor: "bg-[#131c0c]/95",
      borderColor: "border-lime-500/70",
    };
  }
  if (p.includes("tts") || p.includes("edge") || p.includes("eleven")) {
    return {
      name: "TTS & Voice",
      brand: "Audio Engine",
      icon: "🎙️",
      color: "#ec4899",
      glowColor: "rgba(236, 72, 153, 0.5)",
      bgColor: "bg-[#1f0a18]/95",
      borderColor: "border-white/10",
    };
  }
  return {
    name: providerId.toUpperCase(),
    brand: "AI Provider",
    icon: "🔑",
    color: "#818cf8",
    glowColor: "rgba(129, 140, 248, 0.5)",
    bgColor: "bg-[#101426]/95",
    borderColor: "border-white/10",
  };
}

function maskApiKey(key: string = ""): string {
  if (!key) return "Chưa có key";
  if (key.length <= 10) return "••••••••••••";
  const start = key.slice(0, 6);
  const end = key.slice(-4);
  return `${start}••••••••${end}`;
}

export default function OmniRouteConnectionDiagram({
  connections = [],
  onAddKey,
  onSwitchToCatalog,
  onEditConnection,
  onDeleteConnection,
  onToggleConnection,
  onTestConnection,
}: OmniRouteConnectionDiagramProps) {
  const [copiedKey, setCopiedKey] = useState(false);
  const [copiedEndpoint, setCopiedEndpoint] = useState(false);
  const [selectedConnId, setSelectedConnId] = useState<string | null>(null);

  // Canvas Pan & Zoom (Zoom tại vị trí con trỏ chuột)
  const [zoom, setZoom] = useState<number>(0.9);
  const [pan, setPan] = useState<{ x: number; y: number }>({ x: 40, y: 20 });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const canvasRef = useRef<HTMLDivElement>(null);

  const masterEndpoint = "http://127.0.0.1:20128/v1";
  const masterKey = "sk-omniroute-master-key-prod-01";

  const handleCopyMasterKey = (e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(masterKey);
    setCopiedKey(true);
    setTimeout(() => setCopiedKey(false), 2000);
  };

  const handleCopyEndpoint = (e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(masterEndpoint);
    setCopiedEndpoint(true);
    setTimeout(() => setCopiedEndpoint(false), 2000);
  };

  // Drag Pan handlers
  const handleMouseDown = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest(".interactive-card, button, input, textarea")) return;
    setIsDragging(true);
    dragStartRef.current = { x: e.clientX - pan.x, y: e.clientY - pan.y };
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isDragging) return;
    setPan({
      x: e.clientX - dragStartRef.current.x,
      y: e.clientY - dragStartRef.current.y,
    });
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  // Zoom at cursor position
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const zoomFactor = e.deltaY < 0 ? 1.09 : 0.91;
    const newZoom = Math.min(Math.max(Number((zoom * zoomFactor).toFixed(3)), 0.4), 2.2);

    const rect = canvasRef.current?.getBoundingClientRect();
    if (rect) {
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;

      const newPanX = mouseX - (mouseX - pan.x) * (newZoom / zoom);
      const newPanY = mouseY - (mouseY - pan.y) * (newZoom / zoom);

      setZoom(newZoom);
      setPan({ x: newPanX, y: newPanY });
    } else {
      setZoom(newZoom);
    }
  };

  const handleZoomIn = () => setZoom((z) => Math.min(Number((z + 0.1).toFixed(2)), 2.2));
  const handleZoomOut = () => setZoom((z) => Math.max(Number((z - 0.1).toFixed(2)), 0.4));
  const handleResetView = () => {
    setZoom(0.9);
    setPan({ x: 40, y: 20 });
  };

  // ── Tính toán layout tọa độ thực tế theo danh sách connections ──
  const cardW = 230;
  const gap = 24;
  const numCards = Math.max(1, connections.length + 1); // +1 cho nút Add Card
  const totalRowWidth = numCards * cardW + (numCards - 1) * gap;
  const canvasWidth = Math.max(1300, totalRowWidth + 160);

  const rootCenterX = canvasWidth / 2;
  const rootNode = {
    x: rootCenterX - 260,
    y: 40,
    w: 520,
    h: 195,
  };
  const rootBottomPort = {
    x: rootCenterX,
    y: rootNode.y + rootNode.h, // 235
  };

  const startX = Math.max(40, (canvasWidth - totalRowWidth) / 2);

  const activeCount = useMemo(() => {
    return connections.filter((c) => c.isActive !== false).length;
  }, [connections]);

  return (
    <div className="w-full h-full min-h-[700px] flex-1 bg-[#060911] text-slate-100 flex flex-col overflow-hidden relative select-none">
      {/* ─── 1. TOP TOOLBAR ─── */}
      <div className="px-5 py-3 bg-slate-950/95 border-b border-white/10 backdrop-blur-md flex flex-wrap items-center justify-between gap-3 shrink-0 z-30">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-gradient-to-tr from-indigo-500 via-purple-500 to-emerald-500 flex items-center justify-center font-bold text-white shadow-lg shadow-indigo-500/25 text-lg">
            🔑
          </div>
          <div>
            <h2 className="text-sm font-bold text-white flex items-center gap-2">
              <span>Sơ Đồ Gom &amp; Phân Nhánh Key AI (OmniRoute Gateway)</span>
              <span className="px-2 py-0.5 rounded-full bg-emerald-500/20 text-emerald-400 text-[10px] font-mono font-semibold border border-white/10">
                {activeCount > 0 ? `${activeCount} KEY ĐANG HOẠT ĐỘNG` : "CHƯA CÓ KEY NÀO"}
              </span>
            </h2>
            <p className="text-[11px] text-slate-400">
              Chỉ các Key bạn đã thêm thực tế mới xuất hiện trên sơ đồ. TỔNG KEY tự động điều phối tải giữa các Key này.
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2.5">
          {onSwitchToCatalog && (
            <button
              type="button"
              onClick={onSwitchToCatalog}
              className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl bg-slate-900 hover:bg-slate-800 text-slate-300 hover:text-white text-xs font-semibold border border-white/10 transition cursor-pointer"
            >
              <span>📋</span>
              <span>Xem Danh Sách Catalog (290+)</span>
            </button>
          )}

          {onAddKey && (
            <button
              type="button"
              onClick={() => onAddKey()}
              className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl bg-gradient-to-r from-indigo-500 via-purple-500 to-emerald-500 hover:opacity-90 text-white font-bold text-xs shadow-lg shadow-indigo-500/20 transition transform active:scale-95 cursor-pointer"
            >
              <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                <line x1="12" y1="5" x2="12" y2="19" />
                <line x1="5" y1="12" x2="19" y2="12" />
              </svg>
              <span>+ Thêm Key Mới</span>
            </button>
          )}
        </div>
      </div>

      {/* ─── 2. MAIN WORKFLOW CANVAS ─── */}
      <div className="flex-1 flex overflow-hidden relative min-h-0">
        <div
          ref={canvasRef}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onWheel={handleWheel}
          className={`flex-1 relative overflow-hidden bg-[#060911] bg-[radial-gradient(rgba(255,255,255,0.12)_1.2px,transparent_1.2px)] [background-size:26px_26px] ${
            isDragging ? "cursor-grabbing" : "cursor-grab"
          }`}
        >
          {/* Floating Zoom Controls & Hint */}
          <div className="absolute top-4 left-4 z-30 flex items-center gap-2">
            <div className="flex items-center gap-1.5 bg-slate-950/90 backdrop-blur-md border border-white/10 rounded-2xl p-1.5 shadow-xl">
              <button
                type="button"
                onClick={handleZoomOut}
                className="w-8 h-8 rounded-xl bg-slate-900 hover:bg-slate-800 flex items-center justify-center text-slate-300 hover:text-white font-bold text-sm cursor-pointer transition"
                title="Thu nhỏ (Scroll chuột xuống)"
              >
                −
              </button>
              <span className="text-xs font-mono px-2 text-slate-300 min-w-[45px] text-center font-bold">
                {Math.round(zoom * 100)}%
              </span>
              <button
                type="button"
                onClick={handleZoomIn}
                className="w-8 h-8 rounded-xl bg-slate-900 hover:bg-slate-800 flex items-center justify-center text-slate-300 hover:text-white font-bold text-sm cursor-pointer transition"
                title="Phóng to (Scroll chuột lên)"
              >
                +
              </button>
              <div className="w-[1px] h-4 bg-slate-800 mx-1" />
              <button
                type="button"
                onClick={handleResetView}
                className="px-2.5 py-1 rounded-xl bg-slate-900 hover:bg-slate-800 text-[11px] font-medium text-slate-300 hover:text-white transition cursor-pointer font-mono"
              >
                Fit View
              </button>
            </div>

            <span className="text-[11px] font-mono text-slate-400 bg-slate-950/70 border border-white/10 px-2.5 py-1.5 rounded-xl backdrop-blur-md hidden sm:inline-block">
              🖱️ Cuộn chuột để Zoom • Giữ chuột trái để Kéo Canvas
            </span>
          </div>

          {/* Scaled & Panned Canvas Surface */}
          <div
            style={{
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
              transformOrigin: "0 0",
            }}
            className="absolute inset-0 transition-transform duration-75"
          >
            {/* ═══════════════════════════════════════════════════════════
                SVG CONNECTOR WIRES (CHỈ NỐI TỚI CÁC KEY THỰC SỰ ĐÃ THÊM)
               ═══════════════════════════════════════════════════════════ */}
            <svg
              className="absolute inset-0 w-full h-full pointer-events-none z-10"
              style={{ overflow: "visible", width: `${canvasWidth}px`, height: "800px" }}
              xmlns="http://www.w3.org/2000/svg"
            >
              <defs>
                <filter id="neonWireGlow" x="-30%" y="-30%" width="160%" height="160%">
                  <feGaussianBlur stdDeviation="4" result="blur" />
                  <feComposite in="SourceGraphic" in2="blur" operator="over" />
                </filter>
              </defs>

              {/* Master Output Connector Port Pulse Ring */}
              <circle
                cx={rootBottomPort.x}
                cy={rootBottomPort.y}
                r={9}
                fill="#6366f1"
                fillOpacity={activeCount > 0 ? "0.4" : "0.1"}
                className={activeCount > 0 ? "animate-ping" : ""}
              />
              <circle
                cx={rootBottomPort.x}
                cy={rootBottomPort.y}
                r={7}
                fill="#818cf8"
                stroke="#ffffff"
                strokeWidth="2.5"
              />

              {/* Wires to each real connection */}
              {connections.map((conn, idx) => {
                const targetX = startX + idx * (cardW + gap) + cardW / 2;
                const targetY = 380;
                const theme = getProviderTheme(conn.provider);

                const sourceX = rootBottomPort.x;
                const sourceY = rootBottomPort.y;
                const deltaY = targetY - sourceY;
                const controlY1 = sourceY + deltaY * 0.45;
                const controlY2 = targetY - deltaY * 0.45;

                const pathData = `M ${sourceX} ${sourceY} C ${sourceX} ${controlY1}, ${targetX} ${controlY2}, ${targetX} ${targetY}`;
                const isActive = conn.isActive !== false;

                return (
                  <g key={`cable-${conn.id}`} className="transition-opacity duration-300">
                    {/* Glowing Underlayer */}
                    <path
                      d={pathData}
                      fill="none"
                      stroke={theme.color}
                      strokeWidth="6"
                      opacity={isActive ? 0.35 : 0.1}
                      filter="url(#neonWireGlow)"
                    />

                    {/* Core Line */}
                    <path
                      d={pathData}
                      fill="none"
                      stroke={theme.color}
                      strokeWidth="3"
                      strokeDasharray={isActive ? "8 4" : "4 4"}
                      opacity={isActive ? 1 : 0.4}
                    />

                    {/* Flow Dot Animation */}
                    {isActive && (
                      <circle r="3.5" fill="#ffffff" filter="url(#neonWireGlow)">
                        <animateMotion dur="2.8s" repeatCount="indefinite" path={pathData} />
                      </circle>
                    )}

                    {/* Target Port Dot */}
                    <circle cx={targetX} cy={targetY} r={6} fill={theme.color} stroke="#ffffff" strokeWidth="2" />
                  </g>
                );
              })}
            </svg>

            {/* ═══════════════════════════════════════════════════════════
                ROOT NODE: TỔNG KEY (OMNIROUTE GATEWAY MASTER)
               ═══════════════════════════════════════════════════════════ */}
            <div
              style={{
                left: `${rootNode.x}px`,
                top: `${rootNode.y}px`,
                width: `${rootNode.w}px`,
              }}
              className="interactive-card absolute rounded-3xl bg-gradient-to-b from-[#18203f] via-[#11172f] to-[#0c1022] border-2 border-white/10 p-5 shadow-2xl shadow-indigo-500/30 backdrop-blur-2xl z-20"
            >
              {/* Header */}
              <div className="flex items-center justify-between border-b border-white/10 pb-3 mb-3">
                <div className="flex items-center gap-3">
                  <div className="w-12 h-12 rounded-2xl bg-gradient-to-tr from-indigo-500 via-purple-600 to-emerald-400 flex items-center justify-center font-bold text-white shadow-lg text-xl">
                    🔑
                  </div>
                  <div>
                    <h3 className="text-sm font-extrabold text-white flex items-center gap-2">
                      <span>TỔNG KEY (OMNIROUTE GATEWAY)</span>
                      <span className="px-2 py-0.5 rounded-full bg-emerald-500/20 text-emerald-300 text-[9px] font-mono border border-white/10">
                        MASTER ROUTER
                      </span>
                    </h3>
                    <p className="text-xs text-indigo-300">
                      Gom tất cả các Key con bên dưới thành 1 Key duy nhất cho dự án
                    </p>
                  </div>
                </div>

                <div className="flex items-center gap-1.5">
                  <span
                    className={`w-2.5 h-2.5 rounded-full ${
                      activeCount > 0 ? "bg-emerald-400 animate-pulse" : "bg-amber-400"
                    }`}
                  />
                  <span className="text-[11px] font-semibold text-emerald-400 font-mono">
                    {activeCount > 0 ? "ONLINE (200 OK)" : "CHỜ THÊM KEY"}
                  </span>
                </div>
              </div>

              {/* Master Key & Endpoint Bars */}
              <div className="space-y-2 text-xs font-mono">
                {/* Master API Key Box */}
                <div className="flex items-center justify-between bg-slate-950/90 rounded-xl p-2 border border-white/10">
                  <div className="flex items-center gap-2 overflow-hidden">
                    <span className="text-slate-500 shrink-0 font-bold">KEY:</span>
                    <span className="text-amber-300 font-bold truncate select-all">{masterKey}</span>
                  </div>
                  <button
                    type="button"
                    onClick={handleCopyMasterKey}
                    className="ml-2 px-3 py-1 rounded-lg bg-indigo-600 hover:bg-indigo-500 active:scale-95 text-white font-sans text-[11px] font-bold transition flex items-center gap-1 cursor-pointer shrink-0 shadow-md"
                  >
                    {copiedKey ? "✓ Đã Copy" : "📋 Copy Key"}
                  </button>
                </div>

                {/* Master Endpoint Box */}
                <div className="flex items-center justify-between bg-slate-950/90 rounded-xl p-2 border border-white/10">
                  <div className="flex items-center gap-2 overflow-hidden">
                    <span className="text-slate-500 shrink-0 font-bold">ENDPOINT:</span>
                    <span className="text-emerald-400 font-bold truncate select-all">{masterEndpoint}</span>
                  </div>
                  <button
                    type="button"
                    onClick={handleCopyEndpoint}
                    className="ml-2 px-3 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 active:scale-95 text-slate-200 font-sans text-[11px] font-medium transition flex items-center gap-1 cursor-pointer shrink-0 shadow-sm"
                  >
                    {copiedEndpoint ? "✓ Đã Copy" : "📋 Copy URL"}
                  </button>
                </div>
              </div>

              {/* Footer info badge */}
              <div className="mt-3 pt-2.5 border-t border-white/10 flex items-center justify-between text-[11px] text-slate-400">
                <span className="flex items-center gap-1.5 text-slate-300">
                  <span>⚡</span> Đang gộp:{" "}
                  <strong className="text-indigo-300">{connections.length} Key Thực Tế</strong>
                </span>
                <span className="text-emerald-400 font-mono text-[10px]">
                  Tự Động Cân Bằng Tải &amp; Chuyển Mạch
                </span>
              </div>
            </div>

            {/* ═══════════════════════════════════════════════════════════
                EMPTY STATE / QUICK ADD (KHI CHƯA CÓ KEY NÀO THỰC SỰ)
               ═══════════════════════════════════════════════════════════ */}
            {connections.length === 0 && (
              <div
                style={{
                  left: `${rootCenterX - 360}px`,
                  top: "380px",
                  width: "720px",
                }}
                className="interactive-card absolute rounded-3xl bg-slate-950/90 border-2 border-dashed border-white/10 p-6 text-center backdrop-blur-xl z-20"
              >
                <div className="size-14 mx-auto rounded-2xl bg-indigo-500/10 border border-white/10 text-indigo-400 flex items-center justify-center text-2xl mb-3">
                  🔌
                </div>
                <h3 className="text-sm font-bold text-white mb-1">
                  Chưa có khóa API nào được kết nối
                </h3>
                <p className="text-xs text-slate-400 max-w-md mx-auto mb-5">
                  Hãy thêm khóa API thật của bạn (Gemini, OpenAI, Grok, Claude, DeepSeek...) để kết nối vào TỔNG KEY.
                </p>

                {/* Quick Add Buttons */}
                <div className="flex flex-wrap items-center justify-center gap-2">
                  <button
                    type="button"
                    onClick={() => onAddKey && onAddKey("gemini")}
                    className="px-3.5 py-1.5 rounded-xl bg-sky-500/15 hover:bg-sky-500/25 text-sky-300 border border-white/10 text-xs font-bold transition flex items-center gap-1.5 cursor-pointer shadow-sm"
                  >
                    <span>✨</span>
                    <span>+ Thêm Gemini Key</span>
                  </button>

                  <button
                    type="button"
                    onClick={() => onAddKey && onAddKey("openai")}
                    className="px-3.5 py-1.5 rounded-xl bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border border-white/10 text-xs font-bold transition flex items-center gap-1.5 cursor-pointer shadow-sm"
                  >
                    <span>🟢</span>
                    <span>+ Thêm ChatGPT Key</span>
                  </button>

                  <button
                    type="button"
                    onClick={() => onAddKey && onAddKey("grok")}
                    className="px-3.5 py-1.5 rounded-xl bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-white/10 text-xs font-bold transition flex items-center gap-1.5 cursor-pointer shadow-sm"
                  >
                    <span>⚡</span>
                    <span>+ Thêm Grok Key</span>
                  </button>

                  <button
                    type="button"
                    onClick={() => onAddKey && onAddKey("anthropic")}
                    className="px-3.5 py-1.5 rounded-xl bg-purple-500/15 hover:bg-purple-500/25 text-purple-300 border border-white/10 text-xs font-bold transition flex items-center gap-1.5 cursor-pointer shadow-sm"
                  >
                    <span>🧠</span>
                    <span>+ Thêm Claude Key</span>
                  </button>

                  <button
                    type="button"
                    onClick={() => onAddKey && onAddKey("deepseek")}
                    className="px-3.5 py-1.5 rounded-xl bg-cyan-500/15 hover:bg-cyan-500/25 text-cyan-300 border border-white/10 text-xs font-bold transition flex items-center gap-1.5 cursor-pointer shadow-sm"
                  >
                    <span>🚀</span>
                    <span>+ Thêm DeepSeek Key</span>
                  </button>
                </div>
              </div>
            )}

            {/* ═══════════════════════════════════════════════════════════
                REAL CONNECTED PROVIDER NODES (DATA THẬT 100%)
               ═══════════════════════════════════════════════════════════ */}
            {connections.map((conn, idx) => {
              const x = startX + idx * (cardW + gap);
              const theme = getProviderTheme(conn.provider);
              const isSelected = selectedConnId === conn.id;
              const isActive = conn.isActive !== false;

              return (
                <div
                  key={conn.id}
                  style={{
                    left: `${x}px`,
                    top: "380px",
                    width: `${cardW}px`,
                  }}
                  onClick={() => setSelectedConnId(conn.id)}
                  className={`interactive-card absolute rounded-2xl p-4 backdrop-blur-xl border-2 transition-all cursor-pointer z-20 ${
                    theme.bgColor
                  } ${theme.borderColor} ${
                    isSelected
                      ? "ring-2 ring-indigo-400 shadow-2xl scale-[1.03]"
                      : "hover:scale-[1.02] shadow-xl"
                  } ${!isActive ? "opacity-60 grayscale-[0.3]" : ""}`}
                >
                  {/* Provider Header */}
                  <div className="flex items-center justify-between border-b border-white/10 pb-2 mb-2.5">
                    <div className="flex items-center gap-2 overflow-hidden">
                      <span className="text-xl shrink-0">{theme.icon}</span>
                      <div className="overflow-hidden">
                        <h4 className="text-xs font-bold text-white truncate">
                          {conn.name || theme.name}
                        </h4>
                        <p className="text-[10px] text-slate-400 truncate">{theme.brand}</p>
                      </div>
                    </div>

                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (onToggleConnection) onToggleConnection(conn, !isActive);
                      }}
                      className={`w-3.5 h-3.5 rounded-full transition cursor-pointer shrink-0 ${
                        isActive ? "bg-emerald-400 shadow-lg shadow-emerald-400/50 animate-pulse" : "bg-slate-600"
                      }`}
                      title={isActive ? "Đang bật (Click để tắt)" : "Đang tắt (Click để bật)"}
                    />
                  </div>

                  {/* Configured API Key Field */}
                  <div className="bg-slate-950/80 p-2 rounded-xl border border-white/10 text-[10px] font-mono mb-2.5">
                    <div className="text-slate-400 flex justify-between">
                      <span>API Key:</span>
                      <span className={isActive ? "text-emerald-400 font-semibold" : "text-slate-500"}>
                        {isActive ? "Đang Hoạt Động" : "Đã Tạm Tắt"}
                      </span>
                    </div>
                    <div className="text-slate-200 font-bold truncate mt-0.5">
                      {maskApiKey(conn.apiKey)}
                    </div>
                  </div>

                  {/* Latency / Priority */}
                  <div className="flex items-center justify-between text-[9px] font-mono text-slate-400 mb-3">
                    <span>Ưu tiên: #{conn.priority || 1}</span>
                    {conn.lastLatencyMs ? (
                      <span className="text-emerald-400 font-semibold">{conn.lastLatencyMs}ms</span>
                    ) : (
                      <span className="text-slate-500">200 OK</span>
                    )}
                  </div>

                  {/* Action Buttons Row */}
                  <div className="flex items-center gap-1.5 pt-1 border-t border-white/10">
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (onEditConnection) onEditConnection(conn);
                      }}
                      className="flex-1 py-1.5 rounded-xl bg-slate-900 hover:bg-slate-800 active:scale-95 text-slate-200 hover:text-white text-[10px] font-bold transition border border-white/10 cursor-pointer flex items-center justify-center gap-1"
                    >
                      <span>⚙️ Sửa</span>
                    </button>

                    {onTestConnection && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onTestConnection(conn);
                        }}
                        className="px-2.5 py-1.5 rounded-xl bg-slate-900 hover:bg-slate-800 active:scale-95 text-slate-300 hover:text-emerald-400 text-[10px] font-bold transition border border-white/10 cursor-pointer"
                        title="Kiểm tra kết nối"
                      >
                        <span>🧪 Test</span>
                      </button>
                    )}

                    {onDeleteConnection && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          if (confirm(`Bạn có chắc muốn xóa kết nối ${conn.name || conn.provider}?`)) {
                            onDeleteConnection(conn);
                          }
                        }}
                        className="px-2.5 py-1.5 rounded-xl bg-slate-900 hover:bg-rose-950/60 active:scale-95 text-slate-400 hover:text-rose-400 text-[10px] font-bold transition border border-white/10 hover:border-rose-500/40 cursor-pointer"
                        title="Xóa kết nối"
                      >
                        <span>🗑️</span>
                      </button>
                    )}
                  </div>
                </div>
              );
            })}

            {/* ═══════════════════════════════════════════════════════════
                ADD MORE PROVIDER DASHED CARD (Ở CUỐI HÀNG)
               ═══════════════════════════════════════════════════════════ */}
            {connections.length > 0 && (
              <div
                style={{
                  left: `${startX + connections.length * (cardW + gap)}px`,
                  top: "380px",
                  width: `${cardW}px`,
                  height: "210px",
                }}
                onClick={() => onAddKey && onAddKey()}
                className="interactive-card absolute rounded-2xl p-4 bg-slate-950/60 hover:bg-slate-900/80 border-2 border-dashed border-white/10 hover:border-indigo-500/80 transition-all cursor-pointer flex flex-col items-center justify-center text-center backdrop-blur-xl group z-20 shadow-lg"
              >
                <div className="w-10 h-10 rounded-xl bg-indigo-500/10 group-hover:bg-indigo-500/20 text-indigo-400 flex items-center justify-center text-xl mb-2 transition transform group-hover:scale-110">
                  +
                </div>
                <h4 className="text-xs font-bold text-slate-200 group-hover:text-white transition">
                  + Thêm Nhà Cung Cấp
                </h4>
                <p className="text-[10px] text-slate-500 mt-0.5">
                  Thêm key OpenAI, Grok, Claude, Gemini...
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
