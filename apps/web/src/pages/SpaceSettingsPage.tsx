import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, Pencil, Plus, RefreshCw, Settings, Shield, UsersRound } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { api } from "@/lib/api";
import { dataClient, type SpaceRecord } from "@/lib/dataClient";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { buildSpaceHomePath } from "@/lib/lastViewedSpace";

interface SpaceMemberItem {
  user_id: string;
  email: string;
  display_name: string;
  role: string;
}

interface SpaceGroupItem {
  group_id: string;
  name: string;
  description?: string;
  role: string;
  member_count: number;
}

const visibilityOptions = [
  { value: "private", label: "Private" },
  { value: "internal", label: "Signed-in users" },
  { value: "public", label: "Public" },
];

export function sortSpacesForSettings(spaces: SpaceRecord[]): SpaceRecord[] {
  return [...spaces].sort((a, b) => a.name.localeCompare(b.name));
}

export function getSpaceSettingsRoute(spaceId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/settings`;
}

export function getLocalSpaceEditState(
  space: Pick<SpaceRecord, "id">,
  activeSpaceId?: string | null,
): "editing" | "edit" {
  return space.id === activeSpaceId ? "editing" : "edit";
}

export function SpaceSettingsPage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const localMode = getCurrentLaunchMode() === "local";
  const [space, setSpace] = useState<SpaceRecord | null>(null);
  const [localSpaces, setLocalSpaces] = useState<SpaceRecord[]>([]);
  const [nameDraft, setNameDraft] = useState("");
  const [newSpaceName, setNewSpaceName] = useState("");
  const [visibility, setVisibility] = useState("private");
  const [members, setMembers] = useState<SpaceMemberItem[]>([]);
  const [groups, setGroups] = useState<SpaceGroupItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [savingName, setSavingName] = useState(false);
  const [creatingSpace, setCreatingSpace] = useState(false);
  const [savingVisibility, setSavingVisibility] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  const canManageSpace = localMode || space?.member_role === "owner";
  const homePath = spaceId ? buildSpaceHomePath(spaceId) : "/";
  const normalizedDraft = nameDraft.trim();
  const nameChanged = Boolean(space && normalizedDraft && normalizedDraft !== space.name);

  const load = useCallback(async () => {
    if (!spaceId) return;
    setLoading(true);
    setError("");
    try {
      const loadedSpace = await dataClient.getSpace(spaceId);
      setSpace(loadedSpace);
      setNameDraft(loadedSpace.name);
      setVisibility(loadedSpace.visibility ?? "private");
      if (localMode) {
        const spaces = await dataClient.listSpaces();
        const hasCurrentSpace = spaces.some((item) => item.id === loadedSpace.id);
        setLocalSpaces(sortSpacesForSettings(hasCurrentSpace ? spaces : [...spaces, loadedSpace]));
        setMembers([]);
        setGroups([]);
      } else {
        const [memberList, groupList] = await Promise.all([
          api.fetch<SpaceMemberItem[]>(`/spaces/${spaceId}/members`),
          api.fetch<SpaceGroupItem[]>(`/spaces/${spaceId}/groups`),
        ]);
        setMembers(memberList);
        setGroups(groupList);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to load space settings.");
    } finally {
      setLoading(false);
    }
  }, [localMode, spaceId]);

  useEffect(() => {
    void load();
  }, [load]);

  const saveName = async () => {
    if (!spaceId || !nameChanged) return;
    setSavingName(true);
    setMessage("");
    setError("");
    try {
      const updated = await dataClient.updateSpace(spaceId, { name: normalizedDraft });
      setSpace((current) => (current ? { ...current, ...updated } : updated));
      setNameDraft(updated.name);
      setMessage("Space name saved.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save space name.");
    } finally {
      setSavingName(false);
    }
  };

  const createLocalSpace = async () => {
    if (!localMode) return;
    const normalizedName = newSpaceName.trim();
    if (!normalizedName) return;
    setCreatingSpace(true);
    setMessage("");
    setError("");
    try {
      const created = await dataClient.createSpace(normalizedName);
      setLocalSpaces((current) => sortSpacesForSettings([...current, created]));
      setNewSpaceName("");
      setMessage("Space created.");
      navigate(getSpaceSettingsRoute(created.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to create space.");
    } finally {
      setCreatingSpace(false);
    }
  };

  const saveVisibility = async (nextVisibility: string) => {
    if (!spaceId || localMode) return;
    setVisibility(nextVisibility);
    setSavingVisibility(true);
    setMessage("");
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}`, {
        method: "PUT",
        body: { visibility: nextVisibility },
      });
      setSpace((current) => (current ? { ...current, visibility: nextVisibility } : current));
      setMessage("Space visibility saved.");
    } catch (err) {
      setVisibility(space?.visibility ?? "private");
      setError(err instanceof Error ? err.message : "Unable to save visibility.");
    } finally {
      setSavingVisibility(false);
    }
  };

  const accessSummary = useMemo(() => {
    if (localMode) return "Local vault access";
    const count = members.length + groups.length;
    return `${count} access ${count === 1 ? "entry" : "entries"}`;
  }, [groups.length, localMode, members.length]);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-text-muted">
        Loading space settings...
      </div>
    );
  }

  return (
    <div className="min-h-full bg-bg">
      <div className="mx-auto max-w-5xl px-4 py-2 sm:px-6 sm:py-5 lg:px-8">
        <header className="mb-3 flex flex-wrap items-end justify-between gap-2 border-b border-border pb-3 sm:mb-5 sm:items-start sm:gap-3 sm:pb-4">
          <div className="min-w-0">
            <button
              type="button"
              onClick={() => navigate(homePath)}
              className="mb-1 inline-flex items-center gap-1 rounded-button px-1 py-1 text-sm text-text-muted hover:bg-surface-raised hover:text-text sm:mb-2"
            >
              <ArrowLeft size={14} />
              Back to space
            </button>
            <div className="flex items-center gap-2 text-xs text-text-muted">
              <Settings size={14} />
              Space settings
            </div>
            <h1 className="mt-0.5 truncate text-xl font-semibold text-text sm:mt-1 sm:text-2xl">
              {space?.name ?? "Space"}
            </h1>
          </div>
          <div>
            <Button type="button" size="sm" variant="secondary" onClick={() => void load()}>
              <RefreshCw size={14} />
              Refresh
            </Button>
          </div>
        </header>

        {message && (
          <div className="mb-4 rounded-input border border-success/30 bg-success/10 px-3 py-2 text-sm text-success">
            {message}
          </div>
        )}
        {error && (
          <div className="mb-4 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </div>
        )}

        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
          <section className="rounded-card border border-border bg-surface">
            <div className="border-b border-border px-4 py-3">
              <h2 className="text-base font-semibold text-text">Identity</h2>
              <p className="text-xs text-text-muted">
                Rename this space without changing site-wide administration.
              </p>
            </div>
            <div className="space-y-3 p-4">
              <label htmlFor="space-name" className="block text-sm font-medium text-text">
                Space name
              </label>
              <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                <Input
                  id="space-name"
                  value={nameDraft}
                  onChange={(event) => setNameDraft(event.target.value)}
                  disabled={!canManageSpace || savingName}
                  aria-label="Space name"
                />
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void saveName()}
                  disabled={!canManageSpace || !nameChanged || savingName}
                >
                  Save name
                </Button>
              </div>
              {space?.slug && <p className="text-xs text-text-muted">Slug: {space.slug}</p>}
            </div>
          </section>

          <aside className="rounded-card border border-border bg-surface">
            <div className="border-b border-border px-4 py-3">
              <h2 className="text-base font-semibold text-text">Access</h2>
              <p className="text-xs text-text-muted">{accessSummary}</p>
            </div>
            <div className="space-y-3 p-4">
              <div>
                <label htmlFor="space-visibility" className="mb-1 block text-sm font-medium text-text">
                  Visibility
                </label>
                <Select
                  id="space-visibility"
                  value={visibility}
                  onChange={(event) => void saveVisibility(event.target.value)}
                  disabled={localMode || !canManageSpace || savingVisibility}
                  aria-label="Space visibility"
                >
                  {visibilityOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </Select>
              </div>
              {localMode && (
                <p className="text-sm leading-6 text-text-muted">
                  This standalone space is stored in the local vault. Server users and groups are not
                  available here.
                </p>
              )}
            </div>
          </aside>
        </div>

        {!localMode && (
          <section className="mt-4 rounded-card border border-border bg-surface">
            <div className="flex items-center gap-2 border-b border-border px-4 py-3">
              <UsersRound size={16} className="text-text-muted" />
              <h2 className="text-base font-semibold text-text">People and Groups</h2>
            </div>
            <div className="grid gap-4 p-4 lg:grid-cols-2">
              <AccessList
                title="Users"
                emptyText="No users have direct access."
                items={members.map((member) => ({
                  id: member.user_id,
                  title: member.display_name,
                  detail: `${member.email} - ${roleLabel(member.role)}`,
                }))}
              />
              <AccessList
                title="Groups"
                emptyText="No groups have access."
                items={groups.map((group) => ({
                  id: group.group_id,
                  title: group.name,
                  detail: `${group.member_count} members - ${roleLabel(group.role)}`,
                }))}
              />
            </div>
          </section>
        )}

        {localMode && (
          <section className="mt-4 rounded-card border border-border bg-surface">
            <div className="border-b border-border px-4 py-3">
              <h2 className="text-base font-semibold text-text">Spaces in this vault</h2>
              <p className="text-xs text-text-muted">
                Create another local space or switch settings to an existing one.
              </p>
            </div>
            <div className="grid gap-4 p-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
              <div>
                <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                  <Input
                    value={newSpaceName}
                    onChange={(event) => setNewSpaceName(event.target.value)}
                    disabled={creatingSpace}
                    placeholder="New space name"
                    aria-label="New local space name"
                  />
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => void createLocalSpace()}
                    disabled={!newSpaceName.trim() || creatingSpace}
                  >
                    <Plus size={14} />
                    Create space
                  </Button>
                </div>
              </div>
              <div className="rounded-card border border-border">
                {localSpaces.length === 0 ? (
                  <p className="px-3 py-3 text-sm text-text-muted">No local spaces found.</p>
                ) : (
                  <div className="max-h-56 divide-y divide-border overflow-y-auto">
                    {localSpaces.map((item) => {
                      const editState = getLocalSpaceEditState(item, space?.id);
                      const active = editState === "editing";
                      return (
                        <div
                          key={item.id}
                          className={`grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-3 py-2 ${
                            active
                              ? "bg-surface-raised text-text"
                              : "text-text-muted hover:bg-surface-raised hover:text-text"
                          }`}
                          aria-current={active ? "page" : undefined}
                        >
                          <div className="min-w-0">
                            <span className="block truncate text-sm font-medium">{item.name}</span>
                            {item.slug && (
                              <span className="block truncate text-xs text-text-muted">
                                {item.slug}
                              </span>
                            )}
                          </div>
                          {active ? (
                            <span className="text-xs font-medium text-primary">Editing</span>
                          ) : (
                            <Button
                              type="button"
                              size="sm"
                              variant="ghost"
                              onClick={() => navigate(getSpaceSettingsRoute(item.id))}
                              aria-label={`Edit ${item.name} settings`}
                            >
                              <Pencil size={13} />
                              Edit
                            </Button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

function roleLabel(role: string): string {
  switch (role) {
    case "owner":
      return "Owner";
    case "editor":
      return "Read/write";
    case "viewer":
      return "Read";
    default:
      return role;
  }
}

function AccessList({
  title,
  emptyText,
  items,
}: {
  title: string;
  emptyText: string;
  items: Array<{ id: string; title: string; detail: string }>;
}) {
  return (
    <div className="rounded-card border border-border">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <Shield size={14} className="text-text-muted" />
        <h3 className="text-sm font-medium text-text">{title}</h3>
      </div>
      <div className="divide-y divide-border">
        {items.length === 0 ? (
          <p className="px-3 py-3 text-sm text-text-muted">{emptyText}</p>
        ) : (
          items.map((item) => (
            <div key={item.id} className="px-3 py-2">
              <p className="truncate text-sm text-text">{item.title}</p>
              <p className="truncate text-xs text-text-muted">{item.detail}</p>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
