import {
  Boxes,
  Clock3,
  Command,
  CornerDownLeft,
  Globe2,
  Layers3,
  LayoutDashboard,
  Search,
  Server,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";

type SearchEntry = {
  id: string;
  label: string;
  detail: string;
  group: string;
  to: string;
  icon: typeof Search;
};

const routeEntries: SearchEntry[] = [
  { id: "route-overview", label: "Overview", detail: "Archive summary", group: "Pages", to: "/overview", icon: LayoutDashboard },
  { id: "route-sessions", label: "Sessions", detail: "Timeline and evidence", group: "Pages", to: "/sessions", icon: Clock3 },
  { id: "route-instances", label: "Instances", detail: "Launcher profiles", group: "Pages", to: "/instances", icon: Layers3 },
  { id: "route-worlds", label: "Worlds", detail: "Session-linked worlds", group: "Pages", to: "/worlds", icon: Globe2 },
  { id: "route-servers", label: "Servers", detail: "Observed connections", group: "Pages", to: "/servers", icon: Server },
  { id: "route-versions", label: "Versions", detail: "Version history", group: "Pages", to: "/versions", icon: Boxes },
];

export function SearchDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        onOpenChange(true);
      }
    };
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [onOpenChange]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      window.setTimeout(() => inputRef.current?.focus(), 0);
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  const results = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return routeEntries;
    return routeEntries
      .filter((entry) => `${entry.label} ${entry.detail} ${entry.group}`.toLowerCase().includes(normalized))
      .slice(0, 12);
  }, [query]);

  function choose(entry: SearchEntry) {
    navigate(entry.to);
    setQuery("");
    onOpenChange(false);
  }

  return (
    <dialog
      ref={dialogRef}
      className="search-dialog"
      onClose={() => onOpenChange(false)}
      onClick={(event) => {
        if (event.target === dialogRef.current) onOpenChange(false);
      }}
    >
      <div className="search-dialog__surface">
        <div className="search-dialog__input-row">
          <Search aria-hidden="true" />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActiveIndex(0);
            }}
            onKeyDown={(event) => {
              if (results.length === 0) return;
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setActiveIndex((index) => (index + 1) % results.length);
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                setActiveIndex((index) => (index - 1 + results.length) % results.length);
              } else if (event.key === "Enter") {
                event.preventDefault();
                const entry = results[activeIndex];
                if (entry) choose(entry);
              }
            }}
            placeholder="Search pages…"
            aria-label="Search MineTrace"
            aria-controls="minetrace-search-results"
            aria-activedescendant={results[activeIndex] ? `search-result-${results[activeIndex].id}` : undefined}
          />
          <kbd>Esc</kbd>
        </div>

        <div className="search-dialog__results" id="minetrace-search-results" role="listbox" aria-label="Search results">
          {results.length > 0 ? (
            results.map((entry, index) => {
              const Icon = entry.icon;
              return (
                <button
                  key={entry.id}
                  id={`search-result-${entry.id}`}
                  type="button"
                  role="option"
                  aria-selected={index === activeIndex}
                  className={`search-result ${index === activeIndex ? "search-result--active" : ""}`}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => choose(entry)}
                >
                  <span className="search-result__icon" aria-hidden="true">
                    <Icon />
                  </span>
                  <span className="search-result__copy">
                    <strong>{entry.label}</strong>
                    <small>{entry.detail}</small>
                  </span>
                  <span className="search-result__group">{entry.group}</span>
                  <CornerDownLeft className="search-result__enter" aria-hidden="true" />
                </button>
              );
            })
          ) : (
            <div className="search-dialog__empty">
              <Command aria-hidden="true" />
              <strong>No trace found</strong>
              <span>Try the name of a MineTrace page.</span>
            </div>
          )}
        </div>

        <footer className="search-dialog__footer">
          <span>
            <kbd>↑</kbd><kbd>↓</kbd> navigate
          </span>
          <span>
            <kbd>↵</kbd> open
          </span>
          <strong>Search stays on this device</strong>
        </footer>
      </div>
    </dialog>
  );
}
