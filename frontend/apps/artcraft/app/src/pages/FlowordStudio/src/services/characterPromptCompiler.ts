export interface CharacterProfileV1 {
  id: string;
  name: string;
  version: number;
  anchorPath: string;
  anchorSha256: string;
  lockedVisualTraits: string[];
  styleAnchor: string[];
  negativePrompt: string[];
  createdAt: string;
  updatedAt: string;
}

export interface ScenePromptInputV1 {
  sceneId: string;
  sceneContext: string;
}

export interface CompiledScenePromptV1 {
  characterId: string;
  characterVersion: number;
  sceneId: string;
  compiledPrompt: string;
  compiledPromptSha256: string;
}

const SHA256 = /^[a-fA-F0-9]{64}$/;

export class CharacterPromptCompilerError extends Error {
  constructor(public readonly code: string) {
    super(code);
    this.name = "CharacterPromptCompilerError";
  }
}

function normalizeText(value: string): string {
  return value.normalize("NFC").trim().replace(/\s+/g, " ");
}

/**
 * Unicode-normalized, deduplicated tokens sorted by code point.  Do not use
 * localeCompare here: locale configuration would make one saved profile build
 * a different prompt on another user's machine.
 */
export function canonicalizePromptTokens(values: string[]): string[] {
  const tokens = new Set<string>();
  for (const value of values) {
    const token = normalizeText(value);
    if (token) tokens.add(token);
  }
  return [...tokens].sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

export function validateCharacterProfileV1(profile: CharacterProfileV1): void {
  if (!normalizeText(profile.id)) throw new CharacterPromptCompilerError("CHARACTER_ID_REQUIRED");
  if (!normalizeText(profile.name)) throw new CharacterPromptCompilerError("CHARACTER_NAME_REQUIRED");
  if (!Number.isInteger(profile.version) || profile.version < 1) {
    throw new CharacterPromptCompilerError("CHARACTER_VERSION_INVALID");
  }
  if (!normalizeText(profile.anchorPath)) throw new CharacterPromptCompilerError("CHARACTER_ANCHOR_PATH_REQUIRED");
  if (!SHA256.test(profile.anchorSha256)) throw new CharacterPromptCompilerError("CHARACTER_ANCHOR_SHA256_INVALID");
  if (canonicalizePromptTokens(profile.lockedVisualTraits).length === 0) {
    throw new CharacterPromptCompilerError("CHARACTER_LOCKED_TRAITS_REQUIRED");
  }
}

export function compileScenePromptTextV1(scene: ScenePromptInputV1, profile: CharacterProfileV1): string {
  validateCharacterProfileV1(profile);
  const sceneContext = normalizeText(scene.sceneContext);
  if (!normalizeText(scene.sceneId)) throw new CharacterPromptCompilerError("SCENE_ID_REQUIRED");
  if (!sceneContext) throw new CharacterPromptCompilerError("SCENE_CONTEXT_REQUIRED");

  const lockedTraits = canonicalizePromptTokens(profile.lockedVisualTraits);
  const styleAnchor = canonicalizePromptTokens(profile.styleAnchor);
  const negativePrompt = canonicalizePromptTokens(profile.negativePrompt);
  const sections = [sceneContext, lockedTraits.join(", ")];
  if (styleAnchor.length > 0) sections.push(styleAnchor.join(", "));
  if (negativePrompt.length > 0) sections.push(`--no ${negativePrompt.join(", ")}`);
  return sections.join(", ");
}

export async function sha256Utf8(value: string): Promise<string> {
  if (!globalThis.crypto?.subtle) {
    throw new CharacterPromptCompilerError("BRIDGE_CRYPTO_UNAVAILABLE");
  }
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Deterministic compiler boundary. It has no I/O and never mutates its inputs;
 * crypto is asynchronous only because Web Crypto is the portable desktop-web
 * implementation. The output is stable for identical inputs.
 */
export async function compileScenePromptV1(scene: ScenePromptInputV1, profile: CharacterProfileV1): Promise<CompiledScenePromptV1> {
  const compiledPrompt = compileScenePromptTextV1(scene, profile);
  return {
    characterId: profile.id,
    characterVersion: profile.version,
    sceneId: normalizeText(scene.sceneId),
    compiledPrompt,
    compiledPromptSha256: await sha256Utf8(compiledPrompt),
  };
}
