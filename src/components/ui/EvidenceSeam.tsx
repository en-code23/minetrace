import { clsx } from "clsx";
import type { Confidence } from "../../types/domain";

export interface EvidenceSegment {
  id: string;
  weight: number;
  intensity: 0 | 1 | 2 | 3 | 4;
  confidence: Confidence;
  label: string;
}

interface EvidenceSeamProps {
  segments: EvidenceSegment[];
  compact?: boolean;
  label?: string;
}

export function EvidenceSeam({ segments, compact = false, label = "History evidence coverage" }: EvidenceSeamProps) {
  return (
    <div className={clsx("evidence-seam", compact && "evidence-seam--compact")} role="img" aria-label={label}>
      {segments.map((segment) => (
        <span
          key={segment.id}
          className={clsx(
            "evidence-seam__segment",
            `evidence-seam__segment--${segment.confidence}`,
            `evidence-seam__segment--level-${segment.intensity}`,
          )}
          style={{ flexGrow: segment.weight }}
          title={segment.label}
        />
      ))}
    </div>
  );
}

