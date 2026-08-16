import React, { useState } from 'react';
import {
  Activity,
  Boxes,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Code2,
  Cpu,
  Database,
  ExternalLink,
  Folder,
  HardDrive,
  Layers,
  Network,
  RefreshCw,
  Server,
  Settings,
  Shield,
  Sliders,
  Terminal,
  Zap,
} from 'lucide-react';
import { DetailedReadinessStatus } from '../../api/flowordClient';

interface SettingsDevViewProps {
  readiness: DetailedReadinessStatus;
  onRefreshReadiness: () => void;
}

export const SettingsDevView: React.FC<SettingsDevViewProps> = ({
  readiness,
  onRefreshReadiness,
}) => {
  const [activeTab, setActiveTab] = useState<'general' | 'workers' | 'engines' | 'storage' | 'diagnostics'>('general');
  const [devSectionExpanded, setDevSectionExpanded] = useState<boolean>(false);

  return (
    <div className="max-w-7xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-white tracking-tight">Settings & System Architecture</h1>
          <p className="text-xs text-zinc-400">
            Configure system defaults, AI worker pools, engine adapters, and developer diagnostics.
          </p>
        </div>

        <button
          type="button"
          onClick={onRefreshReadiness}
          className="flex items-center gap-2 px-3.5 py-2 rounded-xl bg-white/[0.04] hover:bg-white/[0.08] text-zinc-300 text-xs font-medium border border-white/[0.08] transition"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Test All Services
        </button>
      </div>

      {/* Tabs Navigation */}
      <div className="flex items-center gap-2 border-b border-white/[0.08] pb-2 text-xs font-medium overflow-x-auto">
        <button
          type="button"
          onClick={() => setActiveTab('general')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'general'
              ? 'bg-white/[0.08] text-white font-semibold'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <Settings className="h-4 w-4" />
          General System
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('workers')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'workers'
              ? 'bg-white/[0.08] text-white font-semibold'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <Zap className="h-4 w-4 text-amber-400" />
          Worker Pools & Concurrency
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('engines')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'engines'
              ? 'bg-white/[0.08] text-white font-semibold'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <Boxes className="h-4 w-4 text-blue-400" />
          AI Engines & Adapters
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('storage')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'storage'
              ? 'bg-white/[0.08] text-white font-semibold'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <HardDrive className="h-4 w-4 text-emerald-400" />
          Storage & Directories
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('diagnostics')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'diagnostics'
              ? 'bg-rose-500/20 text-rose-300 font-semibold border border-rose-500/30'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <Code2 className="h-4 w-4 text-rose-400" />
          Developer & Diagnostics
        </button>
      </div>

      {/* Tab Contents */}
      {activeTab === 'general' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-5 text-xs">
          <h3 className="text-sm font-bold text-white">General Preferences</h3>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.05] space-y-2">
              <span className="font-semibold text-white">Default Workflow Strategy</span>
              <p className="text-zinc-400 text-[11px]">
                Define default processing order when new jobs are created from Quick Actions.
              </p>
              <select className="w-full px-3 py-2 rounded-xl bg-[#171b26] border border-white/[0.08] text-zinc-300 focus:outline-none">
                <option>Automated: Image &rarr; 9:16 &rarr; Video &rarr; Review</option>
                <option>Fast Track: Image &rarr; Video &rarr; Direct Publish</option>
              </select>
            </div>

            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.05] space-y-2">
              <span className="font-semibold text-white">Concurrency Limit</span>
              <p className="text-zinc-400 text-[11px]">
                Maximum parallel video generation jobs allowed simultaneously across workers.
              </p>
              <select className="w-full px-3 py-2 rounded-xl bg-[#171b26] border border-white/[0.08] text-zinc-300 focus:outline-none">
                <option>4 Concurrent Jobs (Recommended for 16GB+ RAM)</option>
                <option>8 Concurrent Jobs (High performance workstation)</option>
                <option>2 Concurrent Jobs (Lightweight mode)</option>
              </select>
            </div>
          </div>
        </div>
      )}

      {activeTab === 'workers' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-5 text-xs">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-bold text-white">Active Worker Pools</h3>
              <p className="text-zinc-400 text-[11px]">Manage automated browser profiles & generation executors.</p>
            </div>
            <span className="px-3 py-1 rounded-full text-xs font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
              8/10 Workers Online
            </span>
          </div>

          <div className="space-y-3">
            {[1, 2, 3, 4, 5, 6, 7, 8].map((w) => (
              <div key={w} className="flex items-center justify-between p-3.5 rounded-xl bg-white/[0.02] border border-white/[0.05]">
                <div className="flex items-center gap-3">
                  <span className="h-2.5 w-2.5 rounded-full bg-emerald-400" />
                  <div>
                    <div className="font-semibold text-white">Grok Worker #{w < 10 ? `0${w}` : w}</div>
                    <div className="text-[10px] text-zinc-500 font-mono">PROFILE_WORKER_{w} • Ready</div>
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-[11px] text-zinc-400">Idle / Ready</span>
                  <span className="px-2 py-0.5 rounded text-[10px] bg-white/[0.06] text-zinc-300 font-mono">0 tasks</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {activeTab === 'engines' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-5 text-xs">
          <h3 className="text-sm font-bold text-white">Underlying Engine Adapters</h3>
          <p className="text-zinc-400 text-[11px]">
            All specialized engines are encapsulated into the Floword Unified Facade.
          </p>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-white">OmniRoute AI Gateway</span>
                <span className="text-emerald-400 text-[10px] font-bold">CONNECTED</span>
              </div>
              <p className="text-zinc-400 text-[11px]">
                Powers model routing, rate limit balancing, and LLM script prompting.
              </p>
            </div>

            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-white">MediaCrawler & Youwee</span>
                <span className="text-emerald-400 text-[10px] font-bold">READY</span>
              </div>
              <p className="text-zinc-400 text-[11px]">
                Automated multi-platform trend analysis, hashtag scraping & asset indexing.
              </p>
            </div>

            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-white">OpenMontage & Vynaro</span>
                <span className="text-emerald-400 text-[10px] font-bold">READY</span>
              </div>
              <p className="text-zinc-400 text-[11px]">
                Timeline rendering, 9:16 layout composition, subtitle styling & video fusion.
              </p>
            </div>

            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-white">CapCut Mate Bridge</span>
                <span className="text-emerald-400 text-[10px] font-bold">READY</span>
              </div>
              <p className="text-zinc-400 text-[11px]">
                Automated draft injection, sticker track overlay, and seamless export.
              </p>
            </div>
          </div>
        </div>
      )}

      {activeTab === 'storage' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-5 text-xs">
          <h3 className="text-sm font-bold text-white">Storage Locations & Output Directories</h3>

          <div className="space-y-4">
            <div>
              <label className="block text-zinc-400 mb-1 font-medium">Global Media Workspace Root</label>
              <div className="flex items-center gap-2">
                <input
                  type="text"
                  readOnly
                  value="D:\capcutpolot\artcraft\artifacts\"
                  className="flex-1 px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.08] font-mono text-zinc-300"
                />
                <button className="px-4 py-2 rounded-xl bg-white/[0.06] hover:bg-white/[0.1] text-white font-medium">
                  Browse...
                </button>
              </div>
            </div>

            <div>
              <label className="block text-zinc-400 mb-1 font-medium">CapCut Drafts Destination Directory</label>
              <div className="flex items-center gap-2">
                <input
                  type="text"
                  readOnly
                  value="C:\Users\AppData\Local\CapCut\User Data\Projects\com.lveditor.draft\"
                  className="flex-1 px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.08] font-mono text-zinc-300"
                />
                <button className="px-4 py-2 rounded-xl bg-white/[0.06] hover:bg-white/[0.1] text-white font-medium">
                  Browse...
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {activeTab === 'diagnostics' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-5 text-xs">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-bold text-white flex items-center gap-2">
                <Terminal className="h-4 w-4 text-rose-400" />
                Developer & Diagnostic Console
              </h3>
              <p className="text-zinc-400 text-[11px]">
                Low-level ports, process status, and real-time backend communication logs.
              </p>
            </div>
            <span className="px-3 py-1 rounded-full text-[11px] font-mono bg-blue-500/10 text-blue-400 border border-blue-500/20">
              Unified Server Port :20128
            </span>
          </div>

          {/* Service Health Table */}
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="border-b border-white/[0.06] text-zinc-500 font-medium">
                  <th className="pb-2">Subsystem Service</th>
                  <th className="pb-2">Port / Transport</th>
                  <th className="pb-2">Health</th>
                  <th className="pb-2 text-right">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/[0.04]">
                <tr>
                  <td className="py-2.5 font-medium text-white">Unified Server Core</td>
                  <td className="py-2.5 font-mono text-zinc-400">127.0.0.1:20128</td>
                  <td className="py-2.5 text-emerald-400 font-semibold">● Responsive</td>
                  <td className="py-2.5 text-right">
                    <button className="text-zinc-400 hover:text-white">Restart</button>
                  </td>
                </tr>
                <tr>
                  <td className="py-2.5 font-medium text-white">OmniRoute AI Gateway</td>
                  <td className="py-2.5 font-mono text-zinc-400">Internal Proxy</td>
                  <td className="py-2.5 text-emerald-400 font-semibold">● Responsive</td>
                  <td className="py-2.5 text-right">
                    <button className="text-zinc-400 hover:text-white">Ping</button>
                  </td>
                </tr>
                <tr>
                  <td className="py-2.5 font-medium text-white">CapCut Mate IPC</td>
                  <td className="py-2.5 font-mono text-zinc-400">Named Pipe / RPC</td>
                  <td className="py-2.5 text-emerald-400 font-semibold">● Ready</td>
                  <td className="py-2.5 text-right">
                    <button className="text-zinc-400 hover:text-white">Verify</button>
                  </td>
                </tr>
                <tr>
                  <td className="py-2.5 font-medium text-white">Browser Extension Bridge</td>
                  <td className="py-2.5 font-mono text-zinc-400">WebSocket / CDP</td>
                  <td className="py-2.5 text-emerald-400 font-semibold">● 10 Profiles Active</td>
                  <td className="py-2.5 text-right">
                    <button className="text-zinc-400 hover:text-white">Re-attach</button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          {/* Raw Diagnostic Log Terminal */}
          <div className="rounded-xl bg-[#0a0d13] border border-white/[0.06] p-4 font-mono text-[11px] space-y-1 text-zinc-400 h-44 overflow-y-auto">
            <div className="text-zinc-600">[SYS] Floword Studio unified engine initialized.</div>
            <div className="text-zinc-600">[SYS] Local adapter discovery: 8 modules loaded successfully.</div>
            <div className="text-emerald-400">[IPC] Backend connection verified at http://127.0.0.1:20128</div>
            <div className="text-blue-400">[BRIDGE] Browser extension pool listening for generation tasks.</div>
          </div>
        </div>
      )}
    </div>
  );
};
