import { Check, LockKeyhole, Minus, Pencil, Plus, Search, ShieldCheck, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  DEFAULT_SPACE_ROLE_DEFINITIONS,
  SPACE_PERMISSION_LABELS,
  type SpacePermissionId,
  type SpaceRoleDefinition,
} from "@/lib/spacePermissions";

const permissionOrder = Object.keys(SPACE_PERMISSION_LABELS) as SpacePermissionId[];

export function SpaceRolePermissions({
  definitions = DEFAULT_SPACE_ROLE_DEFINITIONS,
  totalCount = definitions.length,
  query = "",
  onQueryChange,
  onCreate,
  onEdit,
  onDelete,
}: {
  definitions?: SpaceRoleDefinition[];
  totalCount?: number;
  query?: string;
  onQueryChange?: (value: string) => void;
  onCreate?: () => void;
  onEdit?: (role: SpaceRoleDefinition) => void;
  onDelete?: (role: SpaceRoleDefinition) => void;
}) {
  return (
    <section
      className="rounded-card border-border bg-surface min-w-0 max-w-full border"
      aria-labelledby="space-roles-title"
    >
      <header className="border-border flex flex-wrap items-start justify-between gap-3 border-b px-4 py-3">
        <div className="flex min-w-0 items-start gap-2">
          <ShieldCheck size={16} className="text-text-muted mt-0.5 shrink-0" aria-hidden="true" />
          <div>
            <h2 id="space-roles-title" className="text-text text-base font-semibold">
              Space roles
            </h2>
            <p className="text-text-muted mt-0.5 text-xs">
              Built-in roles are additive and enforced for direct users and assigned groups.
            </p>
          </div>
        </div>
        {onCreate ? (
          <Button
            type="button"
            size="sm"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            onClick={onCreate}
          >
            <Plus size={14} />
            Create role
          </Button>
        ) : (
          <Badge variant="outline">
            <LockKeyhole size={12} className="mr-1" aria-hidden="true" />
            Built in
          </Badge>
        )}
      </header>

      {onQueryChange && (
        <div className="border-border flex flex-wrap items-center justify-between gap-3 border-b px-4 py-2.5">
          <div className="relative min-w-[14rem] flex-1 sm:max-w-md">
            <Search
              size={14}
              className="text-text-subtle pointer-events-none absolute left-3 top-1/2 -translate-y-1/2"
              aria-hidden="true"
            />
            <Input
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              placeholder="Search roles or permissions"
              aria-label="Search space roles"
              className="pl-9"
            />
          </div>
          <span className="text-text-muted text-xs">
            {definitions.length} of {totalCount}
          </span>
        </div>
      )}

      <div className="divide-border divide-y md:hidden">
        {definitions.map((definition) => (
          <article key={definition.id} className="px-4 py-3">
            <div className="flex items-start justify-between gap-2">
              <div>
                <h3 className="text-text text-sm font-medium">{definition.name}</h3>
                <p className="text-text-muted mt-0.5 text-xs">{definition.description}</p>
              </div>
              <Badge variant="outline">{definition.built_in === false ? "Space" : "Built in"}</Badge>
            </div>
            <div className="mt-3 flex flex-wrap gap-1.5">
              {definition.permissions.map((permission) => (
                <Badge key={permission} variant="outline">
                  <Check size={12} className="text-primary mr-1" aria-hidden="true" />
                  {SPACE_PERMISSION_LABELS[permission]}
                </Badge>
              ))}
            </div>
            {definition.built_in === false && onEdit && onDelete && (
              <div className="mt-3 flex gap-2">
                <Button type="button" size="sm" variant="secondary" onClick={() => onEdit(definition)}>
                  <Pencil size={14} />
                  Edit
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="text-danger hover:text-danger"
                  onClick={() => onDelete(definition)}
                >
                  <Trash2 size={14} />
                  Delete
                </Button>
              </div>
            )}
          </article>
        ))}
      </div>

      <div className="hidden overflow-x-auto md:block">
        <table className="w-full min-w-[50rem] border-collapse text-left text-sm">
          <thead>
            <tr className="border-border bg-surface-raised border-b">
              <th scope="col" className="text-text-muted w-[18rem] px-4 py-3 text-xs font-medium">
                Role
              </th>
              <th scope="col" className="text-text-muted px-3 py-3 text-xs font-medium">
                Scope
              </th>
              {permissionOrder.map((permission) => (
                <th
                  key={permission}
                  scope="col"
                  className="text-text-muted px-3 py-3 text-center text-xs font-medium"
                >
                  {SPACE_PERMISSION_LABELS[permission]}
                </th>
              ))}
              {(onEdit || onDelete) && (
                <th scope="col" className="text-text-muted px-4 py-3 text-right text-xs font-medium">
                  Actions
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {definitions.map((definition) => (
              <tr key={definition.id} className="border-border border-b last:border-0">
                <th scope="row" className="px-4 py-3 font-normal">
                  <span className="text-text block font-medium">{definition.name}</span>
                  <span className="text-text-muted mt-0.5 block text-xs font-normal">
                    {definition.description}
                  </span>
                </th>
                <td className="px-3 py-3">
                  <Badge variant="outline">
                    {definition.built_in === false ? "This space" : "Built in"}
                  </Badge>
                </td>
                {permissionOrder.map((permission) => {
                  const enabled = definition.permissions.includes(permission);
                  return (
                    <td key={permission} className="px-3 py-3 text-center">
                      <span className="sr-only">
                        {enabled ? "Allowed" : "Not allowed"}: {SPACE_PERMISSION_LABELS[permission]}
                      </span>
                      {enabled ? (
                        <Check size={16} className="text-primary mx-auto" aria-hidden="true" />
                      ) : (
                        <Minus size={16} className="text-text-subtle mx-auto" aria-hidden="true" />
                      )}
                    </td>
                  );
                })}
                {(onEdit || onDelete) && (
                  <td className="px-4 py-3">
                    {definition.built_in === false && (
                      <div className="flex justify-end gap-1">
                        {onEdit && (
                          <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() => onEdit(definition)}
                            aria-label={`Edit ${definition.name}`}
                          >
                            <Pencil size={14} />
                          </Button>
                        )}
                        {onDelete && (
                          <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-danger hover:text-danger"
                            onClick={() => onDelete(definition)}
                            aria-label={`Delete ${definition.name}`}
                          >
                            <Trash2 size={14} />
                          </Button>
                        )}
                      </div>
                    )}
                  </td>
                )}
              </tr>
            ))}
            {definitions.length === 0 && (
              <tr>
                <td
                  colSpan={permissionOrder.length + 3}
                  className="text-text-muted px-4 py-10 text-center"
                >
                  No roles match this search.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
