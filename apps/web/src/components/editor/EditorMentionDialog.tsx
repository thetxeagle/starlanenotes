import { useRef } from "react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { RESPONSIVE_MENU_OPTION_HEIGHT_CLASS } from "@/components/ui/form-control";
import { Input } from "@/components/ui/input";
import { useListboxKeyboardNavigation } from "@/components/ui/useListboxKeyboardNavigation";

export interface EditorMentionMember {
  user_id: string;
  display_name: string;
  email: string;
}

export function filterEditorMentionMembers(
  members: readonly EditorMentionMember[],
  query: string,
): EditorMentionMember[] {
  const normalizedQuery = query.trim().toLowerCase();
  return members.filter((member) =>
    `${member.display_name} ${member.email}`.toLowerCase().includes(normalizedQuery),
  );
}

interface EditorMentionDialogProps {
  members: readonly EditorMentionMember[];
  query: string;
  loading: boolean;
  error: boolean;
  onQueryChange: (query: string) => void;
  onSelect: (member: EditorMentionMember) => void;
  onCancel: () => void;
  description?: string;
}

export function EditorMentionDialog({
  members,
  query,
  loading,
  error,
  onQueryChange,
  onSelect,
  onCancel,
  description = "Add a portable mention for someone in this space.",
}: EditorMentionDialogProps) {
  const filteredMembers = filterEditorMentionMembers(members, query);
  const listboxRef = useRef<HTMLDivElement | null>(null);
  const handleListboxKeyDown = useListboxKeyboardNavigation(listboxRef);

  return (
    <DialogShell
      labelledBy="editor-mention-title"
      describedBy="editor-mention-description"
      maxWidth="sm"
      onClose={onCancel}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="editor-mention-title">Mention a member</DialogTitle>
          <DialogDescription id="editor-mention-description">{description}</DialogDescription>
        </div>
      </DialogHeader>
      <DialogBody>
        <label htmlFor="editor-mention-search" className="text-text block text-sm font-medium">
          Search members
        </label>
        <Input
          id="editor-mention-search"
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          className="bg-surface mt-1 h-8 text-sm"
          placeholder="Search members"
          autoFocus
          autoComplete="off"
        />

        <div
          ref={listboxRef}
          role="listbox"
          aria-label="Members"
          onKeyDown={handleListboxKeyDown}
          className="rounded-input border-border mt-3 max-h-56 overflow-y-auto border p-1"
        >
          {loading ? (
            <p className="text-text-muted px-2 py-2 text-sm">Loading members…</p>
          ) : error ? (
            <p className="text-danger px-2 py-2 text-sm">Unable to load members.</p>
          ) : filteredMembers.length ? (
            filteredMembers.map((member, index) => (
              <button
                key={member.user_id}
                type="button"
                role="option"
                aria-selected="false"
                tabIndex={index === 0 ? 0 : -1}
                className={`rounded-button text-text hover:bg-surface-raised hover:text-primary flex w-full flex-col items-start px-2 py-2 text-left text-sm transition-colors duration-100 ${RESPONSIVE_MENU_OPTION_HEIGHT_CLASS}`}
                onClick={() => onSelect(member)}
              >
                <span>{member.display_name}</span>
                <span className="text-text-muted mt-0.5 text-xs">{member.email}</span>
              </button>
            ))
          ) : (
            <p className="text-text-muted px-2 py-2 text-sm">No members match this search.</p>
          )}
        </div>
      </DialogBody>
      <DialogFooter>
        <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
