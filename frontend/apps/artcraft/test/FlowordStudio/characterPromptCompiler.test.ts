import { webcrypto } from "node:crypto";
import {
  canonicalizePromptTokens,
  CharacterProfileV1,
  compileScenePromptTextV1,
  compileScenePromptV1,
  validateCharacterProfileV1,
} from "../../app/src/pages/FlowordStudio/src/services/characterPromptCompiler";

Object.defineProperty(globalThis, "crypto", { value: webcrypto, configurable: true });

const profile: CharacterProfileV1 = {
  id: "hero-01",
  name: "Hero",
  version: 2,
  anchorPath: "C:/artcraft/anchors/hero.png",
  anchorSha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  lockedVisualTraits: ["black hair", "small scar on left eyebrow", "gray wool coat"],
  styleAnchor: ["cinematic anime", "8k"],
  negativePrompt: ["different face", "extra fingers"],
  createdAt: "2026-09-13T00:00:00Z",
  updatedAt: "2026-09-13T00:00:00Z",
};

describe("Character prompt compiler v1", () => {
  test("is deterministic over repeated compilation", async () => {
    const scene = { sceneId: "scene-01", sceneContext: "Hero speaks at sunrise" };
    const baseline = await compileScenePromptV1(scene, profile);
    for (let index = 0; index < 100; index += 1) {
      await expect(compileScenePromptV1(scene, profile)).resolves.toEqual(baseline);
    }
  });

  test("canonicalizes token order without changing the compiled prompt", () => {
    const reordered = { ...profile, lockedVisualTraits: [...profile.lockedVisualTraits].reverse() };
    expect(compileScenePromptTextV1({ sceneId: "scene-01", sceneContext: "Hero speaks at sunrise" }, reordered)).toBe(
      compileScenePromptTextV1({ sceneId: "scene-01", sceneContext: "Hero speaks at sunrise" }, profile),
    );
    expect(canonicalizePromptTokens(["  beta", "alpha", "beta  "])).toEqual(["alpha", "beta"]);
  });

  test("changes the prompt and SHA when a locked trait changes", async () => {
    const scene = { sceneId: "scene-01", sceneContext: "Hero speaks at sunrise" };
    const baseline = await compileScenePromptV1(scene, profile);
    const changed = await compileScenePromptV1({ ...scene }, { ...profile, lockedVisualTraits: [...profile.lockedVisualTraits, "blue eyes"] });
    expect(changed.compiledPrompt).not.toBe(baseline.compiledPrompt);
    expect(changed.compiledPromptSha256).not.toBe(baseline.compiledPromptSha256);
  });

  test("refuses a profile that cannot prove its anchor identity", () => {
    expect(() => validateCharacterProfileV1({ ...profile, anchorSha256: "missing" })).toThrow("CHARACTER_ANCHOR_SHA256_INVALID");
    expect(() => compileScenePromptTextV1({ sceneId: "scene-01", sceneContext: "Hero speaks" }, { ...profile, lockedVisualTraits: [] })).toThrow("CHARACTER_LOCKED_TRAITS_REQUIRED");
  });
});
