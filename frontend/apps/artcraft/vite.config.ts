import { defineConfig, type Plugin } from "vite";
import tsconfigPaths from "vite-tsconfig-paths";
import path from "path";
import fs from "fs";
import { resolve } from "node:path";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

const SPARK_MODULE_SUBPATH = "/@sparkjsdev/spark/dist/spark.module.js";
const SPARK_WASM_PATTERN =
  /module_or_path = new URL\("(data:application\/wasm;base64,[^"]+)", import\.meta\.url\);/;

const sparkWasmDataUrlFix = (): Plugin => ({
  name: "spark-wasm-data-url-fix",
  enforce: "pre",
  apply: "serve",
  transform(code, id) {
    if (!id.includes(SPARK_MODULE_SUBPATH)) {
      return null;
    }

    const match = code.match(SPARK_WASM_PATTERN);
    if (!match) {
      return null;
    }

    const [, dataUrl] = match;
    const patched = code.replace(
      SPARK_WASM_PATTERN,
      `module_or_path = "${dataUrl}";`,
    );

    return {
      code: patched,
      map: null,
    };
  },
});

const projectRoot = __dirname;
const appRoot = path.resolve(projectRoot, "app");
const workspaceRoot = path.resolve(projectRoot, "..", "..");
const repoRoot = path.resolve(workspaceRoot, "..");

// vite-tsconfig-paths only rewrites imports for files under Vite's `root`
// (apps/artcraft/app). Files under libs/ live outside root, so their
// `@storyteller/*` / `@frontend/*` imports are never rewritten. Generate real
// resolve aliases from tsconfig.base.json so they resolve from any location.
function workspacePathAliases(): Record<string, string> {
  const baseTsconfig = path.resolve(workspaceRoot, "tsconfig.base.json");
  const raw = fs.readFileSync(baseTsconfig, "utf8");
  const stripped = raw
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1")
    .replace(/,(\s*[}\]])/g, "$1");
  const parsed = JSON.parse(stripped) as {
    compilerOptions?: { paths?: Record<string, string[]> };
  };
  const paths = parsed.compilerOptions?.paths ?? {};
  const aliases: Record<string, string> = {};
  for (const [key, targets] of Object.entries(paths)) {
    if (!key.startsWith("@storyteller/") && !key.startsWith("@frontend/")) {
      continue;
    }
    if (key.includes("*") || !targets?.[0]) {
      continue;
    }
    aliases[key] = path.resolve(workspaceRoot, targets[0]);
  }
  return aliases;
}

function tryResolveCandidate(baseDir: string, subPath: string): string | null {
  const target = path.resolve(baseDir, subPath);
  const candidates = [
    target,
    target + ".ts",
    target + ".tsx",
    target + ".js",
    target + ".jsx",
    path.join(target, "index.ts"),
    path.join(target, "index.tsx"),
    path.join(target, "index.js"),
    path.join(target, "index.jsx"),
  ];
  for (const candidate of candidates) {
    try {
      if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
        return candidate;
      }
    } catch {
      // ignore
    }
  }
  return null;
}

// Custom resolver plugin to handle multi-target `@/` alias for PageYouwee & app/src (Cross-Platform)
const multiTargetAliasResolver = (): Plugin => ({
  name: "multi-target-alias-resolver",
  enforce: "pre",
  resolveId(source, importer) {
    if (source.startsWith("@/") && importer) {
      const subPath = source.slice(2);
      const normImporter = importer.replace(/\\/g, "/");

      // 1. If imported within PageYouwee, try resolving inside PageYouwee first
      if (normImporter.includes("PageYouwee")) {
        const youweeResolved = tryResolveCandidate(path.resolve(projectRoot, "app/src/pages/PageYouwee"), subPath);
        if (youweeResolved) return youweeResolved;
      }

      // 1.5. If imported within OmniRoute, try resolving inside OmniRoute/src
      if (normImporter.includes("OmniRoute")) {
        const omnirouteResolved = tryResolveCandidate(path.resolve(projectRoot, "app/src/pages/OmniRoute/src"), subPath);
        if (omnirouteResolved) return omnirouteResolved;
      }

      // 2. Default fallback to app/src
      const defaultResolved = tryResolveCandidate(path.resolve(projectRoot, "app/src"), subPath);
      if (defaultResolved) return defaultResolved;
    }
    return null;
  },
});

export default defineConfig({
  root: appRoot,
  optimizeDeps: {
    exclude: ["@sparkjsdev/spark"],
  },
  build: {
    outDir: path.resolve(projectRoot, "dist"),
    rollupOptions: {
      input: {
        index: resolve(projectRoot, "app/index.html"),
      },
    },
  },
  plugins: [multiTargetAliasResolver(), sparkWasmDataUrlFix(), tsconfigPaths(), wasm(), topLevelAwait()],
  server: {
    // Keep the Tauri dev URL deterministic. Vite otherwise changes ports when
    // the default is occupied, while Tauri continues loading its configured
    // URL and renders a connection-refused page.
    host: "127.0.0.1",
    port: 5174,
    strictPort: true,
    proxy: {
      // OmniRoute page routes. `/home` matters: OmniRoute's `/dashboard`
      // server-redirects to `/home`, and without a proxy entry that landing
      // page fell through to ArtCraft's SPA index.html (the iframe showed
      // ArtCraft's own sign-up screen instead of the router dashboard).
      // ArtCraft itself is tab-driven and claims no URL path, so this is safe.
      // `login`/`forgot-password` are intentionally absent — OmniRoute's login
      // was removed now that it runs embedded on loopback.
      "^/(omniroute|home|dashboard|auth|callback|connect|docs|terms|privacy)": {
        target: "http://127.0.0.1:20128",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/omniroute/, ""),
      },
      "/_next": {
        target: "http://127.0.0.1:20128",
        changeOrigin: true,
      },
      // Next serves its self-hosted fonts from this root namespace. Unproxied,
      // the request fell through to index.html and the browser reported
      // "Failed to decode downloaded font / invalid sfntVersion".
      "/__nextjs_font": {
        target: "http://127.0.0.1:20128",
        changeOrigin: true,
      },
      // OmniRoute's root-level icons/manifest. ArtCraft ships favicon.ico
      // (left alone); these three names belong only to OmniRoute.
      "^/(favicon\\.svg|icon-512\\.png|manifest\\.webmanifest)$": {
        target: "http://127.0.0.1:20128",
        changeOrigin: true,
      },
      "/api": {
        target: "http://127.0.0.1:20128",
        changeOrigin: true,
      }
    },
    watch: {
      ignored: ["**/pages/freellmapi/server/**"],
    },
    fs: {
      allow: [workspaceRoot, repoRoot],
    },
  },
  resolve: {
    alias: {
      ...workspacePathAliases(),
      "~": path.resolve(projectRoot, "app/src"),
      "@mediacrawler": path.resolve(
        projectRoot,
        "app/src/pages/PageMediaCrawler/src",
      ),
      "@freellmapi": path.resolve(
        projectRoot,
        "app/src/pages/freellmapi/client/src",
      ),
      "@vynaro": path.resolve(repoRoot, "vynaro/src"),
      "@vynaro-components": path.resolve(repoRoot, "vynaro/src/components"),
      "@vynaro-hooks": path.resolve(repoRoot, "vynaro/src/hooks"),
      "@vynaro-stores": path.resolve(repoRoot, "vynaro/src/stores"),
      "@vynaro-ipc": path.resolve(repoRoot, "vynaro/src/ipc"),
      "@vynaro-lib": path.resolve(repoRoot, "vynaro/src/lib"),
      "@vynaro-styles": path.resolve(repoRoot, "vynaro/src/styles"),
      "@components": path.resolve(repoRoot, "vynaro/src/components"),
      "@hooks": path.resolve(repoRoot, "vynaro/src/hooks"),
      "@stores": path.resolve(repoRoot, "vynaro/src/stores"),
      "@ipc": path.resolve(repoRoot, "vynaro/src/ipc"),
      "@lib": path.resolve(repoRoot, "vynaro/src/lib"),
      "@styles": path.resolve(repoRoot, "vynaro/src/styles"),
      "next/navigation": path.resolve(projectRoot, "app/src/pages/OmniRoute/next-mocks.tsx"),
      "next/link": path.resolve(projectRoot, "app/src/pages/OmniRoute/next-mocks.tsx"),
      "next/dynamic": path.resolve(projectRoot, "app/src/pages/OmniRoute/next-mocks.tsx"),
      "next-intl": path.resolve(projectRoot, "app/src/pages/OmniRoute/next-mocks.tsx"),
      "next/font/google": path.resolve(projectRoot, "app/src/pages/OmniRoute/next-mocks.tsx"),
    },
    dedupe: [
      "react",
      "react-dom",
      "@preact/signals-core",
      "@preact/signals-react",
      "lucide-react"
    ],
  },
});
