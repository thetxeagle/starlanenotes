import { describe, expect, it } from "vitest";
import { getDocumentStats } from "./documentStats";

describe("document stats", () => {
  it("counts markdown prose and estimates reading time", () => {
    const stats = getDocumentStats("# Title\n\nThis is a short note with [a link](https://example.com).");

    expect(stats.words).toBe(8);
    expect(stats.readingMinutes).toBe(1);
  });

  it("does not count fenced code content as prose", () => {
    const stats = getDocumentStats("Intro words\n\n```ts\nconst value = 'ignored';\n```");

    expect(stats.words).toBe(2);
  });
});
