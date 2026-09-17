"use client";

import { useState, useMemo } from "react";
import Modal from "@/shared/components/Modal";
import ProviderIcon from "@/shared/components/ProviderIcon";
import type { ProviderEntry } from "../providerPageUtils";

interface ProviderSelectorModalProps {
  isOpen: boolean;
  onClose: () => void;
  providerEntries: ProviderEntry<any>[];
  onSelectProvider: (entry: ProviderEntry<any>) => void;
}

export default function ProviderSelectorModal({
  isOpen,
  onClose,
  providerEntries = [],
  onSelectProvider,
}: ProviderSelectorModalProps) {
  const [search, setSearch] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<string>("all");

  const categories = [
    { id: "all", label: "Tất cả" },
    { id: "apikey", label: "API Key" },
    { id: "oauth", label: "OAuth" },
    { id: "webcookie", label: "Web Session" },
    { id: "compatible", label: "Tương thích" },
  ];

  const filteredEntries = useMemo(() => {
    return providerEntries.filter((entry) => {
      const name = (entry.provider?.name || entry.providerId).toLowerCase();
      const id = entry.providerId.toLowerCase();
      const matchesSearch = !search || name.includes(search.toLowerCase()) || id.includes(search.toLowerCase());

      if (!matchesSearch) return false;

      if (selectedCategory === "all") return true;
      if (selectedCategory === "apikey") return entry.displayAuthType === "apikey";
      if (selectedCategory === "oauth") return entry.displayAuthType === "oauth";
      if (selectedCategory === "webcookie") return entry.displayAuthType === "web-cookie" || entry.displayAuthType === "webcookie";
      if (selectedCategory === "compatible") return entry.displayAuthType === "compatible";
      return true;
    });
  }, [providerEntries, search, selectedCategory]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />

      {/* Modal Content */}
      <div className="relative w-full max-w-4xl bg-[#131926] rounded-2xl border border-white/10 shadow-2xl flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-white/10">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-lg bg-indigo-500/10 border border-white/10 flex items-center justify-center">
              <svg className="w-5 h-5 text-indigo-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
              </svg>
            </div>
            <div>
              <h2 className="text-base font-bold text-white">Chọn Nhà Cung Cấp AI</h2>
              <p className="text-[11px] text-slate-400">Chọn provider để thêm kết nối mới</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="w-8 h-8 rounded-lg bg-slate-800/80 hover:bg-slate-700 flex items-center justify-center text-slate-400 hover:text-white transition"
          >
            ✕
          </button>
        </div>

        {/* Search & Category filter */}
        <div className="px-6 py-3 border-b border-white/10 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="relative w-full sm:w-80">
            <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="8" />
              <path strokeLinecap="round" d="M21 21l-4.35-4.35" />
            </svg>
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Tìm kiếm (Gemini, OpenAI, Grok...)"
              className="w-full pl-10 pr-3 py-2 text-sm text-white placeholder-slate-500 bg-[#0b0f17] border border-white/10 rounded-lg focus:border-indigo-500/60 focus:ring-1 focus:ring-indigo-500/30 outline-none transition"
            />
          </div>

          <div className="flex items-center gap-1.5 overflow-x-auto pb-1 sm:pb-0">
            {categories.map((cat) => (
              <button
                key={cat.id}
                onClick={() => setSelectedCategory(cat.id)}
                className={`px-3 py-1.5 text-xs font-bold rounded-lg border transition shrink-0 ${
                  selectedCategory === cat.id
                    ? "bg-indigo-600 text-white border-white/10"
                    : "bg-slate-800/60 text-slate-400 border-white/10 hover:text-white hover:bg-slate-700"
                }`}
              >
                {cat.label}
              </button>
            ))}
          </div>
        </div>

        {/* Providers Grid */}
        <div className="flex-1 overflow-y-auto p-6">
          {filteredEntries.length === 0 ? (
            <div className="py-12 text-center text-slate-500 text-sm">
              Không tìm thấy nhà cung cấp nào phù hợp với &quot;{search}&quot;.
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
              {filteredEntries.map((entry) => (
                <div
                  key={`selector-${entry.providerId}`}
                  onClick={() => onSelectProvider(entry)}
                  className="group flex items-center gap-3 p-3.5 rounded-xl border border-white/10 bg-[#0b0f17] hover:border-indigo-500/50 hover:bg-[#161f2e] cursor-pointer transition-all shadow-sm hover:shadow-lg active:scale-[0.98]"
                >
                  <div className="w-10 h-10 rounded-lg bg-slate-800/80 flex items-center justify-center shrink-0 border border-white/10 group-hover:border-indigo-500/40 transition">
                    <ProviderIcon providerId={entry.providerId} size={24} />
                  </div>
                  <div className="min-w-0 flex-1">
                    <h4 className="text-sm font-bold text-white truncate">
                      {entry.provider?.name || entry.providerId}
                    </h4>
                    <div className="flex items-center gap-1.5 mt-0.5">
                      <span className="text-[10px] px-1.5 py-0.5 rounded font-bold border bg-slate-800/60 text-slate-400 border-white/10">
                        {entry.displayAuthType === "apikey"
                          ? "API Key"
                          : entry.displayAuthType === "oauth"
                            ? "OAuth"
                            : entry.displayAuthType === "web-cookie" || entry.displayAuthType === "webcookie"
                              ? "Web Session"
                              : entry.displayAuthType === "compatible"
                                ? "Tương thích"
                                : entry.displayAuthType}
                      </span>
                      {entry.provider?.hasFree && (
                        <span className="text-[10px] text-emerald-400 font-bold">Miễn phí</span>
                      )}
                    </div>
                  </div>
                  <div className="text-slate-600 group-hover:text-indigo-400 transition pr-1">
                    <svg className="w-4 h-4" viewBox="0 0 20 20" fill="currentColor">
                      <path fillRule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clipRule="evenodd" />
                    </svg>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-6 py-3 border-t border-white/10 text-xs text-slate-500">
          <span>Tìm thấy {filteredEntries.length} nhà cung cấp có sẵn</span>
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg border border-white/10 bg-slate-800/60 hover:bg-slate-700 text-slate-300 hover:text-white font-medium transition"
          >
            Đóng
          </button>
        </div>
      </div>
    </div>
  );
}
