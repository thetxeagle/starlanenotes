import { describe, expect, it } from "vitest";
import { rewriteExactAssetReferences } from "./nativeArchiveReferences";

describe("exact reference raw-byte preservation", () => {
  it("preserves CRLF, emoji, whitespace, titles, code and prefix pins outside exact destinations", () => {
    const short = "starlane://asset/version-1",
      long = "starlane://asset/version-10";
    const source = `😀 Prose ${short}\r\n\r\n[One](<${short}> "${long}")\r\n\r\n![Ten](${long})\r\n\r\n\`${short}\`\r\n`;
    expect(
      rewriteExactAssetReferences(
        source,
        new Map([
          [short, "one.bin"],
          [long, "ten.bin"],
        ]),
      ),
    ).toBe(
      `😀 Prose ${short}\r\n\r\n[One](<one.bin> "${long}")\r\n\r\n![Ten](ten.bin)\r\n\r\n\`${short}\`\r\n`,
    );
  });
  it("does not cascade a replacement that also happens to be another original alias", () => {
    expect(
      rewriteExactAssetReferences(
        "[A](starlane://asset/a) [B](starlane://asset/b)",
        new Map([
          ["starlane://asset/a", "starlane://asset/b"],
          ["starlane://asset/b", "final.bin"],
        ]),
      ),
    ).toBe("[A](starlane://asset/b) [B](final.bin)");
  });
  it("keeps ambiguity in prose/code/title harmless but refuses an accepted reference definition", () => {
    const pin = "starlane://asset/same.bin";
    const ambiguous = new Set([pin]);
    const plain = `Prose ${pin}\n\n\`${pin}\`\n\n[X](https://example.invalid "${pin}")`;
    expect(rewriteExactAssetReferences(plain, new Map(), ambiguous)).toBe(plain);
    expect(() =>
      rewriteExactAssetReferences(`[X][key]\n\n[key]: <${pin}>`, new Map(), ambiguous),
    ).toThrow(/ambiguous/);
  });
  it("preserves external URLs and unknown version pins without prefix guessing", () => {
    const body =
      "[Remote](https://example.invalid/starlane://asset/v1) [Unknown](starlane://asset/v10)";
    expect(rewriteExactAssetReferences(body, new Map([["starlane://asset/v1", "owned.bin"]]))).toBe(
      body,
    );
  });
});

describe("static renderer raw fence/reference ownership", () => {
  it.each(["video", "gallery", "carousel"])(
    "preserves %s fences as code without weakening native archive refusal",
    (kind) => {
      const pin = "starlane://asset/version-1";
      const source = `\`\`\`${kind}\n![code](${pin})\nsrc: ${pin}\n\`\`\`\n`;
      expect(rewriteExactAssetReferences(source, new Map([[pin, "one.bin"]]))).toBe(source);
    },
  );
  it("changes only the first accepted definition and preserves a rejected duplicate exactly", () => {
    const pin = "starlane://asset/version-1";
    const source = `[X][owned]\r\n\r\n[owned]: <${pin}> "${pin}"\r\n[owned]: ${pin}\r\n`;
    expect(rewriteExactAssetReferences(source, new Map([[pin, "one.bin"]]))).toBe(
      `[X][owned]\r\n\r\n[owned]: <one.bin> "${pin}"\r\n[owned]: ${pin}\r\n`,
    );
  });
});
