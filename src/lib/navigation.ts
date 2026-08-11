import type { Session } from "../types/domain";

export type SessionQueryScope = "context" | "instance" | "version";

export function scopedSessionRoute(
  value: string,
  scope: SessionQueryScope,
  kind?: "server" | "world",
): string {
  const params = new URLSearchParams({ q: value });
  if (kind) params.set("kind", kind);
  params.set("scope", scope);
  return `/sessions?${params.toString()}`;
}

export function serverSessionRoute(serverName: string, privacyMask: boolean): string {
  return privacyMask
    ? "/sessions?kind=server"
    : scopedSessionRoute(serverName, "context", "server");
}

export function visibleServerSearchQuery(query: string, privacyMask: boolean): string {
  return privacyMask ? "" : query;
}

export function shouldMaskDetectedServerQuery(
  query: string,
  sessions: Array<Pick<Session, "kind" | "destination" | "contexts">>,
  privacyMask: boolean,
): boolean {
  const normalized = query.trim().toLowerCase();
  return privacyMask
    && normalized.length > 0
    && sessions.some(
      (session) => (session.kind === "server"
        && session.destination?.toLowerCase().includes(normalized))
        || session.contexts.some(
          (context) => context.kind === "server"
            && context.name.toLowerCase().includes(normalized),
        ),
    );
}
