import React, { useState, useEffect } from 'react';
import {
  validateBulkImport,
  commitBulkImport,
  BulkValidationSummary,
  BulkCommitResponse,
  listContentPages,
  ContentPage,
} from '../../api/flowordClient';
import {
  Upload, FileSpreadsheet, CheckCircle2, AlertTriangle, Play,
  RefreshCw, FileText, Check, Layers, ArrowRight, ShieldAlert, Sparkles
} from 'lucide-react';

const CSV_SAMPLE = `page_id,source_image,image_prompt,expand_916_prompt,video_prompt,title,caption,hashtags,platforms,post_mode
page_floword_demo,,Cinematic portrait of a cyberpunk hacker in neon rain,,Hyper realistic camera zoom into neon reflection,Cyberpunk 2099,Amazing futuristic world #cyberpunk #neon,cyberpunk neon ai,facebook|tiktok,auto
`;

export const BulkImportView: React.FC = () => {
  const [csvContent, setCsvContent] = useState<string>(CSV_SAMPLE);
  const [pages, setPages] = useState<ContentPage[]>([]);
  const [validating, setValidating] = useState<boolean>(false);
  const [validationResult, setValidationResult] = useState<BulkValidationSummary | null>(null);
  const [committing, setCommitting] = useState<boolean>(false);
  const [commitResult, setCommitResult] = useState<BulkCommitResponse | null>(null);

  useEffect(() => {
    listContentPages(false).then((p) => {
      setPages(p.pages ?? []);
      // If we have real pages, prefill the first page ID into the sample
      if (p.pages && p.pages.length > 0) {
        const firstId = p.pages[0].id;
        setCsvContent(
          `page_id,source_image,image_prompt,expand_916_prompt,video_prompt,title,caption,hashtags,platforms,post_mode\n${firstId},,Cinematic portrait of a cyberpunk warrior in neon rain,,Hyper realistic camera zoom into neon reflection,Cyberpunk 2099,Amazing futuristic world #cyberpunk #neon,cyberpunk neon ai,facebook|tiktok,auto\n`
        );
      }
    }).catch(console.error);
  }, []);

  const [bannerMessage, setBannerMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const showBanner = (type: 'success' | 'error', text: string) => {
    setBannerMessage({ type, text });
    setTimeout(() => setBannerMessage(null), 4000);
  };

  const handleValidate = async () => {
    setValidating(true);
    setCommitResult(null);
    try {
      const summary = await validateBulkImport({ csv_content: csvContent });
      setValidationResult(summary);
      if (summary.errors.length === 0) {
        showBanner('success', `Đã kiểm tra xong: Toàn bộ ${summary.valid_count} dòng hợp lệ!`);
      } else {
        showBanner('error', `Phát hiện ${summary.errors.length} lỗi trong file CSV.`);
      }
    } catch (err) {
      showBanner('error', `Validation error: ${err}`);
    } finally {
      setValidating(false);
    }
  };

  const handleCommit = async () => {
    if (!validationResult || validationResult.valid_rows.length === 0) return;
    setCommitting(true);
    try {
      const res = await commitBulkImport(validationResult.valid_rows);
      setCommitResult(res);
      setValidationResult(null);
      showBanner('success', `Đã tạo thành công ${res.total_created} jobs vào SQLite Database!`);
    } catch (err) {
      showBanner('error', `Commit error: ${err}`);
    } finally {
      setCommitting(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0b0f17] text-slate-100 overflow-y-auto p-6 space-y-6">
      {/* Banner */}
      {bannerMessage && (
        <div
          className={`p-3 rounded-lg border text-xs font-semibold flex items-center justify-between transition-all ${
            bannerMessage.type === 'success'
              ? 'bg-emerald-950/80 border-emerald-500/50 text-emerald-300'
              : 'bg-rose-950/80 border-rose-500/50 text-rose-300'
          }`}
        >
          <span>{bannerMessage.text}</span>
          <button onClick={() => setBannerMessage(null)} className="text-slate-400 hover:text-white">✕</button>
        </div>
      )}

      {/* Header */}
      <div className="bg-[#131926] p-5 rounded-xl border border-slate-800/80 shadow-lg flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 border border-indigo-500/30 flex items-center justify-center text-indigo-400">
            <FileSpreadsheet className="w-5 h-5" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-white tracking-wide">Bulk Job Import & Validation</h1>
            <p className="text-xs text-slate-400">Import hàng chục / hàng trăm video jobs qua CSV với pre-flight validation nghiêm ngặt</p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={handleValidate}
            disabled={validating || !csvContent.trim()}
            className="flex items-center gap-2 px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-bold shadow transition active:scale-95 disabled:opacity-50"
          >
            <ShieldAlert className={`w-4 h-4 ${validating ? 'animate-spin' : ''}`} />
            <span>{validating ? 'Đang kiểm tra...' : 'Dry-Run Kiểm Tra Batch'}</span>
          </button>
        </div>
      </div>

      {/* Pages quick reference pill list */}
      <div className="bg-[#131926] p-3.5 rounded-xl border border-slate-800 flex items-center gap-2 overflow-x-auto text-xs">
        <span className="text-slate-400 font-semibold flex-shrink-0 flex items-center gap-1.5">
          <Layers className="w-3.5 h-3.5 text-indigo-400" />
          <span>Page IDs khả dụng:</span>
        </span>
        {pages.length === 0 ? (
          <span className="text-slate-500 italic">Chưa có Page nào trong DB.</span>
        ) : (
          pages.map((p) => (
            <span
              key={p.id}
              onClick={() => {
                navigator.clipboard.writeText(p.id);
                showBanner('success', `Đã copy Page ID: ${p.id}`);
              }}
              title="Click để copy ID"
              className="px-2.5 py-1 rounded bg-[#0b0f17] border border-slate-700 hover:border-indigo-500 text-slate-300 font-mono text-[11px] cursor-pointer transition flex items-center gap-1"
            >
              <span>{p.name}</span>
              <span className="text-indigo-400">({p.id})</span>
            </span>
          ))
        )}
      </div>

      {/* CSV Input Editor */}
      <div className="bg-[#131926] p-5 rounded-xl border border-slate-800 shadow space-y-3">
        <div className="flex items-center justify-between">
          <div className="text-xs font-bold text-white flex items-center gap-2">
            <FileText className="w-4 h-4 text-indigo-400" />
            <span>Nội dung CSV / Dữ liệu Nhập</span>
          </div>
          <span className="text-[11px] text-slate-500">Hỗ trợ dấu phân tách: Phẩy (,), Chấm phẩy (;), Tab</span>
        </div>

        <textarea
          rows={10}
          value={csvContent}
          onChange={(e) => setCsvContent(e.target.value)}
          placeholder="Dán nội dung CSV vào đây..."
          className="w-full bg-[#0b0f17] text-xs font-mono text-slate-200 p-4 rounded-lg border border-slate-700 focus:outline-none focus:border-indigo-500 leading-relaxed"
        />
      </div>

      {/* Validation Result Box */}
      {validationResult && (
        <div className="bg-[#131926] p-5 rounded-xl border border-slate-800 shadow space-y-4 animate-in fade-in duration-200">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-slate-800 pb-4">
            <div>
              <h2 className="text-sm font-bold text-white flex items-center gap-2">
                <CheckCircle2 className="w-4 h-4 text-indigo-400" />
                <span>Kết quả Pre-flight Validation</span>
              </h2>
              <p className="text-xs text-slate-400 mt-0.5">
                Tổng: <strong className="text-white">{validationResult.total_rows}</strong> dòng • Hợp lệ: <strong className="text-emerald-400">{validationResult.valid_count}</strong> • Lỗi: <strong className="text-rose-400">{validationResult.invalid_count}</strong>
              </p>
            </div>

            {validationResult.valid_count > 0 && (
              <button
                onClick={handleCommit}
                disabled={committing}
                className="flex items-center gap-2 px-5 py-2.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold shadow-lg transition active:scale-95 disabled:opacity-50"
              >
                <Play className={`w-4 h-4 ${committing ? 'animate-spin' : ''}`} />
                <span>{committing ? 'Đang ghi vào SQLite...' : `Khởi tạo ${validationResult.valid_count} Jobs Thực tế`}</span>
              </button>
            )}
          </div>

          {/* Error taxonomy list */}
          {validationResult.errors.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-xs font-bold text-rose-400 uppercase tracking-wider flex items-center gap-1.5">
                <AlertTriangle className="w-3.5 h-3.5" />
                <span>Danh sách Dòng Bị Lỗi ({validationResult.errors.length})</span>
              </h3>
              <div className="space-y-1.5 max-h-48 overflow-y-auto pr-2">
                {validationResult.errors.map((err, idx) => (
                  <div key={idx} className="bg-rose-950/30 border border-rose-500/30 p-2.5 rounded-lg text-xs flex items-start gap-3">
                    <span className="font-mono font-bold text-rose-400 px-1.5 py-0.5 rounded bg-rose-500/20 text-[10px]">
                      DÒNG {err.row_index}
                    </span>
                    <div className="flex-1">
                      <span className="font-semibold text-rose-200">[{err.field}]</span>{' '}
                      <span className="text-slate-300">{err.message}</span>{' '}
                      <span className="text-[10px] text-rose-400/80 font-mono">({err.code})</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Valid rows preview table */}
          {validationResult.valid_rows.length > 0 && (
            <div className="space-y-2">
              <h3 className="text-xs font-bold text-emerald-400 uppercase tracking-wider flex items-center gap-1.5">
                <Check className="w-3.5 h-3.5" />
                <span>Xem trước Dòng Hợp lệ Sẽ Tạo ({validationResult.valid_rows.length})</span>
              </h3>
              <div className="overflow-x-auto border border-slate-800 rounded-lg max-h-56">
                <table className="w-full text-left text-xs">
                  <thead className="bg-[#0b0f17] text-slate-400 sticky top-0">
                    <tr>
                      <th className="p-2.5">Row</th>
                      <th className="p-2.5">Page ID</th>
                      <th className="p-2.5">Image / Video Prompt</th>
                      <th className="p-2.5">Title / Caption</th>
                      <th className="p-2.5">Platforms</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800">
                    {validationResult.valid_rows.map((r) => (
                      <tr key={r.row_index} className="hover:bg-[#0b0f17]/50">
                        <td className="p-2.5 font-mono text-slate-400">{r.row_index}</td>
                        <td className="p-2.5 font-mono text-indigo-400">{r.page_id}</td>
                        <td className="p-2.5 truncate max-w-[200px] text-slate-300">{r.video_prompt || r.image_prompt}</td>
                        <td className="p-2.5 truncate max-w-[150px] text-slate-400">{r.title || r.caption}</td>
                        <td className="p-2.5 font-mono text-[10px] text-slate-400">{r.platforms.join(', ') || 'All'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Success Commit Banner */}
      {commitResult && (
        <div className="bg-emerald-950/40 border border-emerald-500/40 p-5 rounded-xl text-xs space-y-3 animate-in zoom-in-95 duration-200">
          <div className="flex items-center gap-2 text-emerald-400 font-bold text-sm">
            <CheckCircle2 className="w-5 h-5" />
            <span>Khởi tạo Thành công {commitResult.total_created} Pipeline Jobs Thực tế!</span>
          </div>
          <p className="text-slate-300">
            Batch ID: <strong className="font-mono text-white">{commitResult.batch_id}</strong>. Các Jobs đã được đưa vào hàng đợi SQLite và Scheduler sẽ tự động phân bổ cho Worker xử lý theo Concurrency giới hạn.
          </p>
          <div className="text-[11px] font-mono text-slate-400 max-h-24 overflow-y-auto bg-black/40 p-2 rounded">
            Created Job IDs: {commitResult.created_job_ids.join(', ')}
          </div>
        </div>
      )}
    </div>
  );
};
