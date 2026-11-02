import { useId, useState } from "react";
import { Users } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { CONTROL_CHROME_ACTION_CLASS } from "@/components/ui/control-chrome";

export interface DraftPresencePeer {
  user_id: string;
  connection_id: string;
  display_name: string;
  state: "active" | "idle";
  mode: "view" | "edit";
}
export interface DraftPresenceProps {
  /** Complete authorized roster, including this connection. */
  peers: readonly DraftPresencePeer[];
  currentUserId: string;
  ownConnectionId: string | null;
  status: "connecting" | "ready" | "reconnecting" | "unavailable";
}
export interface DraftPresenceGroup {
  userId: string;
  displayName: string;
  isCurrentUser: boolean;
  sessions: DraftPresencePeer[];
}
/** Derive people from trusted connection identities without treating tabs as users. */
export function groupDraftPresencePeers(
  peers: readonly DraftPresencePeer[],
  currentUserId: string,
  ownConnectionId: string | null,
): DraftPresenceGroup[] {
  const groups = new Map<string, DraftPresenceGroup>();
  for (const peer of peers) {
    let group = groups.get(peer.user_id);
    if (!group) {
      group = {
        userId: peer.user_id,
        displayName: peer.display_name.trim() ? peer.display_name : "Participant",
        isCurrentUser: peer.user_id === currentUserId,
        sessions: [],
      };
      groups.set(peer.user_id, group);
    }
    group.sessions.push(peer);
  }
  const result = [...groups.values()];
  for (const group of result) {
    group.sessions.sort(
      (a, b) =>
        Number(b.connection_id === ownConnectionId) - Number(a.connection_id === ownConnectionId) ||
        a.connection_id.localeCompare(b.connection_id),
    );
  }
  return result.sort(
    (a, b) =>
      Number(b.isCurrentUser) - Number(a.isCurrentUser) ||
      a.displayName.localeCompare(b.displayName) ||
      a.userId.localeCompare(b.userId),
  );
}
function rosterReady({ peers, currentUserId, ownConnectionId, status }: DraftPresenceProps) {
  return (
    status === "ready" &&
    !!ownConnectionId &&
    peers.length > 0 &&
    peers.length <= 64 &&
    new Set(peers.map((peer) => peer.connection_id)).size === peers.length &&
    peers.some((peer) => peer.connection_id === ownConnectionId && peer.user_id === currentUserId)
  );
}
function statusText(props: DraftPresenceProps) {
  if (props.status === "connecting") return "Checking draft participants";
  if (props.status === "reconnecting") return "Reconnecting draft presence";
  return "Draft presence unavailable";
}
function summary(props: DraftPresenceProps, groups: DraftPresenceGroup[]) {
  if (!rosterReady(props)) return statusText(props);
  if (groups.length === 1)
    return props.peers.length === 1
      ? "Only you in this draft"
      : "You · " + props.peers.length + " sessions";
  return (
    groups.length +
    " participants" +
    (props.peers.length > groups.length ? " · " + props.peers.length + " sessions" : "")
  );
}
/** Read-only roster body. No access editing or save state. */
export function DraftPresenceRoster(props: DraftPresenceProps) {
  if (!rosterReady(props)) {
    return (
      <p className="text-text-muted text-sm">
        {statusText(props)}. Participant information will appear after this selected draft is
        verified.
      </p>
    );
  }
  const groups = groupDraftPresencePeers(props.peers, props.currentUserId, props.ownConnectionId);
  return (
    <ul aria-label="Draft participants" className="divide-border divide-y">
      {groups.map((group) => (
        <li key={group.userId} className="min-w-0 py-3 first:pt-0 last:pb-0">
          <p className="text-text text-sm font-medium [overflow-wrap:anywhere] break-words">
            {group.isCurrentUser ? "You" : group.displayName}
          </p>
          <p className="text-text-muted mt-1 text-xs">
            {group.sessions.length} {group.sessions.length === 1 ? "session" : "sessions"}
          </p>
          <ul
            aria-label={(group.isCurrentUser ? "Your" : group.displayName) + " sessions"}
            className="mt-2 space-y-2"
          >
            {group.sessions.map((peer, index) => (
              <li
                key={peer.connection_id}
                className="text-text-muted text-xs [overflow-wrap:anywhere] break-words"
              >
                <span>
                  {peer.connection_id === props.ownConnectionId
                    ? "This session"
                    : group.isCurrentUser
                      ? "Your other session" + (group.sessions.length > 2 ? " " + index : "")
                      : "Session " + (index + 1)}
                </span>
                {" · "}
                <span>{peer.state === "active" ? "Active" : "Idle"}</span>
                {" · "}
                <span>{peer.mode === "edit" ? "Edit access" : "View access"}</span>
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  );
}
function DraftPresenceControl(props: DraftPresenceProps) {
  const [open, setOpen] = useState(false);
  const heading = useId(),
    description = useId();
  const ready = rosterReady(props);
  const groups = ready
    ? groupDraftPresencePeers(props.peers, props.currentUserId, props.ownConnectionId)
    : [];
  const label = summary(props, groups);
  return (
    <Popover open={open && ready} onOpenChange={setOpen} modal>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          disabled={!ready}
          className={CONTROL_CHROME_ACTION_CLASS + " max-w-full"}
          aria-label={label + ". Draft participants"}
        >
          <Users size={16} aria-hidden="true" className="shrink-0" />
          <span className="min-w-0 text-left whitespace-normal" aria-live="polite">
            {label}
          </span>
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        aria-labelledby={heading}
        aria-describedby={description}
        className="w-80 max-w-[calc(100vw-1rem)] p-0"
      >
        <div className="border-border border-b px-4 py-3">
          <h2 id={heading} className="text-text text-sm font-semibold">
            In this draft
          </h2>
          <p id={description} className="text-text-muted mt-1 text-xs">
            Current participants in this selected draft. This does not show the published page’s
            audience or grant access.
          </p>
        </div>
        <div className="p-4">
          <DraftPresenceRoster {...props} />
        </div>
        <div className="border-border flex justify-end border-t px-4 py-3">
          <Button variant="secondary" size="sm" onClick={() => setOpen(false)}>
            Close participants
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
export function DraftPresence(props: DraftPresenceProps) {
  // Another connection/account/status owns a new overlay lifecycle. Old open
  // overlays cannot silently reopen when another selection becomes ready.
  return (
    <DraftPresenceControl
      key={props.currentUserId + "/" + (props.ownConnectionId ?? "") + "/" + props.status}
      {...props}
    />
  );
}
