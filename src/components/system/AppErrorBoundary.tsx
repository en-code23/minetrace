import { Component, type ReactNode } from "react";
import { recordFrontendIssue } from "../../lib/runtime";

interface AppErrorBoundaryProps {
  children: ReactNode;
}

interface AppErrorBoundaryState {
  failed: boolean;
}

export class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = { failed: false };

  static getDerivedStateFromError(): AppErrorBoundaryState {
    return { failed: true };
  }

  componentDidCatch(): void {
    // The recovery screen intentionally avoids persisting error text because it
    // can contain private launcher paths or destination evidence.
    void recordFrontendIssue("renderFailure").catch(() => undefined);
  }

  render() {
    if (!this.state.failed) return this.props.children;

    return (
      <main className="app-recovery" role="alert">
        <section className="app-recovery__panel">
          <div className="app-recovery__mark" aria-hidden="true"><i /><i /><i /></div>
          <p className="eyebrow">Local recovery</p>
          <h1>MineTrace needs to reload</h1>
          <p>
            Something unexpected interrupted the interface. Minecraft source files and the last
            completed archive were not changed.
          </p>
          <button className="button button--primary" type="button" onClick={() => window.location.reload()}>
            Reload MineTrace
          </button>
          <small>If this repeats, quit and reopen the desktop app before running another scan.</small>
        </section>
      </main>
    );
  }
}
