import {
  Boxes,
  ChevronsLeft,
  ChevronsRight,
  Clock3,
  DatabaseZap,
  Globe2,
  Laptop,
  Layers3,
  LayoutDashboard,
  PanelLeftClose,
  PanelLeftOpen,
  ScanLine,
  Search,
  Server,
  Settings2,
  ShieldCheck,
  UserRound,
} from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useRef, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { clsx } from "clsx";
import packageMetadata from "../../../package.json";
import { detectPlatform, isTauriRuntime, shortcutModifier } from "../../lib/runtime";
import { useUiStore } from "../../stores/ui-store";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { TraceMark } from "../ui/TraceMark";
import { SearchDialog } from "./SearchDialog";
import { ArchiveRefreshMonitor } from "../system/ArchiveRefreshMonitor";
import { AutomationController } from "../system/AutomationController";
import { FirstRunSetup } from "../system/FirstRunSetup";

const primaryNav = [
  { to: "/overview", label: "Overview", icon: LayoutDashboard },
  { to: "/profile", label: "Profile", icon: UserRound },
  { to: "/sessions", label: "Sessions", icon: Clock3 },
  { to: "/instances", label: "Instances", icon: Layers3 },
  { to: "/worlds", label: "Worlds", icon: Globe2 },
  { to: "/servers", label: "Servers", icon: Server },
  { to: "/versions", label: "Versions", icon: Boxes },
];

const routeTitles: Record<string, { eyebrow: string; title: string }> = {
  "/overview": { eyebrow: "Archive", title: "Overview" },
  "/profile": { eyebrow: "Player", title: "Profile" },
  "/sessions": { eyebrow: "Evidence", title: "Sessions" },
  "/instances": { eyebrow: "Library", title: "Instances" },
  "/worlds": { eyebrow: "Library", title: "Worlds" },
  "/servers": { eyebrow: "Library", title: "Servers" },
  "/versions": { eyebrow: "Library", title: "Versions" },
  "/scan": { eyebrow: "Sources", title: "Scan center" },
  "/settings": { eyebrow: "MineTrace", title: "Settings" },
};

export function AppShell() {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchOpen, setSearchOpen] = useState(false);
  const [installedVersion, setInstalledVersion] = useState(packageMetadata.version);
  const mainRef = useRef<HTMLElement>(null);
  const sidebarCollapsed = useUiStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useUiStore((state) => state.toggleSidebar);
  const privacyMask = useUiStore((state) => state.privacyMask);
  const togglePrivacyMask = useUiStore((state) => state.togglePrivacyMask);
  const platform = detectPlatform();
  const native = isTauriRuntime();
  const title = routeTitles[location.pathname] ?? routeTitles["/overview"];

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => mainRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [location.pathname]);

  useEffect(() => {
    if (!native) return;
    let active = true;
    void getVersion()
      .then((version) => {
        if (active) setInstalledVersion(version);
      })
      .catch(() => {
        // The packaged frontend version remains a truthful fallback.
      });
    return () => {
      active = false;
    };
  }, [native]);

  return (
    <div
      className={clsx(
        "app-shell",
        sidebarCollapsed && "app-shell--sidebar-collapsed",
        `platform-${platform}`,
      )}
    >
      <ArchiveRefreshMonitor enabled={native} />
      <AutomationController enabled={native} />
      <a className="skip-link" href="#main-content">
        Skip to content
      </a>

      <aside className="sidebar" aria-label="Primary navigation">
        <div className="sidebar__drag-zone" data-tauri-drag-region />
        <div className="sidebar__brand-row">
          <NavLink className="brand" to="/overview" aria-label="MineTrace overview">
            <TraceMark compact={sidebarCollapsed} />
            <span className="brand__text">
              Mine<span>Trace</span>
            </span>
          </NavLink>
          <button
            className="sidebar__collapse"
            type="button"
            onClick={toggleSidebar}
            aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {sidebarCollapsed ? <PanelLeftOpen /> : <PanelLeftClose />}
          </button>
        </div>

        <nav className="sidebar__nav">
          <p className="sidebar__section-label">Explore</p>
          {primaryNav.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              aria-label={label}
              className={({ isActive }) => clsx("nav-item", isActive && "nav-item--active")}
              title={sidebarCollapsed ? label : undefined}
            >
              <Icon aria-hidden="true" />
              <span>{label}</span>
              <ChevronsRight className="nav-item__arrow" aria-hidden="true" />
            </NavLink>
          ))}

          <p className="sidebar__section-label sidebar__section-label--sources">Sources</p>
          <NavLink
            to="/scan"
            aria-label="Scan center"
            className={({ isActive }) => clsx("nav-item", isActive && "nav-item--active")}
            title={sidebarCollapsed ? "Scan center" : undefined}
          >
            <ScanLine aria-hidden="true" />
            <span>Scan center</span>
          </NavLink>
        </nav>

        <div className="sidebar__footer">
          <div className="local-status">
            <span className="local-status__icon" aria-hidden="true">
              <ShieldCheck />
            </span>
            <span className="local-status__copy">
              <strong>Local only</strong>
              <small>Nothing uploaded</small>
            </span>
          </div>
          <NavLink
            to="/settings"
            aria-label="Settings"
            className={({ isActive }) => clsx("nav-item", isActive && "nav-item--active")}
            title={sidebarCollapsed ? "Settings" : undefined}
          >
            <Settings2 aria-hidden="true" />
            <span>Settings</span>
          </NavLink>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar" data-tauri-drag-region>
          <div className="topbar__title" data-tauri-drag-region>
            <span>{title?.eyebrow}</span>
            <ChevronsRight aria-hidden="true" />
            <strong>{title?.title}</strong>
          </div>

          <div className="topbar__actions">
            {native && (
              <button
                className={clsx("icon-action", privacyMask && "icon-action--active")}
                type="button"
                onClick={togglePrivacyMask}
                aria-pressed={privacyMask}
                aria-label={privacyMask ? "Show server addresses" : "Mask server addresses"}
                title={privacyMask ? "Show server addresses" : "Mask server addresses"}
              >
                <ShieldCheck aria-hidden="true" />
              </button>
            )}
            <button className="search-trigger" type="button" onClick={() => setSearchOpen(true)}>
              <Search aria-hidden="true" />
              <span>Find a page</span>
              <kbd>{shortcutModifier()} K</kbd>
            </button>
            {native && (
              <Button
                variant="primary"
                size="small"
                leadingIcon={<DatabaseZap aria-hidden="true" />}
                onClick={() => navigate("/scan")}
              >
                Scan now
              </Button>
            )}
            <span
              className="topbar__version"
              aria-label={`MineTrace version ${installedVersion}`}
              title={`MineTrace ${installedVersion}`}
              data-tauri-drag-region
            >
              v{installedVersion}
            </span>
          </div>
        </header>

        <main ref={mainRef} id="main-content" className="main-content" tabIndex={-1}>
          {native ? (
            <Outlet />
          ) : (
            <div className="page page--empty">
              <EmptyState
                icon={Laptop}
                title="Open MineTrace on your desktop"
                description="This browser build contains no demo archive and cannot access Minecraft folders or the local SQLite database. Run the Tauri app to use MineTrace."
              />
            </div>
          )}
        </main>
      </section>

      <nav className="mobile-nav" aria-label="Mobile navigation">
        {primaryNav.slice(0, 3).map(({ to, label, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) => clsx("mobile-nav__item", isActive && "mobile-nav__item--active")}
          >
            <Icon aria-hidden="true" />
            <span>{label}</span>
          </NavLink>
        ))}
        <NavLink
          to="/scan"
          className={({ isActive }) => clsx("mobile-nav__item", isActive && "mobile-nav__item--active")}
        >
          <ScanLine aria-hidden="true" />
          <span>Scan</span>
        </NavLink>
        <NavLink
          to="/settings"
          className={({ isActive }) => clsx("mobile-nav__item", isActive && "mobile-nav__item--active")}
        >
          <Settings2 aria-hidden="true" />
          <span>Settings</span>
        </NavLink>
      </nav>

      <button
        className="sidebar-edge-toggle"
        type="button"
        onClick={toggleSidebar}
        aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {sidebarCollapsed ? <ChevronsRight /> : <ChevronsLeft />}
      </button>

      <SearchDialog open={searchOpen} onOpenChange={setSearchOpen} />
      <FirstRunSetup enabled={native} />
    </div>
  );
}
