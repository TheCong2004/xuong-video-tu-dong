import { describe, expect, it } from "vitest";
import { buildFlowordStoryPlan } from "./floword-story-plan.js";

const baseInput = {
  prompt: "Kể lại video theo phong cách giáo dục",
  language: "vi",
  targetDurationSeconds: 30,
  inputArtifactIds: ["metadata-1", "scenes-1"],
  sourceMetadata: { probe: { duration_seconds: 12.5, width: 1280, height: 720 } },
  scenes: {
    scenes: [
      { index: 0, start_seconds: 0, end_seconds: 5 },
      { index: 1, start_seconds: 5, end_seconds: 12.5 },
    ],
  },
};

describe("Floword story planning", () => {
  it("turns real source analysis into story and OmniRoute script request", () => {
    const output = buildFlowordStoryPlan(baseInput);
    expect(output.story.beats).toHaveLength(2);
    expect(output.story.sourceDurationSeconds).toBe(12.5);
    expect(output.scriptRequest.executor).toBe("omniroute");
    expect(output.scriptRequest.messages[0].content).toContain("Kể lại video");
    expect(output.scriptRequest.responseFormat).toBe("floword_script_v1");
  });

  it("accepts skipped research and incorporates completed research", () => {
    expect(buildFlowordStoryPlan(baseInput).story.researchUsed).toBe(false);
    const output = buildFlowordStoryPlan({
      ...baseInput,
      research: { records: [{ title: "Xu hướng kể chuyện nhanh" }] },
      inputArtifactIds: [...baseInput.inputArtifactIds, "research-1"],
    });
    expect(output.story.researchUsed).toBe(true);
    expect(output.scriptRequest.messages[0].content).toContain("Xu hướng kể chuyện nhanh");
  });

  it("derives an original-creation plan from duration without source artifacts", () => {
    const output = buildFlowordStoryPlan({
      prompt: "Tạo video ngắn giới thiệu CapCut Automation với hook mạnh",
      language: "vi",
      targetDurationSeconds: 11,
      workflowMode: "original",
      inputArtifactIds: [],
      sourceMetadata: {},
      scenes: {},
    });
    expect(output.story.beats.length).toBeGreaterThan(1);
    expect(output.story.sourceDurationSeconds).toBe(11);
    expect(output.story.beats.at(-1)?.endSeconds).toBe(11);
    expect(output.scriptRequest.executor).toBe("omniroute");
  });
});
