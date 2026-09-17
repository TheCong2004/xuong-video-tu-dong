"use client";

import { cn } from "@/shared/utils/cn";

interface TextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  error?: boolean;
}

/**
 * Textarea — token-driven multiline input mirroring the Input primitive (same border,
 * focus ring and control radius). Replaces ad-hoc raw `<textarea>` styling at call sites.
 */
export default function Textarea({ className, error = false, ...props }: TextareaProps) {
  return (
    <textarea
      className={cn(
        "w-full py-2.5 px-3.5 text-sm text-white font-medium",
        "bg-[#10141e] border border-white/10 rounded-xl",
        "placeholder:text-slate-400",
        "focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 focus:outline-none",
        "transition-all shadow-inner disabled:opacity-50 disabled:cursor-not-allowed",
        "text-[16px] sm:text-sm",
        error ? "border-white/10 focus:border-rose-500 focus:ring-rose-500/20" : "",
        className
      )}
      {...props}
    />
  );
}
