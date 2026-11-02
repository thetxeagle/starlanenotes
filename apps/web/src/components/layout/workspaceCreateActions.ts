import { createContext, useContext } from "react";
import {
  OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT,
  OPEN_SIDEBAR_CREATE_FROM_TEMPLATE_EVENT,
  OPEN_SIDEBAR_CREATE_FOLDER_EVENT,
  OPEN_SIDEBAR_CREATE_SPACE_EVENT,
  OPEN_SIDEBAR_DAILY_NOTE_EVENT,
} from "./Sidebar";

export type WorkspaceCreateAction = "document" | "folder" | "space";
export type WorkspaceSidebarAction = WorkspaceCreateAction | "document-template" | "daily-note";

export function getWorkspaceSidebarEvent(action: WorkspaceSidebarAction): string {
  if (action === "document") return OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT;
  if (action === "document-template") return OPEN_SIDEBAR_CREATE_FROM_TEMPLATE_EVENT;
  if (action === "folder") return OPEN_SIDEBAR_CREATE_FOLDER_EVENT;
  if (action === "daily-note") return OPEN_SIDEBAR_DAILY_NOTE_EVENT;
  return OPEN_SIDEBAR_CREATE_SPACE_EVENT;
}

export function getWorkspaceCreateEvent(action: WorkspaceCreateAction): string {
  return getWorkspaceSidebarEvent(action);
}

/**
 * Desktop uses the mounted Files owner directly. Shell providers replace this
 * fallback on touch layouts so the drawer is mounted before a dialog opens.
 */
export function dispatchWorkspaceSidebarAction(action: WorkspaceSidebarAction): void {
  if (typeof document === "undefined") return;
  document.dispatchEvent(new CustomEvent(getWorkspaceSidebarEvent(action)));
}

export const WorkspaceCreateActionContext = createContext<(action: WorkspaceSidebarAction) => void>(
  dispatchWorkspaceSidebarAction,
);

export function useWorkspaceCreateAction(): (action: WorkspaceSidebarAction) => void {
  return useContext(WorkspaceCreateActionContext);
}
