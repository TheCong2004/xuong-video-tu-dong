export interface FlowordStoryPlanInput {
  readonly prompt: string;
  readonly language: string;
  readonly targetDurationSeconds: number;
  readonly modelId?: string;
  readonly inputArtifactIds: readonly string[];
  readonly sourceMetadata: Record<string, unknown>;
  readonly scenes: Record<string, unknown>;
  readonly research?: Record<string, unknown>;
  readonly workflowMode?: string;
}

function originalSceneSpans(targetDurationSeconds: number): SceneSpan[] {
  if (!Number.isFinite(targetDurationSeconds) || targetDurationSeconds <= 0) {
    throw new Error("target duration is invalid");
  }
  const count = Math.max(1, Math.ceil(targetDurationSeconds / 4));
  const secondsPerScene = targetDurationSeconds / count;
  return Array.from({ length: count }, (_, index) => ({
    index,
    start_seconds: index * secondsPerScene,
    end_seconds: index === count - 1 ? targetDurationSeconds : (index + 1) * secondsPerScene,
  }));
}

interface SceneSpan {
  readonly index: number;
  readonly start_seconds: number;
  readonly end_seconds: number;
}

function finiteNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function sourceDuration(metadata: Record<string, unknown>, scenes: readonly SceneSpan[]): number {
  const probe = metadata.probe;
  if (probe && typeof probe === "object") {
    const record = probe as Record<string, unknown>;
    const duration = finiteNumber(record.duration_seconds) ?? finiteNumber(record.durationSeconds);
    if (duration !== undefined && duration > 0) return duration;
  }
  return scenes.at(-1)?.end_seconds ?? 0;
}

function parseScenes(value: Record<string, unknown>): SceneSpan[] {
  if (!Array.isArray(value.scenes) || value.scenes.length === 0) {
    throw new Error("scenes.scenes must be a non-empty array");
  }
  return value.scenes.map((scene, position) => {
    if (!scene || typeof scene !== "object") throw new Error(`scene ${position} must be an object`);
    const record = scene as Record<string, unknown>;
    const start = finiteNumber(record.start_seconds);
    const end = finiteNumber(record.end_seconds);
    if (start === undefined || end === undefined || start < 0 || end <= start) {
      throw new Error(`scene ${position} has invalid timing`);
    }
    return {
      index: finiteNumber(record.index) ?? position,
      start_seconds: start,
      end_seconds: end,
    };
  });
}

function purpose(index: number, total: number): "hook" | "development" | "payoff" | "cta" {
  if (index === 0) return "hook";
  if (index === total - 1) return total > 2 ? "cta" : "payoff";
  if (index === total - 2 && total > 3) return "payoff";
  return "development";
}

export function buildFlowordStoryPlan(input: FlowordStoryPlanInput) {
  const prompt = input.prompt?.trim();
  if (!prompt) throw new Error("prompt is required");
  const originalCreation = input.workflowMode === "original" || input.workflowMode === "original_creation";
  if (!originalCreation && (!Array.isArray(input.inputArtifactIds) || input.inputArtifactIds.length < 2)) {
    throw new Error("at least source_metadata and scenes artifact IDs are required");
  }
  const sceneSpans = originalCreation
    ? originalSceneSpans(input.targetDurationSeconds)
    : parseScenes(input.scenes);
  const duration = originalCreation
    ? input.targetDurationSeconds
    : sourceDuration(input.sourceMetadata, sceneSpans);
  if (duration <= 0) throw new Error("source duration is invalid");
  const researchUsed = Boolean(input.research);
  const beats = sceneSpans.map((scene, index) => ({
    id: `beat-${index + 1}`,
    sourceSceneIndex: scene.index,
    startSeconds: scene.start_seconds,
    endSeconds: scene.end_seconds,
    purpose: purpose(index, sceneSpans.length),
    narrationGoal: originalCreation
      ? `${purpose(index, sceneSpans.length)} for original scene ${scene.index}`
      : `${purpose(index, sceneSpans.length)} for source scene ${scene.index}`,
  }));
  const story = {
    version: 1,
    kind: "floword_story_plan",
    premise: prompt,
    language: input.language,
    targetDurationSeconds: input.targetDurationSeconds,
    sourceDurationSeconds: duration,
    researchUsed,
    beats,
  } as const;
  const requestContext = {
    userPrompt: prompt,
    sourceMetadata: input.sourceMetadata,
    story,
    research: input.research ?? null,
  };
  const instruction = [
    "Create the final short-video script from this Story Studio plan.",
    `Context: ${JSON.stringify(requestContext)}`,
    "Return ONLY JSON with: title, hook, cta, language, target_duration_seconds, scenes.",
    "Each scene requires: id, index, narration, caption, visual_instruction, search_keywords, emotion, duration_ms.",
  ].join("\n");
  const scriptRequest = {
    version: 1,
    executor: "omniroute",
    model: input.modelId?.trim() || "auto",
    responseFormat: "floword_script_v1",
    inputArtifactIds: [...input.inputArtifactIds],
    messages: [{ role: "user", content: instruction }],
  } as const;
  return { story, scriptRequest };
}
