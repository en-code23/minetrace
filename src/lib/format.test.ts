import { describe, expect, it } from "vitest";
import { formatSessionTime, formatShortDate, formatShortMonth } from "./format";

describe("observed timestamp formatting", () => {
  it("preserves the wall clock and date carried by an RFC 3339 observed offset", () => {
    const observed = "2025-12-31T23:50:00-12:00";

    expect(formatSessionTime(observed)).toBe("23:50");
    expect(formatShortDate(observed)).toBe("Dec 31, 2025");
    expect(formatShortMonth(observed)).toBe("Dec 2025");
  });

  it("does not shift a positive-offset observation into the previous local day", () => {
    const observed = "2026-01-01T00:15:00+14:00";

    expect(formatSessionTime(observed)).toBe("00:15");
    expect(formatShortDate(observed)).toBe("Jan 1, 2026");
    expect(formatShortMonth(observed)).toBe("Jan 2026");
  });
});
