import { useRef, type ReactNode } from "react";
import { ArrowDown, ArrowUp, Check, GripVertical, Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DialogShell } from "@/components/ui/dialog-shell";

export interface EditorMobileToolbarCustomizeAction {
  id: string;
  label: string;
  icon: ReactNode;
}

interface EditorMobileToolbarCustomizeDialogProps {
  activeActions: EditorMobileToolbarCustomizeAction[];
  availableActions: EditorMobileToolbarCustomizeAction[];
  onClose: () => void;
  onMove: (actionId: string, direction: "up" | "down") => void;
  onRemove: (actionId: string) => void;
  onAdd: (actionId: string) => void;
  onReorder: (draggedActionId: string, targetActionId: string) => void;
}

export function EditorMobileToolbarCustomizeDialog({
  activeActions,
  availableActions,
  onClose,
  onMove,
  onRemove,
  onAdd,
  onReorder,
}: EditorMobileToolbarCustomizeDialogProps) {
  const draggedActionRef = useRef<string | null>(null);

  return (
    <DialogShell labelledBy="mobile-toolbar-customize-title" maxWidth="sm" onClose={onClose}>
      <div data-dialog-frame="" className="flex max-h-[min(32rem,calc(100dvh-2rem))] flex-col">
        <div className="flex items-center justify-between gap-3 border-b border-border px-3 py-2">
          <div>
            <h2 id="mobile-toolbar-customize-title" className="text-sm font-medium text-text">
              Toolbar
            </h2>
            <p className="text-xs text-text-muted">Choose quick actions</p>
          </div>
          <Button type="button" variant="secondary" size="sm" onClick={onClose}>
            <Check size={14} />
            <span className="ml-1">Done</span>
          </Button>
        </div>
        <div className="scrollbar-none min-h-0 overflow-y-auto p-2">
          <div className="px-2 pb-1 text-xs font-medium text-text-muted">
            Current actions
          </div>
          <div className="space-y-1">
            {activeActions.map((action, index) => (
              <div
                key={action.id}
                draggable
                onDragStart={() => {
                  draggedActionRef.current = action.id;
                }}
                onDragOver={(event) => event.preventDefault()}
                onDrop={() => {
                  if (draggedActionRef.current) onReorder(draggedActionRef.current, action.id);
                  draggedActionRef.current = null;
                }}
                onDragEnd={() => {
                  draggedActionRef.current = null;
                }}
                className="rounded-button flex w-full items-center gap-2 px-2 py-2 text-sm text-text-muted hover:bg-surface-raised hover:text-text"
              >
                <span className="flex h-7 w-7 cursor-grab items-center justify-center text-text-muted">
                  <GripVertical size={14} />
                </span>
                <span className="flex h-7 w-7 items-center justify-center">{action.icon}</span>
                <span className="min-w-0 flex-1 truncate">{action.label}</span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 w-7 p-0"
                  title="Move up"
                  aria-label={`Move ${action.label} up`}
                  disabled={index === 0}
                  onClick={() => onMove(action.id, "up")}
                >
                  <ArrowUp size={13} />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 w-7 p-0"
                  title="Move down"
                  aria-label={`Move ${action.label} down`}
                  disabled={index === activeActions.length - 1}
                  onClick={() => onMove(action.id, "down")}
                >
                  <ArrowDown size={13} />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 w-7 p-0"
                  title="Remove"
                  aria-label={`Remove ${action.label}`}
                  onClick={() => onRemove(action.id)}
                >
                  <X size={13} />
                </Button>
              </div>
            ))}
          </div>
          <div className="mt-3 px-2 pb-1 text-xs font-medium text-text-muted">
            Available actions
          </div>
          <div className="space-y-1">
            {availableActions.map((action) => (
              <Button
                key={action.id}
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => onAdd(action.id)}
                className="h-auto w-full justify-start gap-3 px-2 py-2 text-left text-sm"
              >
                <span className="flex h-5 w-5 items-center justify-center rounded-full border border-border">
                  <Plus size={12} />
                </span>
                <span className="flex h-7 w-7 items-center justify-center">{action.icon}</span>
                <span>{action.label}</span>
              </Button>
            ))}
          </div>
        </div>
      </div>
    </DialogShell>
  );
}
