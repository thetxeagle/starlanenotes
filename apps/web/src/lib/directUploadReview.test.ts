import { describe, expect, it } from "vitest";
import { validDirectUploadAcknowledgementPath } from "./directUploadReview";
const id = "11111111-1111-4111-8111-111111111111", documentId = "doc-original";
const ack = { id, filename: `${id}.bin`, url: `/api/v1/files/spaces/actual-space-slug/attachments/${documentId}/${id}.bin` };
describe("original upload acknowledgement storage path", () => {
  it("accepts the real server slug independently of the stored space UUID", () => {
    expect(validDirectUploadAcknowledgementPath(documentId, ack)).toBe(true);
  });
  it.each(["other-page", "../doc-original", "doc-original?query"])("refuses another or unsafe document %s", document => {
    expect(validDirectUploadAcknowledgementPath(document, ack)).toBe(false);
  });
  it.each(["../actual-space-slug", "..", "foreign/actual-space-slug", "slug?query", "slug%2fescape", "slug#fragment", "slug\\escape"])("refuses ambiguous storage slug %s", slug => {
    expect(validDirectUploadAcknowledgementPath(documentId, { ...ack, url: `/api/v1/files/spaces/${slug}/attachments/${documentId}/${ack.filename}` })).toBe(false);
  });
  it("refuses a foreign URL authority or changed immutable filename", () => {
    expect(validDirectUploadAcknowledgementPath(documentId, { ...ack, url: `https://foreign.invalid${ack.url}` })).toBe(false);
    expect(validDirectUploadAcknowledgementPath(documentId, { ...ack, filename: `${id}.bin/other` })).toBe(false);
    expect(validDirectUploadAcknowledgementPath(documentId, { ...ack, filename: "display-name.bin" })).toBe(false);
  });
});
