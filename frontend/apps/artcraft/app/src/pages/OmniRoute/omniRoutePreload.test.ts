import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";

describe("OmniRoute internal core surface", () => {
  const host = fs.readFileSync(path.resolve(__dirname, "index.tsx"), "utf8");
  const providersPage = fs.readFileSync(
    path.resolve(__dirname, "src/app/(dashboard)/dashboard/providers/page.tsx"),
    "utf8"
  );

  it("renders the existing providers module directly in the ArtCraft tree", () => {
    expect(host).toContain('from "./src/app/(dashboard)/dashboard/providers/page"');
    expect(host).toContain('<ProvidersPage surface="artcraft-core" />');
    expect(host).not.toContain("iframe");
    expect(host).not.toContain("127.0.0.1");
    expect(host).not.toContain("fetch(");
  });

  it("keeps provider-key cards, route manager, and live topology in that surface", () => {
    expect(providersPage).toContain('surface === "artcraft-core"');
    expect(providersPage).toContain("HighlightableProviderCard");
    expect(providersPage).toContain("RouteManagerSection");
    expect(providersPage).toContain("HomeProviderTopologySection");
    expect(providersPage).toContain('data-testid="omniroute-artcraft-core-surface"');
  });

  it("does not route the OmniRoute tab through a replacement workshop UI", () => {
    expect(host).not.toContain("AiConnectionsWorkspace");
  });
});
