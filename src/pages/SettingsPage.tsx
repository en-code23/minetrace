import {
  ChevronRight,
  EyeOff,
  Laptop,
  Moon,
  ShieldCheck,
  Sun,
  type LucideIcon,
} from "lucide-react";
import { type ReactNode } from "react";
import { PageHeader } from "../components/ui/PageHeader";
import { useUiStore, type ThemePreference } from "../stores/ui-store";

export function SettingsPage() {
  const theme = useUiStore((state) => state.theme);
  const setTheme = useUiStore((state) => state.setTheme);
  const privacyMask = useUiStore((state) => state.privacyMask);
  const togglePrivacyMask = useUiStore((state) => state.togglePrivacyMask);

  return (
    <div className="page page--settings">
      <PageHeader
        eyebrow="Preferences · Current device"
        title="Settings"
        description="Choose how MineTrace looks and how detected multiplayer destinations appear on this device."
      />

      <div className="settings-layout">
        <nav className="settings-index" aria-label="Settings sections">
          <a href="#appearance"><Sun aria-hidden="true" /><span>Appearance</span><ChevronRight aria-hidden="true" /></a>
          <a href="#privacy"><ShieldCheck aria-hidden="true" /><span>Privacy</span><ChevronRight aria-hidden="true" /></a>
        </nav>

        <div className="settings-sections">
          <SettingsSection id="appearance" eyebrow="Interface" title="Appearance" description="A warm limestone light theme and deepslate dark theme share the same evidence language.">
            <div className="theme-picker" role="radiogroup" aria-label="Theme">
              <ThemeOption value="dark" current={theme} onChange={setTheme} icon={Moon} label="Deepslate" detail="Dark" />
              <ThemeOption value="light" current={theme} onChange={setTheme} icon={Sun} label="Limestone" detail="Light" />
              <ThemeOption value="system" current={theme} onChange={setTheme} icon={Laptop} label="System" detail="Automatic" />
            </div>
          </SettingsSection>

          <SettingsSection id="privacy" eyebrow="Private by default" title="Privacy" description="No account, telemetry, DNS lookup, or online profile request is enabled.">
            <SettingsToggle icon={EyeOff} title="Mask server addresses" detail="Hide detected multiplayer destinations throughout MineTrace on this device." checked={privacyMask} onChange={() => togglePrivacyMask()} />
            <div className="privacy-audit">
              <ShieldCheck aria-hidden="true" />
              <div><strong>No network-backed features</strong><span>Scanning and analytics stay local. File access is limited to controlled backend commands.</span></div>
            </div>
          </SettingsSection>
        </div>
      </div>
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
