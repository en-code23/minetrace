import { describe, expect, it } from "vitest";
import { canonicalArchiveChanged, scanIsRunning } from "./scanStatus";

describe("scan archive refresh state", () => {
  it("invalidates canonical queries when an observed run completes", () => {
    expect(canonicalArchiveChanged(
      { id: "scan-1", phase: "parsing", datasetRevision: null },
      { id: "scan-1", phase: "complete", datasetRevision: 4 },
    )).toBe(true);
  });

  it("detects a fast completed run even when its running phase was missed", () => {
    expect(canonicalArchiveChanged(
      { id: "scan-1", phase: "complete", datasetRevision: 4 },
      { id: "scan-2", phase: "complete", datasetRevision: 5 },
    )).toBe(true);
  });

  it("does not repeatedly invalidate for the same completed snapshot", () => {
    const completed = { id: "scan-2", phase: "complete" as const, datasetRevision: 5 };
    expect(canonicalArchiveChanged(completed, completed)).toBe(false);
    expect(scanIsRunning({ phase: "aggregating" })).toBe(true);
    expect(scanIsRunning({ phase: "complete" })).toBe(false);
  });
});
