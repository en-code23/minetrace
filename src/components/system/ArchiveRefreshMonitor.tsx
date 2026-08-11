import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { getScanStatus } from "../../lib/runtime";
import { canonicalArchiveChanged, scanIsRunning } from "../../lib/scanStatus";
import type { ScanProgress } from "../../types/domain";

const archiveQueryRoots = ["dashboard", "sessions", "instances", "worlds", "servers", "versions"];

export function ArchiveRefreshMonitor({ enabled }: { enabled: boolean }) {
  const queryClient = useQueryClient();
  const previousStatus = useRef<Pick<ScanProgress, "id" | "phase" | "datasetRevision"> | null>(null);
  const status = useQuery({
    queryKey: ["scan-status", "global"],
    queryFn: getScanStatus,
    enabled,
    staleTime: 0,
    refetchInterval: (query) => scanIsRunning(query.state.data) ? 750 : 5_000,
    refetchIntervalInBackground: false,
  });

  useEffect(() => {
    if (!status.data) return;

    const current = {
      id: status.data.id,
      phase: status.data.phase,
      datasetRevision: status.data.datasetRevision,
    };
    const shouldRefresh = canonicalArchiveChanged(previousStatus.current, current);
    previousStatus.current = current;

    if (shouldRefresh) {
      void Promise.all(
        archiveQueryRoots.map((queryKey) => queryClient.invalidateQueries({ queryKey: [queryKey] })),
      );
    }
  }, [queryClient, status.data]);

  return null;
}
