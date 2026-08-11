import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import type { DashboardData } from "../../types/domain";
import { ActivityChart } from "./ActivityChart";
import { CoverageNote } from "./CoverageNote";
import { HeroLedger } from "./HeroLedger";

const dashboard: DashboardData = {
  archiveState: "ready",
  generatedAt: "2026-08-10T12:00:00Z",
  coverage: {
    firstDetectedAt: "2020-12-01T10:00:00-12:00",
    lastDetectedAt: "2021-02-28T23:00:00+14:00",
    quality: "partial",
    score: 60,
    verifiedShare: 0.5,
    warning: "Coverage is conservative.",
    observedMonths: 2,
    gapMonths: 1,
  },
  totals: {
    playtimeMinutes: 180,
    uniquePlaytimeMinutes: 180,
    sessions: 2,
    activeDays: 2,
    longestSessionMinutes: 120,
    averageSessionMinutes: 90,
  },
  top: {
    launcher: { name: "Prism Launcher", minutes: 180 },
    instance: { name: "Builder Pack", minutes: 180 },
    version: { name: "1.21.4", minutes: 180 },
    server: { name: "No server evidence", minutes: 0 },
    world: { name: "Workshop", minutes: 180 },
  },
  monthly: [
    {
      month: "2020-12",
      label: "Dec",
      minutes: 60,
      sessions: 1,
      estimatedShare: 0.05,
      confidence: "partial",
      coverage: "observed",
    },
    {
      month: "2021-01",
      label: "Jan",
      minutes: null,
      sessions: null,
      estimatedShare: null,
      confidence: "unknown",
      coverage: "missing",
    },
    {
      month: "2021-02",
      label: "Feb",
      minutes: 120,
      sessions: 1,
      estimatedShare: 0,
      confidence: "verified",
      coverage: "observed",
    },
  ],
  daily: [],
  recentSessions: [],
};

describe("truthful dashboard evidence states", () => {
  it("renders explicit monthly gaps, conservative confidence, and the actual endpoint", () => {
    const hero = renderToStaticMarkup(
      <MemoryRouter><HeroLedger data={dashboard} /></MemoryRouter>,
    );
    const chart = renderToStaticMarkup(<ActivityChart data={dashboard.monthly} />);

    expect(hero).toContain("December 2020");
    expect(hero).toContain("February 2021");
    expect(hero).not.toContain(">Now<");
    expect(hero).toContain("evidence-seam__segment--partial");
    expect(hero).toContain("evidence-seam__segment--unknown");
    expect(chart).toContain("January 2021: no evidence available");
    expect(chart).toContain("average across 2 observed months");
    expect(chart).not.toContain("observed year");
  });

  it("distinguishes a completed empty scan from an archive that was never scanned", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <CoverageNote
          coverage={{ ...dashboard.coverage, observedMonths: 0, gapMonths: 0 }}
          archiveState="scannedNoEvidence"
        />
      </MemoryRouter>,
    );

    expect(html).toContain("Completed scan found no session evidence");
    expect(html).toContain("Scan again");
    expect(html).not.toContain("awaits the first scan");
  });

  it("keeps unknown session durations unknown in the metric rail", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <HeroLedger
          data={{
            ...dashboard,
            totals: {
              ...dashboard.totals,
              longestSessionMinutes: null,
              averageSessionMinutes: null,
            },
          }}
        />
      </MemoryRouter>,
    );

    expect(html.match(/Unknown/g)).toHaveLength(2);
    expect(html).not.toContain("NaN");
  });
});
