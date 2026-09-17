import React from "react";
import { BookOpen, Clock3, FileText, Play, Sparkles } from "lucide-react";

export type ScriptMarketItem = {
  id: string;
  title: string;
  summary: string;
  tags: string[];
  brief: string;
  sources: string[];
  sceneCount: number;
  duration: number;
  voiceStyle: string;
  useTrendResearch: boolean;
};

const sampleScripts: ScriptMarketItem[] = [
  {
    id: "sample-lantern-keeper-v1",
    title: "Người giữ đèn ở phố cổ",
    summary:
      "Phim dọc kể chuyện 60 giây về một người thắp đèn lồng mỗi tối; nhịp ấm áp, kết bằng thông điệp giữ gìn ký ức đô thị.",
    tags: ["Kể chuyện", "Cảm xúc", "Phố cổ"],
    brief:
      "Tạo phim dọc 60 giây, 6 cảnh liên tục về Người Giữ Đèn ở Phố Cổ. Nhân vật chính xuất hiện xuyên suốt: người phụ nữ khoảng 35 tuổi, áo dài xanh đậm, tóc búi thấp, luôn cầm đèn lồng giấy màu hổ phách. Cảnh 1 phố cổ lúc chạng vạng; cảnh 2 mở cửa tiệm; cảnh 3 thắp từng chiếc đèn; cảnh 4 du khách dừng lại; cảnh 5 cơn mưa nhẹ phản chiếu ánh đèn; cảnh 6 toàn cảnh con phố ấm áp. Giọng kể tiếng Việt tự nhiên, ngắt nghỉ rõ, kết bằng một câu ngắn truyền cảm hứng.",
    sources: [],
    sceneCount: 6,
    duration: 60,
    voiceStyle: "storytelling_vietnamese",
    useTrendResearch: true,
  },
  {
    id: "sample-mountain-train-v1",
    title: "Chuyến tàu đêm qua miền núi",
    summary:
      "Mẫu điện ảnh 90 giây với một nhân vật xuyên suốt, ánh sáng xanh đêm và nhịp đọc chậm, giàu khoảng lặng.",
    tags: ["Điện ảnh", "Du hành", "Cảnh quan"],
    brief:
      "Tạo phim dọc 90 giây, 8 cảnh liên tục về Chuyến Tàu Đêm Qua Miền Núi. Nhân vật chính xuất hiện xuyên suốt: chàng trai 28 tuổi, áo khoác nâu, ba lô vải cũ, mái tóc đen ngắn, vé tàu trong tay. Cảnh 1 ga tàu đêm; cảnh 2 lên toa tàu; cảnh 3 cửa sổ phản chiếu khuôn mặt; cảnh 4 tàu đi qua rừng sương; cảnh 5 trà nóng trên bàn; cảnh 6 bình minh ló sau núi; cảnh 7 xuống ga nhỏ; cảnh 8 nhân vật bước vào thung lũng. Giữ cảm xúc trầm, nhiều khoảng nghỉ, giọng thuyết minh điện ảnh tiếng Việt.",
    sources: [],
    sceneCount: 8,
    duration: 90,
    voiceStyle: "cinematic_narrator",
    useTrendResearch: true,
  },
];

interface Props {
  onUseScript: (script: ScriptMarketItem) => void;
}

export const ScriptMarketView: React.FC<Props> = ({ onUseScript }) => (
  <div className="mx-auto w-full max-w-[1320px] p-5 lg:p-7">
    <header className="rounded-2xl border border-white/10 bg-[#121622] p-5">
      <div className="flex items-start gap-3">
        <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-gradient-to-br from-amber-400 to-orange-500 text-white">
          <BookOpen className="h-6 w-6" />
        </div>
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.18em] text-amber-300">
            Kho khởi tạo
          </p>
          <h1 className="mt-1 text-xl font-bold text-white">Chợ kịch bản</h1>
          <p className="mt-1 max-w-3xl text-sm text-zinc-400">
            Chọn một mẫu để điền thẳng vào Xưởng Phim AI. Mẫu chỉ là dữ liệu
            thử cục bộ; job chỉ được tạo khi bạn thêm anchor nhân vật và bấm
            nút sản xuất trong Xưởng.
          </p>
        </div>
      </div>
    </header>

    <section className="mt-5 grid gap-5 lg:grid-cols-2">
      {sampleScripts.map((script) => (
        <article
          key={script.id}
          className="flex flex-col rounded-2xl border border-white/10 bg-[#121622] p-5"
        >
          <div className="flex items-start justify-between gap-4">
            <div>
              <h2 className="text-lg font-bold text-white">{script.title}</h2>
              <p className="mt-2 text-sm leading-6 text-zinc-400">
                {script.summary}
              </p>
            </div>
            <Sparkles className="h-5 w-5 shrink-0 text-amber-300" />
          </div>
          <div className="mt-4 flex flex-wrap gap-2">
            {script.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-full bg-amber-500/10 px-2.5 py-1 text-xs font-medium text-amber-200"
              >
                {tag}
              </span>
            ))}
          </div>
          <dl className="mt-5 grid grid-cols-2 gap-3 rounded-xl bg-black/20 p-3 text-sm">
            <div className="flex items-center gap-2 text-zinc-400">
              <FileText className="h-4 w-4 text-amber-300" />
              {script.sceneCount} cảnh
            </div>
            <div className="flex items-center gap-2 text-zinc-400">
              <Clock3 className="h-4 w-4 text-amber-300" />
              {script.duration} giây
            </div>
          </dl>
          <button
            type="button"
            onClick={() => onUseScript(script)}
            className="mt-5 inline-flex items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-amber-500 to-orange-500 px-4 py-3 font-semibold text-white transition hover:brightness-110"
          >
            <Play className="h-4 w-4 fill-current" />
            Dùng trong Xưởng Phim AI
          </button>
        </article>
      ))}
    </section>
  </div>
);
