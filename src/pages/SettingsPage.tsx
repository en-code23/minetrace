import {
  ChevronRight,
  EyeOff,
  Download,
  Laptop,
  Moon,
  RefreshCw,
  ScanLine,
  ShieldCheck,
  Sun,
  type LucideIcon,
} from "lucide-react";
import { type ReactNode } from "react";
import { PageHeader } from "../components/ui/PageHeader";
import { useUiStore, type ThemePreference } from "../stores/ui-store";
import { useUpdateStore } from "../stores/update-store";
import { Button } from "../components/ui/Button";

export function SettingsPage() {
  const theme = useUiStore((state) => state.theme);
  const setTheme = useUiStore((state) => state.setTheme);
  const privacyMask = useUiStore((state) => state.privacyMask);
  const togglePrivacyMask = useUiStore((state) => state.togglePrivacyMask);
  const autoScan = useUiStore((state) => state.autoScan);
  const setAutoScan = useUiStore((state) => state.setAutoScan);
  const autoUpdate = useUiStore((state) => state.autoUpdate);
  const setAutoUpdate = useUiStore((state) => state.setAutoUpdate);

  return (
    <div className="page page--settings">
      <PageHeader
        eyebrow="Preferences · Current device"
        title="Settings"
        description="Appearance, privacy, scanning, and app updates for this device."
      />

      <div className="settings-layout">
        <nav className="settings-index" aria-label="Settings sections">
          <a href="#appearance"><Sun aria-hidden="true" /><span>Appearance</span><ChevronRight aria-hidden="true" /></a>
          <a href="#privacy"><ShieldCheck aria-hidden="true" /><span>Privacy</span><ChevronRight aria-hidden="true" /></a>
          <a href="#automation"><RefreshCw aria-hidden="true" /><span>Automation</span><ChevronRight aria-hidden="true" /></a>
        </nav>

        <div className="settings-sections">
          <SettingsSection id="appearance" eyebrow="Interface" title="Appearance" description="A warm limestone light theme and deepslate dark theme share the same evidence language.">
            <div className="theme-picker" role="radiogroup" aria-label="Theme">
              <ThemeOption value="dark" current={theme} onChange={setTheme} icon={Moon} label="Deepslate" detail="Dark" />
              <ThemeOption value="light" current={theme} onChange={setTheme} icon={Sun} label="Limestone" detail="Light" />
              <ThemeOption value="system" current={theme} onChange={setTheme} icon={Laptop} label="System" detail="Automatic" />
            </div>
          </SettingsSection>

          <SettingsSection id="privacy" eyebrow="Private by default" title="Privacy" description="Play data, account details, and server history are not uploaded.">
            <SettingsToggle icon={EyeOff} title="Mask server addresses" detail="Obscure most of each detected multiplayer destination throughout MineTrace on this device." checked={privacyMask} onChange={() => togglePrivacyMask()} />
            <div className="privacy-audit">
              <ShieldCheck aria-hidden="true" />
              <div><strong>Play analysis stays local</strong><span>Only the updater contacts GitHub, when enabled or checked manually. No play data is included.</span></div>
            </div>
          </SettingsSection>

          <SettingsSection id="automation" eyebrow="Keep it current" title="Automation" description="Choose what MineTrace does without an extra click.">
            <SettingsToggle icon={ScanLine} title="Refresh statistics automatically" detail="Run a standard read-only scan at launch and every 30 minutes while MineTrace is open." checked={autoScan} onChange={setAutoScan} />
            <SettingsToggle icon={RefreshCw} title="Install app updates automatically" detail="Check for signed GitHub releases at launch, install an available update, then restart." checked={autoUpdate} onChange={setAutoUpdate} />
            <UpdateControls />
          </SettingsSection>
        </div>
      </div>
    </div>
  );
}

function UpdateControls() {
  const state = useUpdateStore((value) => value.state);
  const version = useUpdateStore((value) => value.availableVersion);
  const progress = useUpdateStore((value) => value.progress);
  const message = useUpdateStore((value) => value.message);
  const checkForUpdates = useUpdateStore((value) => value.checkForUpdates);
  const installAvailableUpdate = useUpdateStore((value) => value.installAvailableUpdate);
  const busy = state === "checking" || state === "downloading" || state === "ready";

  return (
    <div className={`update-control update-control--${state}`}>
      <span className="settings-toggle-row__icon" aria-hidden="true"><Download /></span>
      <div>
        <strong>{version ? `MineTrace ${version}` : "App updates"}</strong>
        <small>{message ?? "Check GitHub Releases for a newer signed version."}</small>
        {state === "downloading" && <progress value={progress ?? undefined} max={1} aria-label="Update download progress" />}
      </div>
      {state === "available" ? (
        <Button size="small" variant="primary" onClick={() => void installAvailableUpdate()}>Update & restart</Button>
      ) : (
        <Button size="small" variant="secondary" loading={busy} onClick={() => void checkForUpdates(false)}>Check now</Button>
      )}
    </div>
  );
}

function SettingsSection({
  id,
  eyebrow,
  title,
  description,
  children,
}: {
  id: string;
  eyebrow: string;
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-section" id={id}>
      <header><p className="eyebrow">{eyebrow}</p><h2>{title}</h2><p>{description}</p></header>
      <div className="settings-section__body">{children}</div>
    </section>
  );
}

function SettingsToggle({
  icon: Icon,
  title,
  detail,
  checked,
  onChange,
}: {
  icon: LucideIcon;
  title: string;
  detail: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="settings-toggle-row">
      <span className="settings-toggle-row__icon" aria-hidden="true"><Icon /></span>
      <div><strong>{title}</strong><small>{detail}</small></div>
      <button type="button" role="switch" aria-label={title} aria-checked={checked} className={`switch ${checked ? "switch--on" : ""}`} onClick={() => onChange(!checked)}><span /></button>
    </div>
  );
}

function ThemeOption({
  value,
  current,
  onChange,
  icon: Icon,
  label,
  detail,
}: {
  value: ThemePreference;
  current: ThemePreference;
  onChange: (theme: ThemePreference) => void;
  icon: LucideIcon;
  label: string;
  detail: string;
}) {
  return (
    <button type="button" role="radio" aria-checked={current === value} className={`theme-option ${current === value ? "theme-option--selected" : ""}`} onClick={() => onChange(value)}>
      <span className={`theme-option__preview theme-option__preview--${value}`} aria-hidden="true"><i /><i /><Icon /></span>
      <span><strong>{label}</strong><small>{detail}</small></span>
    </button>
  );
}
