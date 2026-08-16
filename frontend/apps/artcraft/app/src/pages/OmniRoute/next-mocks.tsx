import React from "react";

// Mock next/navigation
export function useRouter() {
  return {
    push: (url: string) => console.log("Navigate to", url),
    replace: (url: string) => console.log("Replace with", url),
    prefetch: () => {},
    back: () => {},
  };
}

// Mock next/link
export default function Link({ children, href, ...props }: any) {
  return (
    <a href={href} {...props} onClick={(e) => {
      e.preventDefault();
      console.log("Link clicked to", href);
    }}>
      {children}
    </a>
  );
}

// Mock next/dynamic
export function dynamic(importFunc: any, options: any) {
  // Return a lazy component that we can render
  return React.lazy(importFunc);
}

// Mock next-intl
export function useTranslations(namespace: string) {
  return (key: string, values?: any) => {
    return `${namespace}.${key}`;
  };
}
