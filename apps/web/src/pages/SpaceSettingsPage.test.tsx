import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  getLocalSpaceEditState,
  getSpaceDraftActionState,
  getSpaceDraftEmptyMessage,
  getSpaceGroupAccessActionState,
  getSpaceGroupEmptyMessage,
  getSpaceUserAccessActionState,
  getSpaceSettingsRoute,
  SPACE_GROUPS_DESKTOP_TABLE_CLASS,
  SPACE_GROUPS_MOBILE_LIST_CLASS,
  SPACE_DRAFTS_DESKTOP_TABLE_CLASS,
  SPACE_DRAFTS_MOBILE_LIST_CLASS,
  SPACE_USERS_DESKTOP_TABLE_CLASS,
  SPACE_USERS_MOBILE_LIST_CLASS,
  SpaceGroupAccessMobileRow,
  SpaceDraftMobileRow,
  SpaceUserAccessMobileRow,
  SPACE_SETTINGS_GENERAL_LAYOUT_CLASS,
  sortSpacesForSettings,
} from "./SpaceSettingsPage";
import type { DraftSummaryRecord, SpaceRecord } from "@/lib/dataClient";
import { DEFAULT_SPACE_ROLE_DEFINITIONS } from "@/lib/spacePermissions";

const member = {
  user_id: "user-1",
  email: "alex@example.com",
  display_name: "Alex Example",
  effective_role: "editor",
  direct_role: "editor",
  blocked: false,
  sources: [
    { kind: "direct" as const, id: "user-1", name: "Alex Example", role: "editor" },
    { kind: "group" as const, id: "group-1", name: "Writers", role: "commenter" },
  ],
};

const managedGroup = {
  group_id: "group-managed",
  name: "Operations",
  description: "A managed group for space operators.",
  role: "editor",
  member_count: 14,
  managed: true,
};

const sharedGroup = {
  group_id: "group-shared",
  name: "Research",
  description: "A shared group used across multiple spaces.",
  role: "viewer",
  member_count: 28,
  managed: false,
};

const draft: DraftSummaryRecord = {
  id: "draft-12345678-1234-1234-1234-123456789abc",
  document_id: "document-1",
  space_id: "space-1",
  owner_id: "user-1",
  owner_name: "Alex Example",
  document_title: "Published handbook",
  title: "Handbook revision draft",
  access_role: "owner",
  can_publish: true,
  created_at: "2026-09-30T12:00:00.000Z",
  updated_at: "2026-10-02T13:45:00.000Z",
};

describe("SpaceSettingsPage helpers", () => {
  it("stacks General settings as full-width divided rows instead of dashboard columns", () => {
    expect(SPACE_SETTINGS_GENERAL_LAYOUT_CLASS).toContain("flex-col");
    expect(SPACE_SETTINGS_GENERAL_LAYOUT_CLASS).toContain("divide-y");
    expect(SPACE_SETTINGS_GENERAL_LAYOUT_CLASS).not.toContain("grid-cols");
  });

  it("sorts local settings spaces alphabetically without mutating the source list", () => {
    const spaces: SpaceRecord[] = [
      { id: "space-b", name: "Zeta", slug: "zeta" },
      { id: "space-a", name: "Alpha", slug: "alpha" },
      { id: "space-c", name: "Archive", slug: "archive" },
    ];

    const sorted = sortSpacesForSettings(spaces);

    expect(sorted.map((space) => space.name)).toEqual(["Alpha", "Archive", "Zeta"]);
    expect(spaces.map((space) => space.name)).toEqual(["Zeta", "Alpha", "Archive"]);
  });

  it("builds the active-space settings route used by local switch/create actions", () => {
    expect(getSpaceSettingsRoute("space-1")).toBe("/spaces/space-1/settings");
    expect(getSpaceSettingsRoute("space/one")).toBe("/spaces/space%2Fone/settings");
  });

  it("marks the loaded local space as editing and other spaces as editable", () => {
    expect(getLocalSpaceEditState({ id: "space-1" }, "space-1")).toBe("editing");
    expect(getLocalSpaceEditState({ id: "space-2" }, "space-1")).toBe("edit");
    expect(getLocalSpaceEditState({ id: "space-2" }, null)).toBe("edit");
  });

  it("keeps the desktop access table and phone member list in separate responsive lanes", () => {
    expect(SPACE_USERS_DESKTOP_TABLE_CLASS).toContain("hidden");
    expect(SPACE_USERS_DESKTOP_TABLE_CLASS).toContain("md:block");
    expect(SPACE_USERS_MOBILE_LIST_CLASS).toContain("md:hidden");
    expect(SPACE_USERS_MOBILE_LIST_CLASS).not.toContain("overflow-x-auto");
  });

  it("keeps the desktop groups table and phone group list in separate responsive lanes", () => {
    expect(SPACE_GROUPS_DESKTOP_TABLE_CLASS).toContain("hidden");
    expect(SPACE_GROUPS_DESKTOP_TABLE_CLASS).toContain("md:block");
    expect(SPACE_GROUPS_DESKTOP_TABLE_CLASS).toContain("overflow-x-auto");
    expect(SPACE_GROUPS_MOBILE_LIST_CLASS).toContain("md:hidden");
    expect(SPACE_GROUPS_MOBILE_LIST_CLASS).not.toContain("overflow-x-auto");
  });

  it("renders managed and shared group details with their exact phone actions", () => {
    const renderGroup = (group: typeof managedGroup) =>
      renderToStaticMarkup(
        <SpaceGroupAccessMobileRow
          group={group}
          roleDefinitions={DEFAULT_SPACE_ROLE_DEFINITIONS}
          onOpenMembers={() => {}}
          onEditRole={() => {}}
          onRemove={() => {}}
        />,
      );

    const managedHtml = renderGroup(managedGroup);
    expect(managedHtml).toContain("Operations");
    expect(managedHtml).toContain("A managed group for space operators.");
    expect(managedHtml).toContain("14");
    expect(managedHtml).toContain("Editor");
    expect(managedHtml).toContain("Dedicated space group");
    expect(managedHtml).toContain("Members");
    expect(managedHtml).toContain("Edit role");
    expect(managedHtml).not.toContain('aria-label="Remove Operations"');
    expect(managedHtml.match(/min-h-11/g)?.length).toBe(2);

    const sharedHtml = renderGroup(sharedGroup);
    expect(sharedHtml).toContain("Research");
    expect(sharedHtml).toContain("Shared group");
    expect(sharedHtml).toContain("28");
    expect(sharedHtml).toContain("Viewer");
    expect(sharedHtml).not.toContain("Members</button>");
    expect(sharedHtml).toContain("Edit role");
    expect(sharedHtml).toContain("Remove");
    expect(sharedHtml.match(/min-h-11/g)?.length).toBe(2);
  });

  it("allows long phone group content to wrap without shrinking action targets", () => {
    const longGroup = {
      ...sharedGroup,
      name: "Research-and-Development-International-Shared-Group",
      description: "Detailed-access-policy-".repeat(12),
    };
    const html = renderToStaticMarkup(
      <SpaceGroupAccessMobileRow
        group={longGroup}
        roleDefinitions={DEFAULT_SPACE_ROLE_DEFINITIONS}
        onOpenMembers={() => {}}
        onEditRole={() => {}}
        onRemove={() => {}}
      />,
    );

    expect(html).toContain(longGroup.name);
    expect(html).toContain(longGroup.description);
    expect(html).toContain("break-words");
    expect(html.match(/min-h-11/g)?.length).toBe(2);
    expect(html).toContain('aria-label="Actions for Research-and-Development-International-Shared-Group"');
  });

  it("preserves managed/shared action predicates and group empty/search messages", () => {
    expect(getSpaceGroupAccessActionState(managedGroup)).toEqual({
      canOpenMembers: true,
      canRemove: false,
    });
    expect(getSpaceGroupAccessActionState(sharedGroup)).toEqual({
      canOpenMembers: false,
      canRemove: true,
    });
    expect(getSpaceGroupEmptyMessage(0)).toBe("No groups are assigned to this space.");
    expect(getSpaceGroupEmptyMessage(3)).toBe("No groups match this search.");
  });

  it("keeps the desktop drafts table and phone draft list in separate responsive lanes", () => {
    expect(SPACE_DRAFTS_DESKTOP_TABLE_CLASS).toContain("hidden");
    expect(SPACE_DRAFTS_DESKTOP_TABLE_CLASS).toContain("md:block");
    expect(SPACE_DRAFTS_DESKTOP_TABLE_CLASS).toContain("overflow-x-auto");
    expect(SPACE_DRAFTS_MOBILE_LIST_CLASS).toContain("md:hidden");
    expect(SPACE_DRAFTS_MOBILE_LIST_CLASS).not.toContain("overflow-x-auto");
  });

  it("preserves owner-only Share/Delete actions and viewer Open-only actions", () => {
    expect(getSpaceDraftActionState("owner")).toEqual({ canShare: true, canDelete: true });
    expect(getSpaceDraftActionState("editor")).toEqual({ canShare: false, canDelete: false });
    expect(getSpaceDraftActionState("viewer")).toEqual({ canShare: false, canDelete: false });

    const renderDraft = (record: DraftSummaryRecord) =>
      renderToStaticMarkup(
        <SpaceDraftMobileRow
          draft={record}
          onOpen={() => {}}
          onShare={() => {}}
          onDelete={() => {}}
        />,
      );
    const ownerHtml = renderDraft(draft);
    expect(ownerHtml).toContain("Open</button>");
    expect(ownerHtml).toContain("Share</button>");
    expect(ownerHtml).toContain("Delete</button>");
    expect(ownerHtml.match(/min-h-11/g)?.length).toBe(4);

    const viewerHtml = renderDraft({ ...draft, access_role: "viewer" });
    expect(viewerHtml).toContain("Open</button>");
    expect(viewerHtml).not.toContain("Share</button>");
    expect(viewerHtml).not.toContain("Delete</button>");
    expect(viewerHtml.match(/min-h-11/g)?.length).toBe(2);
  });

  it("shows complete draft context and a touch-safe full-ID copy action on phones", () => {
    const html = renderToStaticMarkup(
      <SpaceDraftMobileRow
        draft={draft}
        onOpen={() => {}}
        onShare={() => {}}
        onDelete={() => {}}
      />,
    );

    expect(html).toContain("Handbook revision draft");
    expect(html).toContain("Published page: Published handbook");
    expect(html).toContain("Alex Example");
    expect(html).toContain("owner");
    expect(html).toContain(new Date(draft.updated_at).toLocaleString());
    expect(html).toContain(draft.id);
    expect(html).toContain('aria-label="Copy draft ID"');
    expect(html).toContain("min-h-11 min-w-11");
    expect(html).toContain("break-all");
  });

  it("wraps long draft text and IDs and preserves empty versus filtered messages", () => {
    const longDraft = {
      ...draft,
      title: "Long-draft-title-".repeat(8),
      document_title: "Long-published-page-title-".repeat(8),
      id: "draft-" + "0123456789abcdef".repeat(8),
    };
    const html = renderToStaticMarkup(
      <SpaceDraftMobileRow
        draft={longDraft}
        onOpen={() => {}}
        onShare={() => {}}
        onDelete={() => {}}
      />,
    );
    expect(html).toContain(longDraft.title);
    expect(html).toContain(longDraft.document_title);
    expect(html).toContain(longDraft.id);
    expect(html).toContain("break-words");
    expect(html).toContain("break-all");
    expect(getSpaceDraftEmptyMessage("")).toBe("No drafts in this space.");
    expect(getSpaceDraftEmptyMessage("needle")).toBe("No drafts match this search.");
  });

  it("renders phone member identity, effective role, sources, override, and touch-safe actions", () => {
    const html = renderToStaticMarkup(
      <SpaceUserAccessMobileRow
        user={member}
        roleDefinitions={DEFAULT_SPACE_ROLE_DEFINITIONS}
        savingAccess={false}
        onEditDirectRole={() => {}}
        onRevokeDirectAccess={() => {}}
        onToggleBlock={() => {}}
      />,
    );

    expect(html).toContain("Alex Example");
    expect(html).toContain("alex@example.com");
    expect(html).toContain("Editor");
    expect(html).toContain("Access sources");
    expect(html).toContain("Direct");
    expect(html).toContain("Writers");
    expect(html).toContain("Override");
    expect(html).toContain("Edit role");
    expect(html).toContain("Revoke access");
    expect(html).toContain("Block");
    expect(html).toContain('aria-label="Actions for Alex Example"');
    expect(html.match(/min-h-11/g)?.length).toBeGreaterThanOrEqual(3);
  });

  it("preserves owner protections and inherited-member block actions on phones", () => {
    const owner = {
      ...member,
      direct_role: "owner",
      effective_role: "owner",
    };
    const inheritedBlockedMember = {
      ...member,
      direct_role: undefined,
      blocked: true,
      sources: [{ kind: "group" as const, id: "group-1", name: "Writers", role: "editor" }],
    };

    expect(getSpaceUserAccessActionState(owner)).toMatchObject({
      canEditDirectRole: false,
      canRevokeDirectAccess: false,
      canToggleBlock: false,
    });
    expect(getSpaceUserAccessActionState(inheritedBlockedMember)).toMatchObject({
      canEditDirectRole: false,
      canRevokeDirectAccess: false,
      canToggleBlock: true,
      blockActionLabel: "Unblock",
    });

    const html = renderToStaticMarkup(
      <SpaceUserAccessMobileRow
        user={inheritedBlockedMember}
        roleDefinitions={DEFAULT_SPACE_ROLE_DEFINITIONS}
        savingAccess
        onEditDirectRole={() => {}}
        onRevokeDirectAccess={() => {}}
        onToggleBlock={() => {}}
      />,
    );
    expect(html).toContain("Explicit deny");
    expect(html).toContain("Unblock");
    expect(html).toContain("disabled");
    expect(html).not.toContain("Edit role");
    expect(html).not.toContain("Revoke access");
  });

});
