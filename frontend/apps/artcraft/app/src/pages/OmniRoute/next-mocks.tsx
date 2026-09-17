import React from "react";
import viMessages from "./src/i18n/messages/vi.json";

// Mock next/navigation
export function useRouter() {
  return {
    push: (url: string) => console.log("Navigate to", url),
    replace: (url: string) => console.log("Replace with", url),
    prefetch: () => {},
    back: () => {},
  };
}

// The existing provider module only uses this for an optional `search` query.
// In ArtCraft it stays empty because navigation is tab based rather than URL
// based; this preserves the module without inventing a URL host.
export function useSearchParams() {
  return new URLSearchParams();
}

export function usePathname() {
  return "/dashboard/providers";
}

export function useParams() {
  return {};
}

export function redirect(url: string): never {
  throw new Error(`Navigation is not available in the embedded OmniRoute surface: ${url}`);
}

// Mock next/link
const Link = React.forwardRef<HTMLAnchorElement, any>(function Link(
  { children, href, onClick, ...props }: any,
  ref
) {
  return (
    <a
      ref={ref}
      href={href}
      {...props}
      onClick={(e) => {
        onClick?.(e);
        if (!e.defaultPrevented) {
          e.preventDefault();
          console.log("Link clicked to", href);
        }
      }}
    >
      {children}
    </a>
  );
});
export default Link;

// Mock next/dynamic
export function dynamic(importFunc: any, options: any) {
  // Return a lazy component that we can render
  return React.lazy(importFunc);
}

// Mock next-intl
export function useTranslations(namespace: string) {
  const dictionary = namespace.split(".").reduce<unknown>(
    (value, segment) =>
      value && typeof value === "object" ? (value as Record<string, unknown>)[segment] : undefined,
    viMessages
  );
  const translate = (key: string, values?: Record<string, unknown>) => {
    const translated =
      dictionary && typeof dictionary === "object"
        ? (dictionary as Record<string, unknown>)[key]
        : undefined;
    const fallback = typeof translated === "string" ? translated : key;
    return values
      ? Object.entries(values).reduce(
          (text, [name, value]) => text.replaceAll(`{${name}}`, String(value)),
          fallback
        )
      : fallback;
  };
  translate.has = (key: string) =>
    Boolean(dictionary && typeof dictionary === "object" && key in (dictionary as object));
  return translate;
}

// OmniRoute's shared controls read the active locale even though the compact
// ArtCraft surface exposes no language switcher.
export function useLocale() {
  return "vi";
}
