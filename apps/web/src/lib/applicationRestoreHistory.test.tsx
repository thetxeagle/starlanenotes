import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
vi.mock("./api", () => ({ api: {}, getApiBase: () => "/api/v1" }));
import {
  applicationHistoryScope,
  sqliteApplicationHistoryScope,
  parseApplicationRestoreHistory,
} from "./applicationRestoreHistory";
import { ApplicationRestoreHistorySummary } from "../components/admin/ApplicationRestoreHistorySummary";
const complete = () => ({
  status: "completed_isolated_test",
  completed_at: "2026-10-15T00:00:00Z",
  scope: applicationHistoryScope,
  operation: "00000000-0000-4000-8000-000000000001",
  pages: 1,
  drafts: 2,
  registered_assets: 3,
  legacy_assets: 1,
  exact_binary_bytes: "104857695",
  sampled_humans: 4,
  semantics: "historical_completed_isolated_application_test_at_recorded_time",
  full_supported_restore_verified: false,
});
describe("historical isolated application-test observation", () => {
  it("shows the recorded timestamp, bounded scope and counts without a current verified badge", () => {
    const value = parseApplicationRestoreHistory(complete());
    const markup = renderToStaticMarkup(<ApplicationRestoreHistorySummary value={value} />);
    expect(markup).toContain('dateTime="2026-10-15T00:00:00Z"');
    expect(markup).toContain("PostgreSQL application test with preserved versioned attachments");
    expect(markup).toContain("4");
    expect(markup).toContain("104,857,695 bytes");
    expect(markup).toContain("Historical result at the recorded time");
    expect(markup).toContain(
      "Current edits and complete restore verification are outside this test",
    );
    expect(markup).not.toContain("text-success");
    expect(markup).not.toContain('role="status"');
  });
  it("shows only the exact supported SQLite historical scope without promoting current health", () => {
    const value = parseApplicationRestoreHistory({ ...complete(), scope: sqliteApplicationHistoryScope });
    const markup = renderToStaticMarkup(<ApplicationRestoreHistorySummary value={value} />);
    expect(markup).toContain("SQLite application test with preserved versioned attachments");
    expect(markup).not.toContain("PostgreSQL");
    expect(markup).toContain("Historical result at the recorded time");
    expect(markup).not.toContain("text-success");
    expect(() => parseApplicationRestoreHistory({ ...complete(), scope: "restored-sqlite-latest" })).toThrow(/unavailable/);
  });
  it.each([
    ["status", "verified"],
    ["full_supported_restore_verified", true],
    ["scope", "caller-certified"],
    ["completed_at", null],
    ["completed_at", "not-a-time"],
    ["operation", "arbitrary"],
    ["pages", 0],
    ["drafts", -1],
    ["registered_assets", 257],
    ["sampled_humans", 9],
    ["sampled_humans", 0],
    ["exact_binary_bytes", "1073741825"],
    ["exact_binary_bytes", "01"],
    ["semantics", "fresh_current_source_verified"],
  ])("refuses malformed or expanded %s claims", (key, value) => {
    expect(() => parseApplicationRestoreHistory({ ...complete(), [key as string]: value })).toThrow(
      /unavailable/,
    );
  });
  it("requires complete counts and refuses an asset total outside the tested bound", () => {
    const value: Record<string, unknown> = complete();
    delete value.legacy_assets;
    expect(() => parseApplicationRestoreHistory(value)).toThrow(/unavailable/);
    expect(() =>
      parseApplicationRestoreHistory({ ...complete(), registered_assets: 200, legacy_assets: 100 }),
    ).toThrow(/unavailable/);
  });
  it("keeps absent/unavailable history separate from completion and rejects retained claims", () => {
    expect(renderToStaticMarkup(<ApplicationRestoreHistorySummary />)).toBe("Unavailable");
    const unavailable = {
      status: "unavailable",
      semantics: complete().semantics,
      full_supported_restore_verified: false,
    };
    expect(
      renderToStaticMarkup(
        <ApplicationRestoreHistorySummary value={parseApplicationRestoreHistory(unavailable)} />,
      ),
    ).toBe("Unavailable");
    expect(() =>
      parseApplicationRestoreHistory({ ...unavailable, completed_at: complete().completed_at }),
    ).toThrow(/unavailable/);
  });
  it("shows genuine zero attachment counts only within a completed historical scope", () => {
    const value = parseApplicationRestoreHistory({
      ...complete(),
      registered_assets: 0,
      legacy_assets: 0,
      exact_binary_bytes: "0",
    });
    expect(renderToStaticMarkup(<ApplicationRestoreHistorySummary value={value} />)).toContain(
      "0 bytes",
    );
  });
});
