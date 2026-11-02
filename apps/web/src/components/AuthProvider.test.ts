import { describe, expect, it, vi } from "vitest";
import { api } from "@/lib/api";
import { dataClient } from "@/lib/dataClient";
import {
  buildAuthCallbackCleanUrl,
  downloadServerWorkspace,
  mergeAuthUserUpdate,
  parseAuthCallbackFragment,
} from "./AuthProvider";

describe("auth callback fragment parsing", () => {
  it("downloads every personal-space page body while leaving other spaces metadata-only", async () => {
    const fetch = vi.spyOn(api, "fetch").mockImplementation(async (path) => {
      if (path === "/spaces") {
        return [
          {
            id: "personal",
            name: "Personal Space",
            slug: "personal",
            created_by: "user-1",
          },
          {
            id: "shared",
            name: "Shared Space",
            slug: "shared",
            created_by: "user-2",
          },
        ] as never;
      }
      if (path === "/spaces/personal/folders") return [{ id: "folder-1" }] as never;
      if (path === "/spaces/personal/documents?include_nested=true") {
        return [{ id: "doc-1", draft_id: "draft-1" }, { id: "doc-2" }] as never;
      }
      if (path === "/spaces/personal/documents/doc-1/drafts/draft-1") {
        return { id: "draft-1", document_id: "doc-1", title: "One", content: "First" } as never;
      }
      if (path === "/spaces/personal/documents/doc-2/content") {
        return { id: "doc-2", title: "Two", content: "Second" } as never;
      }
      throw new Error(`Unexpected path: ${path}`);
    });
    const listAttachments = vi
      .spyOn(dataClient, "listDocumentAttachments")
      .mockImplementation(async (_spaceId, documentId) =>
        documentId === "doc-1"
          ? [
              {
                id: "asset-1",
                filename: "diagram.png",
                original_name: "Diagram.png",
                mime_type: "image/png",
                size_bytes: 42,
                url: "/api/v1/files/diagram.png",
                created_at: "2026-07-26T21:00:00.000Z",
              },
            ]
          : [],
      );
    const downloadAttachment = vi
      .spyOn(dataClient, "downloadDocumentAttachmentToVault")
      .mockResolvedValue("data:image/png;base64,cG5n");

    await downloadServerWorkspace({ id: "user-1" });

    expect(fetch).toHaveBeenCalledWith("/spaces");
    expect(fetch).toHaveBeenCalledWith("/spaces/personal/folders");
    expect(fetch).toHaveBeenCalledWith("/spaces/personal/documents?include_nested=true");
    expect(fetch).toHaveBeenCalledWith(
      "/spaces/personal/documents/doc-1/drafts/draft-1",
    );
    expect(fetch).not.toHaveBeenCalledWith("/spaces/personal/documents/doc-1/content");
    expect(fetch).toHaveBeenCalledWith("/spaces/personal/documents/doc-2/content");
    expect(fetch).not.toHaveBeenCalledWith("/spaces/shared/folders");
    expect(fetch).not.toHaveBeenCalledWith("/spaces/shared/documents?include_nested=true");
    expect(listAttachments).toHaveBeenCalledWith("personal", "doc-1");
    expect(listAttachments).toHaveBeenCalledWith("personal", "doc-2");
    expect(downloadAttachment).toHaveBeenCalledWith(
      "personal",
      "doc-1",
      expect.objectContaining({ id: "asset-1" }),
    );

    fetch.mockRestore();
    listAttachments.mockRestore();
    downloadAttachment.mockRestore();
  });

  it("extracts complete OAuth tokens and removes them from the hash", () => {
    const parsed = parseAuthCallbackFragment(
      "#access_token=access.jwt&refresh_token=refresh.jwt&state=kept",
    );

    expect(parsed.hasAuthParams).toBe(true);
    expect(parsed.tokens).toEqual({
      accessToken: "access.jwt",
      refreshToken: "refresh.jwt",
    });
    expect(parsed.sanitizedHash).toBe("#state=kept");
  });

  it("treats partial callback tokens as invalid auth params", () => {
    const parsed = parseAuthCallbackFragment("#access_token=access.jwt&view=home");

    expect(parsed.hasAuthParams).toBe(true);
    expect(parsed.tokens).toBeNull();
    expect(parsed.sanitizedHash).toBe("#view=home");
  });

  it("preserves search params when building the cleaned callback URL", () => {
    expect(
      buildAuthCallbackCleanUrl(
        { pathname: "/documents/doc-1", search: "?mode=preview" },
        "#outline",
      ),
    ).toBe("/documents/doc-1?mode=preview#outline");
  });

  it("merges partial auth user updates without inventing a missing user", () => {
    const currentUser = {
      id: "user-1",
      email: "eagle@example.test",
      display_name: "The Eagle",
      role: "owner",
      totp_enabled: false,
    };

    expect(mergeAuthUserUpdate(currentUser, { totp_enabled: true })).toEqual({
      ...currentUser,
      totp_enabled: true,
    });
    expect(mergeAuthUserUpdate(null, { totp_enabled: true })).toBeNull();
  });
});
