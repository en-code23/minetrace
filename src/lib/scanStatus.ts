import type { ScanProgress } from "../types/domain";

type ScanArchiveIdentity = Pick<ScanProgress, "id" | "phase" | "datasetRevision">;

export function scanIsRunning(status: Pick<ScanProgress, "phase"> | undefined): boolean {
  return status !== undefined
    && ["discovering", "indexing", "parsing", "aggregating"].includes(status.phase);
}

export function canonicalArchiveChanged(
  previous: ScanArchiveIdentity | null,
  current: ScanArchiveIdentity,
): boolean {
  if (!previous || !current.id || current.phase !== "complete") return false;

  return previous.phase !== "complete"
    || previous.id !== current.id
    || previous.datasetRevision !== current.datasetRevision;
}
