import { Select } from "@/components/ui/select";
import { type TablePlacement } from "./blockPlacement";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogCloseAction,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { TABLE_STYLE_PRESETS, type TableStyleId } from "@/components/editor/tableStyles";

interface TableStyleDialogProps {
  initialStyle: TableStyleId;
  initialPlacement?: TablePlacement;
  onApply: (style: TableStyleId, placement: TablePlacement) => void;
  onClose: () => void;
}

export function TableStyleDialog({
  initialStyle,
  initialPlacement = "center",
  onApply,
  onClose,
}: TableStyleDialogProps) {
  const [placement, setPlacement] = useState<TablePlacement>(initialPlacement);
  const [selectedStyle, setSelectedStyle] = useState(initialStyle);

  return (
    <DialogShell
      labelledBy="table-style-dialog-title"
      describedBy="table-style-dialog-description"
      maxWidth="lg"
      onClose={onClose}
    >
      <DialogHeader>
        <div className="min-w-0">
          <DialogTitle id="table-style-dialog-title">Table style</DialogTitle>
          <DialogDescription id="table-style-dialog-description">
            Choose a presentation that fits this table&apos;s content.
          </DialogDescription>
        </div>
        <DialogCloseAction label="Close table style" onClick={onClose} />
      </DialogHeader>

      <DialogBody>
        <label className="text-text mb-4 block text-sm">
          Table placement
          <Select
            aria-label="Table placement"
            value={placement}
            onChange={(event) => setPlacement(event.target.value as TablePlacement)}
            className="mt-2"
          >
            <option value="left">Left</option>
            <option value="center">Center</option>
            <option value="right">Right</option>
          </Select>
        </label>
        <ToggleGroup
          type="single"
          value={selectedStyle}
          onValueChange={(value) => {
            if (value) setSelectedStyle(value as TableStyleId);
          }}
          className="grid grid-cols-1 items-stretch justify-stretch gap-3 sm:grid-cols-2"
          aria-label="Table style options"
        >
          {TABLE_STYLE_PRESETS.map((preset) => (
            <ToggleGroupItem
              key={preset.id}
              value={preset.id}
              aria-label={preset.label}
              className="border-border bg-surface hover:bg-surface-raised data-[state=on]:border-primary data-[state=on]:bg-primary/5 focus-ring h-auto min-h-32 w-full flex-col items-stretch justify-start gap-3 border p-3 text-left"
            >
              <span
                aria-hidden="true"
                className={`table-style-swatch table-style-swatch-${preset.id}`}
              >
                <span className="table-style-swatch-row table-style-swatch-header">
                  <span />
                  <span />
                  <span />
                </span>
                <span className="table-style-swatch-row">
                  <span />
                  <span />
                  <span />
                </span>
                <span className="table-style-swatch-row">
                  <span />
                  <span />
                  <span />
                </span>
              </span>
              <span>
                <span className="text-text block text-sm font-medium">{preset.label}</span>
                <span className="text-text-muted mt-1 block text-xs leading-5">
                  {preset.description}
                </span>
              </span>
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </DialogBody>

      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onClose}>
          Cancel
        </Button>
        <Button type="button" onClick={() => onApply(selectedStyle, placement)}>
          Apply style
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
