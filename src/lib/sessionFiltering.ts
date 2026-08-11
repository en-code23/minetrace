import type { Session, SessionKind } from "../types/domain";
import type { SessionQueryScope } from "./navigation";

export function sessionMatchesQuery(
  session: Session,
  query: string,
  scope: SessionQueryScope | "generic" = "generic",
  kind: SessionKind | "all" = "all",
): boolean {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;

  if (scope === "instance") return session.instance.toLowerCase() === normalized;
  if (scope === "version") return session.version.toLowerCase() === normalized;
  if (scope === "context") {
    return session.contexts.some(
      (context) => (kind === "all" || context.kind === kind)
        && context.name.toLowerCase() === normalized,
    );
  }

  return [
    session.destination,
    session.instance,
    session.launcher,
    session.version,
    session.loader,
    ...session.contexts.map((context) => context.name),
  ].some((value) => value?.toLowerCase().includes(normalized));
}

export function sessionMatchesKind(
  session: Pick<Session, "kind" | "contexts">,
  kind: SessionKind | "all",
): boolean {
  if (kind === "all") return true;
  return session.kind === kind
    || ((kind === "server" || kind === "world")
      && session.contexts.some((context) => context.kind === kind));
}
