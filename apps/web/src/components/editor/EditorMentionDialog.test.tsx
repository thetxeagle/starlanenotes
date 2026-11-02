import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { EditorMentionDialog, filterEditorMentionMembers } from "./EditorMentionDialog";

const members = [
  { user_id: "harley", display_name: "Harley Frank", email: "harley@example.test" },
  { user_id: "sam", display_name: "Sam Carter", email: "sam@example.test" },
];

describe("EditorMentionDialog", () => {
  it("filters members by name or email", () => {
    expect(filterEditorMentionMembers(members, "CARTER")).toEqual([members[1]]);
    expect(filterEditorMentionMembers(members, "harley@")).toEqual([members[0]]);
  });

  it("uses the shared accessible dialog shell and member choices", () => {
    const html = renderToStaticMarkup(
      <EditorMentionDialog
        members={members}
        query=""
        loading={false}
        error={false}
        onQueryChange={() => undefined}
        onSelect={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="editor-mention-title"');
    expect(html).toContain('aria-describedby="editor-mention-description"');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("Harley Frank");
    expect(html).toContain("sam@example.test");
    expect(html).toContain(">Cancel<");
  });
});
