import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useAuth } from "@/hooks/useAuth";
import { dataClient } from "@/lib/dataClient";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { buildSpaceHomePath } from "@/lib/lastViewedSpace";
import { getPersonalSpaceOwnerLabel } from "@/lib/spaceLabels";

export interface WorkspaceSpace {
  id: string;
  name: string;
  slug: string;
  icon?: string;
  accent_color?: string;
  member_role?: string;
  owner_username?: string;
  owner_display_name?: string;
  created_by?: string;
}

export interface WorkspaceSpaceSection {
  label: "Recent" | "Owned" | "All spaces" | "Search results";
  spaces: WorkspaceSpace[];
}

export const RECENT_WORKSPACE_SPACE_STORAGE_KEY = "starlane_recent_space_ids";
export const WORKSPACE_SPACE_SELECTOR_LIMIT = 25;
export const RECENT_WORKSPACE_SPACE_LIMIT = 5;
export const OWNED_WORKSPACE_SPACE_LIMIT = 10;

export function canCreateWorkspaceSpace(localMode: boolean, role?: string): boolean {
  return localMode || role === "admin";
}

export function isCurrentWorkspaceSpaceLoad(
  requestId: number,
  currentRequestId: number,
): boolean {
  return requestId === currentRequestId;
}

export function resolveActiveWorkspaceSpace(
  spaces: WorkspaceSpace[],
  routeSpaceId?: string,
  currentSpaceId?: string,
): WorkspaceSpace | null {
  if (spaces.length === 0) return null;
  return (
    spaces.find((space) => space.id === routeSpaceId) ??
    spaces.find((space) => space.id === currentSpaceId) ??
    spaces[0]
  );
}

export function updateRecentWorkspaceSpaceIds(
  recentSpaceIds: string[],
  spaceId: string,
): string[] {
  return [spaceId, ...recentSpaceIds.filter((id) => id !== spaceId)].slice(
    0,
    WORKSPACE_SPACE_SELECTOR_LIMIT,
  );
}

export function getWorkspaceSpaceSections(
  spaces: WorkspaceSpace[],
  recentSpaceIds: string[],
  query = "",
): WorkspaceSpaceSection[] {
  const normalizedQuery = query.trim().toLowerCase();
  if (normalizedQuery) {
    return [
      {
        label: "Search results",
        spaces: spaces
          .filter(
            (space) =>
              space.name.toLowerCase().includes(normalizedQuery) ||
              space.slug.toLowerCase().includes(normalizedQuery) ||
              (getPersonalSpaceOwnerLabel(space)?.toLowerCase().includes(normalizedQuery) ?? false),
          )
          .slice(0, WORKSPACE_SPACE_SELECTOR_LIMIT),
      },
    ];
  }

  const recentSpaceSet = new Set(recentSpaceIds);
  const recentSpaces = recentSpaceIds
    .map((id) => spaces.find((space) => space.id === id))
    .filter((space): space is WorkspaceSpace => Boolean(space))
    .slice(0, RECENT_WORKSPACE_SPACE_LIMIT);
  const ownedSpaces = spaces
    .filter((space) => space.member_role === "owner" && !recentSpaceSet.has(space.id))
    .slice(0, OWNED_WORKSPACE_SPACE_LIMIT);
  const pinnedSpaceIds = new Set([...recentSpaces, ...ownedSpaces].map((space) => space.id));
  const remainingSpaceLimit = Math.max(
    0,
    WORKSPACE_SPACE_SELECTOR_LIMIT - recentSpaces.length - ownedSpaces.length,
  );
  const remainingSpaces = spaces
    .filter((space) => !pinnedSpaceIds.has(space.id))
    .slice(0, remainingSpaceLimit);

  return [
    { label: "Recent", spaces: recentSpaces },
    { label: "Owned", spaces: ownedSpaces },
    { label: "All spaces", spaces: remainingSpaces },
  ].filter((section) => section.spaces.length > 0) as WorkspaceSpaceSection[];
}

function readRecentWorkspaceSpaceIds(): string[] {
  try {
    const stored = JSON.parse(
      window.localStorage.getItem(RECENT_WORKSPACE_SPACE_STORAGE_KEY) ?? "[]",
    );
    return Array.isArray(stored)
      ? stored.filter((id): id is string => typeof id === "string")
      : [];
  } catch {
    return [];
  }
}

function writeRecentWorkspaceSpaceIds(spaceIds: string[]): void {
  try {
    window.localStorage.setItem(RECENT_WORKSPACE_SPACE_STORAGE_KEY, JSON.stringify(spaceIds));
  } catch {
    // Recent space ordering is a convenience; storage failure must not block navigation.
  }
}

interface WorkspaceSpaceContextValue {
  spaces: WorkspaceSpace[];
  activeSpace: WorkspaceSpace | null;
  recentSpaceIds: string[];
  loading: boolean;
  canCreateSpace: boolean;
  selectSpace: (space: WorkspaceSpace) => void;
  createSpace: (name: string) => Promise<WorkspaceSpace>;
  refreshSpaces: (preferredSpaceId?: string) => Promise<WorkspaceSpace[]>;
}

const WorkspaceSpaceContext = createContext<WorkspaceSpaceContextValue | null>(null);

export function WorkspaceSpaceProvider({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const { spaceId: routeSpaceId } = useParams<{ spaceId: string }>();
  const { user } = useAuth();
  const localMode = getCurrentLaunchMode() === "local";
  const [spaces, setSpaces] = useState<WorkspaceSpace[]>([]);
  const [activeSpace, setActiveSpace] = useState<WorkspaceSpace | null>(null);
  const [recentSpaceIds, setRecentSpaceIds] = useState(readRecentWorkspaceSpaceIds);
  const [loading, setLoading] = useState(true);
  const requestIdRef = useRef(0);
  const routeSpaceIdRef = useRef(routeSpaceId);
  const activeSpaceIdRef = useRef<string | undefined>(undefined);
  routeSpaceIdRef.current = routeSpaceId;
  activeSpaceIdRef.current = activeSpace?.id;

  const rememberSpace = useCallback((spaceId: string) => {
    setRecentSpaceIds((current) => {
      const next = updateRecentWorkspaceSpaceIds(current, spaceId);
      writeRecentWorkspaceSpaceIds(next);
      return next;
    });
  }, []);

  const refreshSpaces = useCallback(async (preferredSpaceId?: string) => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    try {
      const nextSpaces = await dataClient.listSpaces();
      if (!isCurrentWorkspaceSpaceLoad(requestId, requestIdRef.current)) return nextSpaces;
      setSpaces(nextSpaces);
      setActiveSpace(
        resolveActiveWorkspaceSpace(
          nextSpaces,
          preferredSpaceId ?? routeSpaceIdRef.current,
          activeSpaceIdRef.current,
        ),
      );
      return nextSpaces;
    } finally {
      if (isCurrentWorkspaceSpaceLoad(requestId, requestIdRef.current)) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshSpaces().catch(() => undefined);
    return () => {
      requestIdRef.current += 1;
    };
  }, [refreshSpaces]);

  useEffect(() => {
    if (!routeSpaceId) return;
    const routeSpace = spaces.find((space) => space.id === routeSpaceId);
    if (routeSpace) setActiveSpace(routeSpace);
  }, [routeSpaceId, spaces]);

  useEffect(() => {
    if (activeSpace?.id) rememberSpace(activeSpace.id);
  }, [activeSpace?.id, rememberSpace]);

  const selectSpace = useCallback(
    (space: WorkspaceSpace) => {
      setActiveSpace(space);
      rememberSpace(space.id);
      navigate(buildSpaceHomePath(space.id));
    },
    [navigate, rememberSpace],
  );

  const createSpace = useCallback(
    async (name: string) => {
      if (!canCreateWorkspaceSpace(localMode, user?.role)) {
        throw new Error("You do not have permission to create a space.");
      }
      const normalizedName = name.trim();
      if (!normalizedName) throw new Error("Space name is required.");
      const created = await dataClient.createSpace(normalizedName);
      await refreshSpaces(created.id);
      setActiveSpace(created);
      rememberSpace(created.id);
      navigate(buildSpaceHomePath(created.id));
      return created;
    },
    [localMode, navigate, refreshSpaces, rememberSpace, user?.role],
  );

  return (
    <WorkspaceSpaceContext.Provider
      value={{
        spaces,
        activeSpace,
        recentSpaceIds,
        loading,
        canCreateSpace: canCreateWorkspaceSpace(localMode, user?.role),
        selectSpace,
        createSpace,
        refreshSpaces,
      }}
    >
      {children}
    </WorkspaceSpaceContext.Provider>
  );
}

export function useWorkspaceSpaces(): WorkspaceSpaceContextValue {
  const context = useContext(WorkspaceSpaceContext);
  if (!context) {
    throw new Error("useWorkspaceSpaces must be used within a WorkspaceSpaceProvider");
  }
  return context;
}

/** Keeps isolated Sidebar renders and downstream embeddings compatible while layouts share one owner. */
export function WorkspaceSpaceBoundary({ children }: { children: ReactNode }) {
  const context = useContext(WorkspaceSpaceContext);
  return context ? children : <WorkspaceSpaceProvider>{children}</WorkspaceSpaceProvider>;
}
