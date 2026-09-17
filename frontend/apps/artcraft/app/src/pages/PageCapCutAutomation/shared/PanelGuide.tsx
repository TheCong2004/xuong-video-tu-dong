import { twMerge } from "tailwind-merge";

interface PanelGuideProps {
  /** Một dòng: mục này để làm gì */
  what: string;
  /** Cách dùng ngắn (①②③…) */
  how: string;
  /** Cảnh báo / điều kiện (vd: cần draft mate / local) */
  need?: string;
  /** warn = vàng khi thiếu path/draft */
  tone?: "default" | "warn";
  className?: string;
}

/** Hướng dẫn nhỏ thống nhất trên mọi panel CapCut Automation. */
export function PanelGuide({
  what,
  how,
  need,
  tone = "default",
  className,
}: PanelGuideProps) {
  return (
    <div
      className={twMerge(
        "shrink-0 border-b px-4 py-2.5 text-xs leading-relaxed",
        tone === "warn"
          ? "border-amber-500/20 bg-amber-500/10 text-amber-200"
          : "border-white/10 bg-[#131926]/60 text-zinc-400",
        className,
      )}
    >
      <p>
        <span className="font-semibold text-zinc-200">Mục này: </span>
        {what}
      </p>
      <p className="mt-0.5">
        <span className="font-semibold text-zinc-200">Cách dùng: </span>
        {how}
      </p>
      {need ? (
        <p
          className={twMerge(
            "mt-0.5",
            tone === "warn" ? "text-amber-300 font-medium" : "text-zinc-400",
          )}
        >
          <span className="font-semibold text-zinc-300">Cần: </span>
          {need}
        </p>
      ) : null}
    </div>
  );
}
