import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Textarea } from "@/components/ui/textarea";

export function canSubmitTemplateGuidance(content: string): boolean {
  return content.trim().length > 0;
}

export function EditorTemplateGuidanceDialog({
  content,
  onChange,
  onCancel,
  onSubmit,
}: {
  content: string;
  onChange: (content: string) => void;
  onCancel: () => void;
  onSubmit: (content: string) => boolean | void;
}) {
  const [error, setError] = useState("");
  const canSubmit = canSubmitTemplateGuidance(content);

  return (
    <DialogShell
      labelledBy="editor-template-guidance-title"
      describedBy="editor-template-guidance-description"
      maxWidth="md"
      onClose={onCancel}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (canSubmit && onSubmit(content.trim()) === false) {
            setError("The editor changed while this dialog was open. Close it and try again.");
          }
        }}
      >
        <DialogHeader>
          <div>
            <DialogTitle id="editor-template-guidance-title">Writing instruction</DialogTitle>
            <DialogDescription id="editor-template-guidance-description">
              Add a note that will guide anyone using this template.
            </DialogDescription>
          </div>
        </DialogHeader>
        <DialogBody>
          <label htmlFor="editor-template-guidance-content" className="block text-sm font-medium text-text">
            Instruction
          </label>
          <Textarea
            id="editor-template-guidance-content"
            value={content}
            onChange={(event) => onChange(event.target.value)}
            className="mt-1 min-h-28"
            placeholder="Describe what should be written in this section"
            autoFocus
            aria-describedby="editor-template-guidance-description"
          />
          {error && <p role="alert" className="mt-2 text-sm text-danger">{error}</p>}
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSubmit}>
            Insert instruction
          </Button>
        </DialogFooter>
      </form>
    </DialogShell>
  );
}
