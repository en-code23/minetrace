import { format, formatDistanceToNowStrict, parseISO } from "date-fns";

export function formatDuration(totalMinutes: number | null, compact = false): string {
  if (totalMinutes === null) return "Unknown";

  const hours = Math.floor(totalMinutes / 60);
  const minutes = Math.round(totalMinutes % 60);

  if (hours === 0) return `${minutes}m`;
  if (compact) return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  return minutes > 0 ? `${hours.toLocaleString()} h ${minutes} min` : `${hours.toLocaleString()} h`;
}

export function formatHeroDuration(totalMinutes: number): { primary: string; secondary: string } {
  const hours = Math.floor(totalMinutes / 60);
  const minutes = Math.round(totalMinutes % 60);
  return {
    primary: hours.toLocaleString(),
    secondary: `h ${String(minutes).padStart(2, "0")} min`,
  };
}

export function formatSessionTime(isoDate: string): string {
  const observedTime = /T(\d{2}):(\d{2})/.exec(isoDate);
  return observedTime
    ? `${observedTime[1]}:${observedTime[2]}`
    : format(parseISO(isoDate), "HH:mm");
}

export function formatSessionDate(isoDate: string): string {
  return format(parseISO(observedDatePart(isoDate)), "EEEE, MMMM d");
}

export function formatShortDate(isoDate: string): string {
  return format(parseISO(observedDatePart(isoDate)), "MMM d, yyyy");
}

export function formatShortMonth(isoDate: string): string {
  return format(parseISO(`${observedDatePart(isoDate).slice(0, 7)}-01`), "MMM yyyy");
}

export function formatRelativeDate(isoDate: string): string {
  return formatDistanceToNowStrict(parseISO(isoDate), { addSuffix: true });
}

export function formatMonth(isoMonth: string): string {
  return format(parseISO(`${isoMonth}-01`), "MMMM yyyy");
}

export function pluralize(value: number, singular: string, plural = `${singular}s`): string {
  return `${value.toLocaleString()} ${value === 1 ? singular : plural}`;
}

function observedDatePart(isoDate: string): string {
  const observedDate = /^\d{4}-\d{2}-\d{2}/.exec(isoDate)?.[0];
  return observedDate ?? isoDate;
}
