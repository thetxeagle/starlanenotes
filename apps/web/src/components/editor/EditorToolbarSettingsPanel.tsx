import { useState } from "react";
import { ArrowDown, ArrowUp, GripVertical, Plus, RotateCcw, X } from "lucide-react";
import {
  DEFAULT_MOBILE_TOOLBAR_ACTIONS,
  getAvailableMobileToolbarActionIds,
  getInitialMobileToolbarActions,
  getMobileToolbarAction,
  persistMobileToolbarActions,
  type MobileToolbarActionId,
} from "@/components/editor/Editor";
import { Button } from "@/components/ui/button";

function moveAction(
  actions: MobileToolbarActionId[],
  action: MobileToolbarActionId,
  direction: "up" | "down",
): MobileToolbarActionId[] {
  const index = actions.indexOf(action);
  const target = direction === "up" ? index - 1 : index + 1;
  if (index < 0 || target < 0 || target >= actions.length) return actions;
  const next = [...actions];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}

/** Account Settings owner for the editor action order removed from the editor rail. */
export function EditorToolbarSettingsPanel() {
  const [actions, setActions] = useState<MobileToolbarActionId[]>(getInitialMobileToolbarActions);

  const commit = (next: MobileToolbarActionId[]) => {
    setActions(next);
    persistMobileToolbarActions(next);
  };

  return (
    <div className="grid gap-4">
      <div>
        <h2 className="text-text text-heading-lg font-semibold">Editor toolbar</h2>
        <p className="text-text-muted mt-1 text-sm">
          Choose and order the quick actions shown while editing. Changes apply to open editors.
        </p>
      </div>
      <div className="border-border bg-surface rounded-card border">
        <div className="border-border flex items-center justify-between gap-3 border-b px-4 py-3">
          <div>
            <h3 className="text-text text-sm font-semibold">Current actions</h3>
            <p className="text-text-muted text-xs">The selected order is preserved across open editors.</p>
          </div>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => commit([...DEFAULT_MOBILE_TOOLBAR_ACTIONS])}
          >
            <RotateCcw size={14} />
            Reset
          </Button>
        </div>
        <div className="grid gap-1 p-2">
          {actions.map((action, index) => {
            const meta = getMobileToolbarAction(action, null);
            return (
              <div
                key={action}
                className="rounded-button flex items-center gap-2 px-2 py-2 text-sm text-text-muted hover:bg-surface-raised hover:text-text"
              >
                <GripVertical size={14} aria-hidden="true" />
                <span className="flex size-7 items-center justify-center">{meta.icon}</span>
                <span className="min-w-0 flex-1 truncate">{meta.label}</span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="size-7 p-0"
                  aria-label={`Move ${meta.label} up`}
                  disabled={index === 0}
                  onClick={() => commit(moveAction(actions, action, "up"))}
                >
                  <ArrowUp size={13} />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="size-7 p-0"
                  aria-label={`Move ${meta.label} down`}
                  disabled={index === actions.length - 1}
                  onClick={() => commit(moveAction(actions, action, "down"))}
                >
                  <ArrowDown size={13} />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="size-7 p-0"
                  aria-label={`Remove ${meta.label}`}
                  onClick={() => commit(actions.filter((current) => current !== action))}
                >
                  <X size={13} />
                </Button>
              </div>
            );
          })}
        </div>
      </div>
      {getAvailableMobileToolbarActionIds(actions).length > 0 && (
        <div className="border-border bg-surface rounded-card border p-4">
          <h3 className="text-text text-sm font-semibold">Available actions</h3>
          <div className="mt-2 grid gap-1">
            {getAvailableMobileToolbarActionIds(actions).map((action) => {
              const meta = getMobileToolbarAction(action, null);
              return (
                <Button
                  key={action}
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-auto justify-start gap-3 px-2 py-2 text-left"
                  onClick={() => commit([...actions, action])}
                >
                  <Plus size={14} />
                  <span className="flex size-7 items-center justify-center">{meta.icon}</span>
                  <span>{meta.label}</span>
                </Button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
