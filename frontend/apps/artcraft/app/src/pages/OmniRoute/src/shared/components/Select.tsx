"use client";

import { useId } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/shared/utils/cn";

interface SelectOption {
  value: string;
  label: string;
}

interface SelectProps extends Omit<React.SelectHTMLAttributes<HTMLSelectElement>, "size"> {
  label?: React.ReactNode;
  options?: SelectOption[];
  placeholder?: string;
  error?: React.ReactNode;
  hint?: React.ReactNode;
  selectClassName?: string;
}

export default function Select({
  label,
  options = [],
  value,
  onChange,
  placeholder,
  error,
  hint,
  disabled = false,
  required = false,
  className,
  selectClassName,
  id: externalId,
  children,
  ...props
}: SelectProps) {
  const t = useTranslations("common");
  const generatedId = useId();
  const selectId = externalId || generatedId;
  const errorId = error ? `${selectId}-error` : undefined;
  const hintId = hint && !error ? `${selectId}-hint` : undefined;
  const describedBy = [errorId, hintId].filter(Boolean).join(" ") || undefined;

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label && (
        <label htmlFor={selectId} className="text-sm font-semibold text-slate-200">
          {label}
          {required && (
            <span className="text-rose-500 ml-1" aria-hidden="true">
              *
            </span>
          )}
        </label>
      )}
      <div className="relative">
        <select
          id={selectId}
          value={value}
          onChange={onChange}
          disabled={disabled}
          required={required}
          aria-required={required || undefined}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy}
          className={cn(
            "w-full py-2.5 px-3.5 pe-10 text-sm text-white font-medium",
            "bg-[#10141e] border border-white/10 rounded-xl appearance-none",
            "focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 focus:outline-none",
            "transition-all disabled:opacity-50 disabled:cursor-not-allowed",
            "text-[16px] sm:text-sm",
            error ? "border-white/10 focus:border-rose-500 focus:ring-rose-500/20" : "",
            selectClassName
          )}
          {...props}
        >
          {!children && (placeholder ?? t("selectOption")) && (
            <option value="" disabled className="bg-[#10141e] text-slate-400">
              {placeholder ?? t("selectOption")}
            </option>
          )}
          {!children &&
            options.map((option) => (
              <option key={option.value} value={option.value} className="bg-[#10141e] text-white">
                {option.label}
              </option>
            ))}
          {children}
        </select>
        <div
          className="absolute inset-y-0 end-0 flex items-center pe-3 pointer-events-none text-slate-400"
          aria-hidden="true"
        >
          <span className="material-symbols-outlined text-[20px]">expand_more</span>
        </div>
      </div>
      {error && (
        <p id={errorId} className="text-xs text-rose-400 flex items-center gap-1" role="alert">
          <span className="material-symbols-outlined text-[14px]" aria-hidden="true">
            error
          </span>
          {error}
        </p>
      )}
      {hint && !error && (
        <p id={hintId} className="text-xs text-slate-400">
          {hint}
        </p>
      )}
    </div>
  );
}

