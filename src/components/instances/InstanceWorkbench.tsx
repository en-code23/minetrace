import {
  AlertTriangle,
  Box,
  Boxes,
  CircleCheck,
  ExternalLink,
  FileCode2,
  Globe2,
  Rocket,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { scopedSessionRoute } from "../../lib/navigation";
import { formatDuration, formatRelativeDate } from "../../lib/format";
import type { Confidence, InstanceSummary } from "../../types/domain";
import { Button } from "../ui/Button";
import { ConfidenceBadge } from "../ui/ConfidenceBadge";

const confidenceGuidance: Record<Confidence, { label: string; detail: string }> = {
  verified: {
    label: "Verified aggregate",
    detail: "The contributing sessions have directly observed start and end boundaries.",
  },
  high: {
    label: "High-confidence aggregate",
    detail: "Most contributing boundaries are observed; at least one boundary was inferred.",
  },
  partial: {
    label: "Partial aggregate",
    detail: "Some retained logs are incomplete, so this profile does not represent guaranteed total playtime.",
  },
  unknown: {
    label: "Unknown aggregate confidence",
    detail: "The retained evidence cannot establish reliable boundaries for all contributing activity.",
  },
};

function lastObserved(value: string | null): string {
  return value ? `last observed ${formatRelativeDate(value)}` : "last observation unknown";
}

export function InstanceWorkbench({ instance }: { instance: InstanceSummary }) {
  const navigate = useNavigate();
  const confidence = confidenceGuidance[instance.confidence];
  const crashShare = instance.sessions > 0
    ? `${Math.round((instance.crashCount / instance.sessions) * 1_000) / 10}% of reconstructed launches`
    : "No reconstructed launches";

  return (
    <article className="instance-workbench">
      <header className="instance-workbench__header">
        <div className={`instance-glyph instance-glyph--${instance.accent}`} aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <div className="instance-workbench__title">
          <div>
            <p className="eyebrow">Selected instance</p>
            <h2>{instance.name}</h2>
          </div>
          <ConfidenceBadge confidence={instance.confidence} />
        </div>
        <div className="instance-workbench__actions">
          <Button
            variant="secondary"
            size="small"
            trailingIcon={<ExternalLink aria-hidden="true" />}
            onClick={() => navigate(scopedSessionRoute(instance.name, "instance"))}
          >
            Inspect history
          </Button>
        </div>
      </header>

      <div className="instance-workbench__hero">
        <div>
          <span>Detected session runtime</span>
          <strong>{formatDuration(instance.totalMinutes)}</strong>
          <small>{instance.sessions.toLocaleString()} reconstructed {instance.sessions === 1 ? "launch" : "launches"} · {lastObserved(instance.lastPlayedAt)}</small>
        </div>
        <dl>
          <div>
            <dt><Rocket aria-hidden="true" /> Launcher</dt>
            <dd>{instance.launcher}</dd>
          </div>
          <div>
            <dt><Box aria-hidden="true" /> Version</dt>
            <dd>{instance.version ?? "Not detected"}</dd>
          </div>
          <div>
            <dt><FileCode2 aria-hidden="true" /> Loader</dt>
            <dd>{instance.loader ?? "Not detected"}</dd>
          </div>
        </dl>
      </div>

      <div className="instance-workbench__grid">
        <section className="workbench-section">
          <header>
            <div>
              <p className="eyebrow">Observed contents</p>
              <h3>Profile evidence</h3>
            </div>
          </header>
          <div className="inventory-list">
            <InventoryRow
              icon={Boxes}
              label="Reconstructed sessions"
              value={instance.sessions.toLocaleString()}
              detail="Canonical launches linked to this profile"
            />
            <InventoryRow
              icon={Globe2}
              label="Local worlds"
              value={instance.worldCount.toLocaleString()}
              detail="Distinct world names observed in session logs"
            />
            <InventoryRow
              icon={AlertTriangle}
              label="Crash exits"
              value={instance.crashCount.toLocaleString()}
              detail={crashShare}
              warning={instance.crashCount > 0}
            />
            <InventoryRow
              icon={FileCode2}
              label="Installed mods"
              value={instance.modCount === null ? "Not indexed" : instance.modCount.toLocaleString()}
              detail={instance.modCount === null ? "No mods inventory has been read" : "Read-only inventory count"}
            />
          </div>
        </section>

        <section className="workbench-section workbench-section--provenance">
          <header>
            <div>
              <p className="eyebrow">Reading guide</p>
              <h3>How to interpret this profile</h3>
            </div>
          </header>
          <div className="provenance-stack">
            <EvidenceNote
              number="01"
              title="Observed boundaries"
              detail="Runtime includes reconstructed start-to-end intervals. Sessions with unknown duration contribute no invented minutes."
            />
            <EvidenceNote
              number="02"
              title="Session-linked worlds"
              detail="World counts come from destinations observed in logs, not from save-folder or NBT inspection."
            />
            <EvidenceNote number="03" title={confidence.label} detail={confidence.detail} />
          </div>
        </section>
      </div>
    </article>
  );
}

function InventoryRow({
  icon: Icon,
  label,
  value,
  detail,
  warning = false,
}: {
  icon: typeof CircleCheck;
  label: string;
  value: string;
  detail: string;
  warning?: boolean;
}) {
  return (
    <div className="inventory-row">
      <span className="inventory-row__icon" aria-hidden="true"><Icon /></span>
      <div>
        <strong>{label}</strong>
        <small>{detail}</small>
      </div>
      <span className={warning ? "inventory-row__value inventory-row__value--warning" : "inventory-row__value"}>{value}</span>
    </div>
  );
}

function EvidenceNote({ number, title, detail }: { number: string; title: string; detail: string }) {
  return (
    <div className="provenance-step">
      <span>{number}</span>
      <div>
        <strong>{title}</strong>
        <small>{detail}</small>
      </div>
    </div>
  );
}
