import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import {
  serverSessionRoute,
  scopedSessionRoute,
  shouldMaskDetectedServerQuery,
  visibleServerSearchQuery,
} from "../lib/navigation";
import { sessionMatchesKind, sessionMatchesQuery } from "../lib/sessionFiltering";
import type { Session } from "../types/domain";

vi.mock("../lib/runtime", () => ({
  getInstances: vi.fn(),
  getWorlds: vi.fn(),
  getServers: vi.fn(),
  getVersions: vi.fn(),
  getSessions: vi.fn(),
}));

import { InstancesPage } from "./InstancesPage";
import { LibraryPage } from "./LibraryPage";
import { SessionsPage } from "./SessionsPage";

function renderExplorer(
  element: ReactElement,
  queryKey: string[],
  data: unknown,
  initialEntry = "/",
): string {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  });
  queryClient.setQueryData(queryKey, data);

  return renderToStaticMarkup(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[initialEntry]}>{element}</MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("native explorer pages", () => {
  it("renders nullable instance evidence without inventing profile metadata", () => {
    const html = renderExplorer(
      <InstancesPage />,
      ["instances", "all"],
      {
        items: [{
          id: "instance-1",
          name: "Builder Pack",
          launcher: "Prism Launcher",
          version: null,
          loader: null,
          totalMinutes: 125,
          sessions: 3,
          lastPlayedAt: null,
          modCount: null,
          worldCount: 1,
          crashCount: 0,
          confidence: "partial",
          accent: "moss",
        }],
        total: 501,
        truncated: true,
      },
    );

    expect(html).toContain("Builder Pack");
    expect(html).toContain("Version not detected");
    expect(html).toContain("Loader not detected");
    expect(html).toContain("Installed mods");
    expect(html).toContain("Not indexed");
    expect(html).toContain("1 of 501 profiles loaded");
    expect(html).toContain("500 remain outside this bounded view");
    expect(html).not.toContain("Explorer preview");
    expect(html).not.toContain("Source files");
  });

  it("labels world totals as session-linked and keeps unavailable metadata unknown", () => {
    const html = renderExplorer(
      <LibraryPage kind="worlds" />,
      ["worlds", "all"],
      {
        items: [{
          id: "world-1",
          name: "Workshop",
          instance: "Builder Pack",
          mode: null,
          version: null,
          totalMinutes: 95,
          lastPlayedAt: null,
          sizeLabel: null,
          confidence: "high",
          runtimeBasis: "sessionLinked",
        }],
        total: 1,
        truncated: false,
      },
    );

    expect(html).toContain("Inspect reconstructed sessions linked to Workshop");
    expect(html).toContain("Session-linked runtime");
    expect(html).toContain("Observed in session logs");
    expect(html).toContain("Version not detected");
    expect(html).toContain("It is not an estimate of exact time spent inside that world");
  });

  it("shows a scan action instead of demo versions when the archive is empty", () => {
    const html = renderExplorer(
      <LibraryPage kind="versions" />,
      ["versions", "all"],
      { items: [], total: 0, truncated: false },
    );

    expect(html).toContain("No versions in the archive");
    expect(html).toContain("Open Scan center");
    expect(html).not.toContain("Sample upgrade timeline");
  });

  it("does not place a server address in navigation while privacy masking is active", () => {
    expect(serverSessionRoute("private.example.net:25565", true)).toBe("/sessions?kind=server");
    expect(serverSessionRoute("private.example.net:25565", false)).toBe(
      "/sessions?q=private.example.net%3A25565&kind=server&scope=context",
    );
    expect(scopedSessionRoute("Workshop", "context", "world")).toBe(
      "/sessions?q=Workshop&kind=world&scope=context",
    );
    expect(scopedSessionRoute("1.21.8", "version")).toBe(
      "/sessions?q=1.21.8&scope=version",
    );
  });

  it("masks a previously routed detected server destination when privacy is enabled", () => {
    const sessions = [{
      kind: "server" as const,
      destination: "private.example.net:25565",
      contexts: [{ kind: "server" as const, name: "private.example.net:25565" }],
    }];

    expect(shouldMaskDetectedServerQuery("private.example.net:25565", sessions, true)).toBe(true);
    expect(shouldMaskDetectedServerQuery("private.example.net:25565", sessions, false)).toBe(false);
    expect(shouldMaskDetectedServerQuery("Builder Pack", sessions, true)).toBe(false);
  });

  it("finds mixed sessions through their canonical world and server contexts", () => {
    const mixedSession: Session = {
      id: "session-mixed",
      startedAt: "2026-08-10T14:00:00+02:00",
      endedAt: "2026-08-10T15:00:00+02:00",
      durationMinutes: 60,
      launcher: "Prism Launcher",
      instance: "Builder Pack",
      version: "1.21.8",
      loader: "Fabric",
      kind: "mixed",
      destination: "Multiple destinations",
      contexts: [
        { kind: "server", name: "private.example.net:25565" },
        { kind: "world", name: "Workshop" },
      ],
      exitKind: "clean",
      confidence: "high",
      source: "logs/latest.log",
    };

    expect(sessionMatchesQuery(mixedSession, "private.example.net")).toBe(true);
    expect(sessionMatchesQuery(mixedSession, "Workshop")).toBe(true);
    expect(sessionMatchesQuery(mixedSession, "Workshop", "context", "world")).toBe(true);
    expect(sessionMatchesQuery(mixedSession, "Builder Pack", "context", "world")).toBe(false);
    expect(sessionMatchesQuery(mixedSession, "1.21.8", "version")).toBe(true);
    expect(sessionMatchesKind(mixedSession, "server")).toBe(true);
    expect(sessionMatchesKind(mixedSession, "world")).toBe(true);
    expect(sessionMatchesKind(mixedSession, "menu")).toBe(false);
    expect(shouldMaskDetectedServerQuery("private.example.net", [mixedSession], true)).toBe(true);
  });

  it("removes an existing server search value from the visible masked interface", () => {
    expect(visibleServerSearchQuery("private.example.net:25565", true)).toBe("");
    expect(visibleServerSearchQuery("private.example.net:25565", false)).toBe(
      "private.example.net:25565",
    );
  });

  it("makes a bounded session archive window explicit", () => {
    const html = renderExplorer(
      <SessionsPage />,
      ["sessions", "all"],
      {
        items: [{
          id: "session-1",
          startedAt: "2026-08-10T14:00:00+02:00",
          endedAt: "2026-08-10T15:00:00+02:00",
          durationMinutes: 60,
          launcher: "Prism Launcher",
          instance: "Builder Pack",
          version: "1.21.8",
          loader: "Fabric",
          kind: "world",
          destination: "Workshop",
          contexts: [{ kind: "world", name: "Workshop" }],
          exitKind: "clean",
          confidence: "verified",
          source: "logs/latest.log",
        }],
        total: 501,
        truncated: true,
      },
      "/sessions",
    );

    expect(html).toContain("1 of 501 sessions loaded");
    expect(html).toContain("500 remain outside this bounded view");
    expect(html).toContain("filters on this page apply to that window");
  });
});
