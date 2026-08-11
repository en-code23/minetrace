import { ListFilter } from "lucide-react";

export function ArchiveWindowNote({
  loaded,
  total,
  noun,
  detail,
}: {
  loaded: number;
  total: number;
  noun: string;
  detail: string;
}) {
  const remaining = Math.max(0, total - loaded);

  return (
    <aside className="archive-window-note" aria-label={`Loaded ${noun} archive window`}>
      <ListFilter aria-hidden="true" />
      <div>
        <strong>{loaded.toLocaleString()} of {total.toLocaleString()} {noun} loaded</strong>
        <span>{remaining.toLocaleString()} remain outside this bounded view. {detail}</span>
      </div>
    </aside>
  );
}
