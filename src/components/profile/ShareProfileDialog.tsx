import { save } from "@tauri-apps/plugin-dialog";
import { Check, Download, Image as ImageIcon, X } from "lucide-react";
import { useMemo, useState } from "react";
import { formatDuration } from "../../lib/format";
import { saveShareImage } from "../../lib/runtime";
import type { ProfileData } from "../../types/domain";
import { Button } from "../ui/Button";

type ShareField = "player" | "playtime" | "sessions" | "version" | "world" | "stats";

const initialFields: Record<ShareField, boolean> = {
  player: true,
  playtime: true,
  sessions: true,
  version: true,
  world: true,
  stats: true,
};

export function ShareProfileDialog({ profile, onClose }: { profile: ProfileData; onClose: () => void }) {
  const [fields, setFields] = useState(initialFields);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectedCount = useMemo(() => Object.values(fields).filter(Boolean).length, [fields]);

  async function createImage() {
    setSaving(true);
    setSaved(false);
    setError(null);
    try {
      const canvas = await renderShareCard(profile, fields);
      const blob = await new Promise<Blob>((resolve, reject) => {
        canvas.toBlob((value) => value ? resolve(value) : reject(new Error("Could not create the PNG.")), "image/png");
      });
      const path = await save({
        title: "Save MineTrace profile card",
        defaultPath: `MineTrace-${profile.identity?.name ?? "profile"}.png`,
        filters: [{ name: "PNG image", extensions: ["png"] }],
      });
      if (!path) return;
      await saveShareImage(path, Array.from(new Uint8Array(await blob.arrayBuffer())));
      setSaved(true);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : "The profile image could not be saved.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="share-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="share-dialog" role="dialog" aria-modal="true" aria-labelledby="share-title">
        <header>
          <span aria-hidden="true"><ImageIcon /></span>
          <div><p className="eyebrow">Share on social</p><h2 id="share-title">Build a profile card</h2><p>Choose what appears. MineTrace creates a PNG locally and never posts for you.</p></div>
          <button className="icon-action" type="button" onClick={onClose} aria-label="Close share dialog"><X /></button>
        </header>
        <div className="share-fields">
          {([
            ["player", "Player name and head"],
            ["playtime", "Total playtime"],
            ["sessions", "Session count"],
            ["version", "Most-played version"],
            ["world", "Most-played world"],
            ["stats", "In-game stat spotlights"],
          ] as const).map(([key, label]) => (
            <label key={key} className={fields[key] ? "share-field share-field--selected" : "share-field"}>
              <input type="checkbox" checked={fields[key]} onChange={() => setFields((current) => ({ ...current, [key]: !current[key] }))} />
              <span>{label}</span><Check aria-hidden="true" />
            </label>
          ))}
        </div>
        {error && <p className="share-dialog__message share-dialog__message--error" role="alert">{error}</p>}
        {saved && <p className="share-dialog__message" role="status"><Check aria-hidden="true" /> Profile card saved. It is ready to post.</p>}
        <footer>
          <small>{selectedCount} fields selected · no server addresses are included</small>
          <Button variant="primary" leadingIcon={<Download aria-hidden="true" />} loading={saving} disabled={selectedCount === 0} onClick={() => void createImage()}>
            Save share image
          </Button>
        </footer>
      </section>
    </div>
  );
}

async function renderShareCard(profile: ProfileData, fields: Record<ShareField, boolean>): Promise<HTMLCanvasElement> {
  const canvas = document.createElement("canvas");
  canvas.width = 1200;
  canvas.height = 630;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Canvas is unavailable.");

  context.fillStyle = "#121713";
  context.fillRect(0, 0, 1200, 630);
  context.fillStyle = "#1a211c";
  context.fillRect(42, 42, 1116, 546);
  context.strokeStyle = "#3c493f";
  context.lineWidth = 2;
  context.strokeRect(42, 42, 1116, 546);
  context.fillStyle = "#91b68c";
  context.fillRect(42, 42, 10, 546);
  context.font = "600 22px system-ui";
  context.fillStyle = "#91b68c";
  context.fillText("MINETRACE  /  PLAYER DOSSIER", 88, 92);

  let headingX = 88;
  if (fields.player && (profile.currentSkin || profile.avatar)) {
    const image = await loadImage(profile.currentSkin?.dataUrl ?? profile.avatar!.dataUrl);
    context.imageSmoothingEnabled = false;
    if (profile.currentSkin) {
      context.drawImage(image, 8, 8, 8, 8, 88, 128, 136, 136);
      if (image.height >= 64) context.drawImage(image, 40, 8, 8, 8, 88, 128, 136, 136);
    } else {
      context.drawImage(image, 88, 128, 136, 136);
    }
    headingX = 256;
  }
  context.fillStyle = "#f1ead9";
  context.font = "700 56px system-ui";
  context.fillText(fields.player ? (profile.identity?.name ?? "Local player") : "My Minecraft archive", headingX, 188);
  context.fillStyle = "#9ea89f";
  context.font = "400 22px system-ui";
  context.fillText("Built from local play evidence", headingX, 228);

  const metrics: Array<[string, string]> = [];
  if (fields.playtime) metrics.push([formatDuration(profile.summary.totalPlaytimeMinutes, true), "PLAYTIME"]);
  if (fields.sessions) metrics.push([profile.summary.sessions.toLocaleString(), "SESSIONS"]);
  if (fields.version) metrics.push([profile.summary.mostPlayedVersion ?? "Not observed", "TOP VERSION"]);
  if (fields.world) metrics.push([profile.summary.mostPlayedWorld ?? "Not observed", "TOP WORLD"]);
  if (fields.stats) {
    for (const stat of profile.randomStats.slice(0, Math.max(0, 4 - metrics.length))) {
      metrics.push([formatStat(stat.value, stat.unit), stat.label.toUpperCase()]);
    }
  }
  const width = Math.min(238, 1024 / Math.max(metrics.length, 1));
  metrics.slice(0, 4).forEach(([value, label], index) => {
    const x = 88 + index * (width + 18);
    context.fillStyle = index % 2 === 0 ? "#222b24" : "#242922";
    context.fillRect(x, 336, width, 142);
    context.fillStyle = "#f1ead9";
    context.font = "700 31px system-ui";
    context.fillText(trimCanvasText(context, value, width - 32), x + 16, 398);
    context.fillStyle = "#9ea89f";
    context.font = "600 15px system-ui";
    context.fillText(trimCanvasText(context, label, width - 32), x + 16, 442);
  });
  context.fillStyle = "#65736a";
  context.font = "500 18px system-ui";
  context.fillText("MineTrace · private, local-first Minecraft history", 88, 548);
  return canvas;
}

function loadImage(source: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("The local skin could not be drawn."));
    image.src = source;
  });
}

function trimCanvasText(context: CanvasRenderingContext2D, value: string, maxWidth: number): string {
  if (context.measureText(value).width <= maxWidth) return value;
  let candidate = value;
  while (candidate.length > 1 && context.measureText(`${candidate}…`).width > maxWidth) candidate = candidate.slice(0, -1);
  return `${candidate}…`;
}

function formatStat(value: number, unit: string): string {
  if (unit === "ticks") return formatDuration(Math.round(value / 1_200), true);
  if (unit === "centimeters") return value >= 100_000 ? `${(value / 100_000).toLocaleString(undefined, { maximumFractionDigits: 1 })} km` : `${Math.round(value / 100).toLocaleString()} m`;
  if (unit === "tenths") return (value / 10).toLocaleString(undefined, { maximumFractionDigits: 1 });
  return value.toLocaleString();
}
