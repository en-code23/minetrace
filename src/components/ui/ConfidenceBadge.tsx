import { CircleCheck, CircleDashed, CircleHelp, Signal } from "lucide-react";
import type { Confidence } from "../../types/domain";

const confidenceContent: Record<Confidence, { label: string; detail: string }> = {
  verified: { label: "Verified", detail: "Start and end are directly supported by log evidence." },
  high: { label: "High estimate", detail: "Most timestamps are supported; one boundary was inferred." },
  partial: { label: "Partial", detail: "The available file provides incomplete session evidence." },
  unknown: { label: "Unknown", detail: "The available evidence cannot establish a reliable duration." },
};

function ConfidenceIcon({ confidence }: { confidence: Confidence }) {
  if (confidence === "verified") return <CircleCheck aria-hidden="true" />;
  if (confidence === "high") return <Signal aria-hidden="true" />;
  if (confidence === "partial") return <CircleDashed aria-hidden="true" />;
  return <CircleHelp aria-hidden="true" />;
}

export function ConfidenceBadge({ confidence, compact = false }: { confidence: Confidence; compact?: boolean }) {
  const content = confidenceContent[confidence];
  return (
    <span
      className={`confidence confidence--${confidence}`}
      title={content.detail}
      aria-label={`${content.label}. ${content.detail}`}
    >
      <ConfidenceIcon confidence={confidence} />
      {!compact && <span>{content.label}</span>}
    </span>
  );
}

