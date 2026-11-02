import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  DraftPresence,
  DraftPresenceRoster,
  groupDraftPresencePeers,
  type DraftPresencePeer,
  type DraftPresenceProps,
} from "./DraftPresence";
const self: DraftPresencePeer = {
  user_id: "me",
  connection_id: "own",
  display_name: "Eagle",
  state: "active",
  mode: "edit",
};
const other: DraftPresencePeer = {
  user_id: "other",
  connection_id: "remote",
  display_name: "Simmons",
  state: "idle",
  mode: "view",
};
function props(
  peers: readonly DraftPresencePeer[] = [self],
  status: DraftPresenceProps["status"] = "ready",
): DraftPresenceProps {
  return { peers, currentUserId: "me", ownConnectionId: "own", status };
}
describe("authoritative selected draft presence presentation", () => {
  it("groups people separately from connections without modifying the roster", () => {
    const another = { ...self, connection_id: "another", state: "idle" as const };
    const peers = Object.freeze([
      Object.freeze(other),
      Object.freeze(another),
      Object.freeze(self),
    ]);
    const groups = groupDraftPresencePeers(peers, "me", "own");
    expect(groups.map((group) => group.userId)).toEqual(["me", "other"]);
    expect(groups[0].sessions.map((peer) => peer.connection_id)).toEqual(["own", "another"]);
    expect(peers.map((peer) => peer.connection_id)).toEqual(["remote", "another", "own"]);
    expect(renderToStaticMarkup(<DraftPresence {...props(peers)} />)).toContain(
      "2 participants · 3 sessions",
    );
  });
  it("names your other session instead of counting it as another person", () => {
    const peers = [
      self,
      { ...self, connection_id: "other-tab", state: "idle" as const, mode: "view" as const },
    ];
    expect(renderToStaticMarkup(<DraftPresence {...props(peers)} />)).toContain("You · 2 sessions");
    const html = renderToStaticMarkup(<DraftPresenceRoster {...props(peers)} />);
    for (const text of [
      "This session",
      "Your other session",
      "Active",
      "Idle",
      "Edit access",
      "View access",
    ])
      expect(html).toContain(text);
    expect(html).not.toContain("2 participants");
  });
  it("claims only you only for a ready roster containing your own identity", () => {
    expect(renderToStaticMarkup(<DraftPresence {...props()} />)).toContain(
      "Only you in this draft",
    );
    for (const invalid of [
      props([], "ready"),
      { ...props(), ownConnectionId: null },
      props([{ ...self, user_id: "foreign" }]),
      props([self, self]),
    ]) {
      const html = renderToStaticMarkup(<DraftPresence {...invalid} />);
      expect(html).not.toContain("Only you");
      expect(html).toContain("disabled");
      expect(html).toContain("Draft presence unavailable");
    }
  });
  it("hides stale names and counts while connecting, reconnecting or unavailable", () => {
    for (const status of ["connecting", "reconnecting", "unavailable"] as const) {
      const value = props([self, other], status);
      const control = renderToStaticMarkup(<DraftPresence {...value} />);
      const roster = renderToStaticMarkup(<DraftPresenceRoster {...value} />);
      expect(control).not.toContain("Only you");
      expect(control).not.toContain("2 participants");
      expect(control).toContain("disabled");
      expect(roster).not.toContain("Simmons");
      expect(roster).toContain("selected draft is verified");
    }
  });
  it("renders full long names as text and distinguishes idle view access", () => {
    const name = "<script>unsafe()</script>" + " Long participant name ".repeat(24);
    const html = renderToStaticMarkup(
      <DraftPresenceRoster {...props([self, { ...other, display_name: name }])} />,
    );
    expect(html).toContain("&lt;script&gt;unsafe()&lt;/script&gt;");
    expect(html).not.toContain("<script>");
    expect(html).toContain(" Long participant name ".repeat(24));
    expect(html).toContain("View access");
    expect(html).toContain("Idle");
    expect(html).not.toContain("Editing");
  });
  it("supports 64 sessions without silently clipping a larger roster", () => {
    const peers = [
      self,
      ...Array.from({ length: 63 }, (_, n) => ({
        ...other,
        user_id: "u" + n,
        connection_id: "c" + n,
      })),
    ];
    expect(renderToStaticMarkup(<DraftPresence {...props(peers)} />)).toContain("64 participants");
    const tooMany = [...peers, { ...other, connection_id: "extra" }];
    expect(renderToStaticMarkup(<DraftPresence {...props(tooMany)} />)).toContain(
      "Draft presence unavailable",
    );
    expect(renderToStaticMarkup(<DraftPresenceRoster {...props(tooMany)} />)).not.toContain(
      "Simmons",
    );
  });
});
