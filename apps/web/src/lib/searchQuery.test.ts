import { describe, expect, it } from "vitest";
import { formatSearchQueryInput, parseSearchQueryInput } from "./searchQuery";

describe("searchQuery", () => {
  it("round trips exact responsibility IDs without reinterpreting author or Owner properties", () => {
    const input =
      "policy maintenance-user:Person-A maintenance-group:Engineering-A user:Author-A property:Owner=Legacy";
    const parsed = parseSearchQueryInput(input);
    expect(parsed.filters).toEqual({
      maintenance_user: "Person-A",
      maintenance_group: "Engineering-A",
      user: "Author-A",
      property_name: "Owner",
      property_value: "Legacy",
    });
    expect(parsed.query).toBe("policy");
    expect(formatSearchQueryInput(parsed.query, parsed.filters)).toBe(input);
    expect(parseSearchQueryInput("maintenance-user: maintenance-group:").filters).toEqual({});
  });
  it("round trips canonical identifiers separately from existing custom properties", () => {
    const input = "handoff identifier:Ops-17 property:Identifier=Legacy-9";
    const parsed = parseSearchQueryInput(input);
    expect(parsed).toEqual({
      query: "handoff",
      filters: { identifier: "Ops-17", property_name: "Identifier", property_value: "Legacy-9" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput(parsed.query, parsed.filters)).toBe(input);
    expect(parseSearchQueryInput("identifier:").filters).toEqual({});
  });
  it("extracts tag and space tokens from query text", () => {
    expect(
      parseSearchQueryInput('roadmap tag:Planning space:space-1 folder:Project "launch notes"'),
    ).toEqual({
      query: "roadmap launch notes",
      filters: {
        tag: "Planning",
        space_id: "space-1",
        folder: "Project",
      },
      starredOnly: false,
    });
  });

  it("accepts folder_id as an API-compatible alias for the folder filter", () => {
    expect(parseSearchQueryInput("handoff folder_id:folder-123")).toEqual({
      query: "handoff",
      filters: {
        folder: "folder-123",
      },
      starredOnly: false,
    });
  });

  it("extracts and formats user filters with author as a typed alias", () => {
    expect(parseSearchQueryInput('handoff user:"Harley Frank"')).toEqual({
      query: "handoff",
      filters: { user: "Harley Frank" },
      starredOnly: false,
    });
    expect(parseSearchQueryInput("author:user-123").filters).toEqual({ user: "user-123" });
    expect(formatSearchQueryInput("handoff", { user: "Harley Frank" })).toBe(
      'handoff user:"Harley Frank"',
    );
  });

  it("extracts property tokens with equals or colon separators", () => {
    expect(parseSearchQueryInput("property:Status=Open")).toEqual({
      query: "",
      filters: {
        property_name: "Status",
        property_value: "Open",
      },
      starredOnly: false,
    });

    expect(parseSearchQueryInput("property.Priority:High blockers")).toEqual({
      query: "blockers",
      filters: {
        property_name: "Priority",
        property_value: "High",
      },
      starredOnly: false,
    });
  });

  it("extracts title tokens for local and server search", () => {
    expect(parseSearchQueryInput('handoff title:"Launch Plan"')).toEqual({
      query: "handoff",
      filters: {
        title: "Launch Plan",
      },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("handoff", { title: "Launch Plan" })).toBe(
      'handoff title:"Launch Plan"',
    );
  });

  it("ignores empty filter tokens", () => {
    expect(parseSearchQueryInput("tag: space: property:")).toEqual({
      query: "",
      filters: {},
      starredOnly: false,
    });
  });

  it("extracts the SNQL starred directive without sending it to document search", () => {
    expect(parseSearchQueryInput("release notes is:starred tag:planning")).toEqual({
      query: "release notes",
      filters: { tag: "planning" },
      starredOnly: true,
    });
  });

  it("extracts and formats the private draft directive", () => {
    expect(parseSearchQueryInput("launch is:draft")).toEqual({
      query: "launch",
      filters: { is_draft: "true" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("launch", { is_draft: "true" })).toBe("launch is:draft");
  });

  it("extracts the attachment type directive for local and server search", () => {
    expect(parseSearchQueryInput("handoff has:attachment")).toEqual({
      query: "handoff",
      filters: { has_attachment: "true" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("handoff", { has_attachment: "true" })).toBe(
      "handoff has:attachment",
    );
  });

  it("extracts the cover type directive for local and server search", () => {
    expect(parseSearchQueryInput("handoff has:cover")).toEqual({
      query: "handoff",
      filters: { has_cover: "true" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("handoff", { has_cover: "true" })).toBe("handoff has:cover");
  });

  it("extracts the workflow type directive for local and server search", () => {
    expect(parseSearchQueryInput("handoff has:workflow")).toEqual({
      query: "handoff",
      filters: { has_workflow: "true" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("handoff", { has_workflow: "true" })).toBe(
      "handoff has:workflow",
    );
  });

  it("extracts tag and property presence directives for local and server search", () => {
    expect(parseSearchQueryInput("handoff has:tag has:properties")).toEqual({
      query: "handoff",
      filters: { has_tag: "true", has_property: "true" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("handoff", { has_tag: "true", has_property: "true" })).toBe(
      "handoff has:tag has:property",
    );
  });

  it("extracts and formats negative presence directives for local and server search", () => {
    expect(
      parseSearchQueryInput(
        "handoff has:noattachment has:nocover has:noworkflow has:notag has:noproperties",
      ),
    ).toEqual({
      query: "handoff",
      filters: {
        has_attachment: "false",
        has_cover: "false",
        has_workflow: "false",
        has_tag: "false",
        has_property: "false",
      },
      starredOnly: false,
    });
    expect(
      formatSearchQueryInput("handoff", {
        has_attachment: "false",
        has_cover: "false",
        has_workflow: "false",
        has_tag: "false",
        has_property: "false",
      }),
    ).toBe("handoff has:noattachment has:nocover has:noworkflow has:notag has:noproperty");
    expect(parseSearchQueryInput("attachment:false cover:false").filters).toEqual({
      has_attachment: "false",
      has_cover: "false",
    });
    expect(
      parseSearchQueryInput("workflow:false tag:false property:false properties:false").filters,
    ).toEqual({
      has_workflow: "false",
      has_tag: "false",
      has_property: "false",
    });
  });

  it("extracts the locked-state directive for local and server search", () => {
    expect(parseSearchQueryInput("handoff is:locked")).toEqual({
      query: "handoff",
      filters: { is_locked: "true" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("handoff", { is_locked: "true" })).toBe("handoff is:locked");
    expect(parseSearchQueryInput("handoff is:unlocked")).toEqual({
      query: "handoff",
      filters: { is_locked: "false" },
      starredOnly: false,
    });
    expect(formatSearchQueryInput("handoff", { is_locked: "false" })).toBe("handoff is:unlocked");
  });

  it("extracts metadata date directives for local and server search", () => {
    expect(parseSearchQueryInput("handoff created:2026-07-14 updated:2026-07-15")).toEqual({
      query: "handoff",
      filters: {
        created_on: "2026-07-14",
        updated_on: "2026-07-15",
      },
      starredOnly: false,
    });
    expect(
      formatSearchQueryInput("handoff", {
        created_on: "2026-07-14",
        updated_on: "2026-07-15",
      }),
    ).toBe("handoff created:2026-07-14 updated:2026-07-15");
  });

  it("accepts metadata date aliases for typed-query and API-field compatibility", () => {
    expect(parseSearchQueryInput("handoff created_on:2026-07-14 updated_on:2026-07-15")).toEqual({
      query: "handoff",
      filters: {
        created_on: "2026-07-14",
        updated_on: "2026-07-15",
      },
      starredOnly: false,
    });

    expect(parseSearchQueryInput("handoff modified:2026-07-16")).toEqual({
      query: "handoff",
      filters: {
        updated_on: "2026-07-16",
      },
      starredOnly: false,
    });
  });

  it("does not treat malformed metadata date directives as full SNQL filters", () => {
    expect(
      parseSearchQueryInput("created:yesterday updated:2026-7-5 modified:today alpha"),
    ).toEqual({
      query: "alpha",
      filters: {},
      starredOnly: false,
    });
  });

  it("formats legacy saved filters as visible SNQL terms", () => {
    const input = formatSearchQueryInput(
      "launch notes",
      {
        tag: "Product Work",
        folder: "Project Plans",
        property_name: "Review Status",
        property_value: "In progress",
      },
      true,
    );

    expect(input).toBe(
      'launch notes tag:"Product Work" folder:"Project Plans" property:"Review Status=In progress" is:starred',
    );
    expect(parseSearchQueryInput(input)).toEqual({
      query: "launch notes",
      filters: {
        tag: "Product Work",
        folder: "Project Plans",
        property_name: "Review Status",
        property_value: "In progress",
      },
      starredOnly: true,
    });
  });

  it("formats newer saved filters as visible bounded SNQL terms", () => {
    const input = formatSearchQueryInput(
      "release",
      {
        title: "Launch Plan",
        has_attachment: "false",
        has_cover: "true",
        is_locked: "false",
        created_on: "2026-07-14",
        updated_on: "2026-07-15",
      },
      false,
    );

    expect(input).toBe(
      'release title:"Launch Plan" has:noattachment has:cover is:unlocked created:2026-07-14 updated:2026-07-15',
    );
    expect(parseSearchQueryInput(input)).toEqual({
      query: "release",
      filters: {
        title: "Launch Plan",
        has_attachment: "false",
        has_cover: "true",
        is_locked: "false",
        created_on: "2026-07-14",
        updated_on: "2026-07-15",
      },
      starredOnly: false,
    });
  });
});
