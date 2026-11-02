import * as React from "react";
import * as PopoverPrimitive from "@radix-ui/react-popover";

import { cn } from "@/lib/utils";

const Popover = PopoverPrimitive.Root;

const PopoverTrigger = PopoverPrimitive.Trigger;

const PopoverAnchor = PopoverPrimitive.Anchor;

export const POPOVER_VIEWPORT_CONTAINMENT_CLASS =
  "max-h-[var(--radix-popover-content-available-height)] overflow-y-auto overscroll-contain";

const PopoverContent = React.forwardRef<
  React.ElementRef<typeof PopoverPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof PopoverPrimitive.Content>
>(({ className, align = "start", side = "bottom", sideOffset = 4, collisionPadding = 8, ...props }, ref) => (
  <PopoverPrimitive.Portal>
    <PopoverPrimitive.Content
      ref={ref}
      align={align}
      side={side}
      sideOffset={sideOffset}
      collisionPadding={collisionPadding}
      avoidCollisions
      className={cn(
        "rounded-input border-border bg-surface-raised text-text shadow-card z-[70] w-72 border p-1 outline-none transition-opacity duration-100 data-[state=closed]:opacity-0 data-[state=open]:opacity-100",
        POPOVER_VIEWPORT_CONTAINMENT_CLASS,
        className,
      )}
      {...props}
    />
  </PopoverPrimitive.Portal>
));
PopoverContent.displayName = PopoverPrimitive.Content.displayName;

export { Popover, PopoverTrigger, PopoverContent, PopoverAnchor };
