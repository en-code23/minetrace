import { RefreshCw, ScanLine, ShieldCheck, Sparkles } from "lucide-react";
import { useState } from "react";
import { useUiStore } from "../../stores/ui-store";
import { Button } from "../ui/Button";

export function FirstRunSetup({ enabled }: { enabled: boolean }) {
  const setupComplete = useUiStore((state) => state.setupComplete);
  const completeSetup = useUiStore((state) => state.completeSetup);
  const [autoScan, setAutoScan] = useState(true);
  const [autoUpdate, setAutoUpdate] = useState(true);

  if (!enabled || setupComplete) return null;

  return (
    <div className="setup-backdrop" role="presentation">
      <section className="setup-dialog" role="dialog" aria-modal="true" aria-labelledby="setup-title">
        <header className="setup-dialog__header">
          <span className="setup-dialog__mark" aria-hidden="true"><Sparkles /></span>
          <div>
            <p className="eyebrow">One-time setup</p>
            <h2 id="setup-title">Keep MineTrace current?</h2>
            <p>Choose what should happen automatically. Both choices stay editable in Settings.</p>
          </div>
        </header>

        <div className="setup-dialog__choices">
          <PreferenceChoice
            icon={ScanLine}
            title="Refresh your statistics"
            detail="A standard read-only scan runs when MineTrace opens and every 30 minutes while it stays open."
            automatic={autoScan}
            onChange={setAutoScan}
          />
          <PreferenceChoice
            icon={RefreshCw}
            title="Update MineTrace"
            detail="Signed releases are downloaded and installed when MineTrace starts."
            automatic={autoUpdate}
            onChange={setAutoUpdate}
          />
        </div>

        <footer className="setup-dialog__footer">
          <span><ShieldCheck aria-hidden="true" /> Scans stay on this device. App updates come from signed GitHub releases.</span>
          <Button variant="primary" onClick={() => completeSetup({ autoScan, autoUpdate })}>Save choices</Button>
        </footer>
      </section>
    </div>
  );
}

function PreferenceChoice({
  icon: Icon,
  title,
  detail,
  automatic,
  onChange,
}: {
  icon: typeof ScanLine;
  title: string;
  detail: string;
  automatic: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <fieldset className="setup-choice">
      <legend><Icon aria-hidden="true" /><span><strong>{title}</strong><small>{detail}</small></span></legend>
      <label className={automatic ? "setup-option setup-option--selected" : "setup-option"}>
        <input type="radio" checked={automatic} onChange={() => onChange(true)} />
        <span><strong>Automatic <em>Recommended</em></strong><small>Keep it current without extra steps.</small></span>
      </label>
      <label className={!automatic ? "setup-option setup-option--selected" : "setup-option"}>
        <input type="radio" checked={!automatic} onChange={() => onChange(false)} />
        <span><strong>Manual</strong><small>Only run it when you choose.</small></span>
      </label>
    </fieldset>
  );
}
