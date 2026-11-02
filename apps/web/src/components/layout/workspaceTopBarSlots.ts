import { createContext, useContext } from "react";

export interface WorkspaceTopBarSlots {
  actions: HTMLElement | null;
}

const EMPTY_WORKSPACE_TOP_BAR_SLOTS: WorkspaceTopBarSlots = {
  actions: null,
};

export const WorkspaceTopBarSlotContext = createContext<WorkspaceTopBarSlots>(
  EMPTY_WORKSPACE_TOP_BAR_SLOTS,
);

export function useWorkspaceTopBarSlots(): WorkspaceTopBarSlots {
  return useContext(WorkspaceTopBarSlotContext);
}
