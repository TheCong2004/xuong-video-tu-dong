import ProvidersPage from "./src/app/(dashboard)/dashboard/providers/page";

// This is the existing OmniRoute provider module rendered directly inside the
// ArtCraft React tree. It has no iframe, WebView, redirect, or localhost UI
// dependency; its requests are backend transport only.
//
// The components in the ArtCraft surface branch use hardcoded dark-theme
// Tailwind classes (matching FlowordStudio style: bg-[#131926], text-white,
// text-slate-400, border-white/10, etc.) instead of OmniRoute's CSS-variable
// design tokens, so they render correctly without OmniRoute's globals.css.
export function PageOmniRoute() {
  return (
    <main className="floword-shell h-[calc(100vh-56px)] overflow-y-auto bg-[#0b0e14] p-6 text-white selection:bg-indigo-500/30 selection:text-white">
      <div className="max-w-7xl mx-auto">
        <ProvidersPage surface="artcraft-core" />
      </div>
    </main>
  );
}

