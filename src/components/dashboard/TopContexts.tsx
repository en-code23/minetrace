import { ArrowUpRight, Boxes, Layers3, RadioTower, Rocket, TreePine } from "lucide-react";
import { Link } from "react-router-dom";
import { formatDuration } from "../../lib/format";
import { useUiStore } from "../../stores/ui-store";
import type { DashboardData } from "../../types/domain";

export function TopContexts({ top, totalMinutes }: { top: DashboardData["top"]; totalMinutes: number }) {
  const privacyMask = useUiStore((state) => state.privacyMask);
  const denominator = Math.max(totalMinutes, 1);
  const rows = [
    { key: "instance", label: "Instance", value: top.instance, icon: Layers3, to: "/instances" },
    { key: "version", label: "Version", value: top.version, icon: Boxes, to: "/versions" },
    { key: "launcher", label: "Launcher", value: top.launcher, icon: Rocket, to: "/instances" },
    { key: "server", label: "Server", value: top.server, icon: RadioTower, to: "/servers" },
    { key: "world", label: "World", value: top.world, icon: TreePine, to: "/worlds" },
  ];

  return (
    <section className="panel contexts-panel" aria-labelledby="contexts-heading">
      <header className="panel__header">
        <div>
          <p className="eyebrow">Dominant evidence contexts</p>
          <h2 id="contexts-heading">Top contexts</h2>
        </div>
      </header>
      <div className="context-list">
        {rows.map(({ key, label, value, icon: Icon, to }) => {
          const destinationOnly = key === "server" || key === "world";
          const maskServer = key === "server" && privacyMask && !value.name.startsWith("No ");
          return <Link className="context-row" to={to} key={key}>
            <span className="context-row__icon" aria-hidden="true">
              <Icon />
            </span>
            <span className="context-row__copy">
              <small>{label}</small>
              <strong>{maskServer ? "Masked multiplayer server" : value.name}</strong>
            </span>
            <span className="context-row__metric">
              <strong>{destinationOnly ? "Observed" : formatDuration(value.minutes, true)}</strong>
              <span>{destinationOnly ? "session-linked" : `${Math.round((value.minutes / denominator) * 100)}%`}</span>
            </span>
            <ArrowUpRight className="context-row__arrow" aria-hidden="true" />
          </Link>;
        })}
      </div>
    </section>
  );
}
