import { useState, type FormEvent } from "react";
import { Check, Copy, KeyRound } from "lucide-react";
import type { ApiKeyDisplayItem } from "@/components/admin/ApiKeyManagementCard";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "@/components/ui/button";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

export type ApiKeyKind = "personal" | "service";

export interface ApiKeyCreateInput {
  kind: ApiKeyKind;
  name: string;
  expiresAt?: string;
  scopes: string[];
}

export interface ApiKeyCreateResult extends ApiKeyDisplayItem {
  key: string;
}

interface ApiKeyCreateDialogProps {
  onCreate: (input: ApiKeyCreateInput) => Promise<ApiKeyCreateResult>;
  onClose: () => void;
}

const API_KEY_SCOPE_OPTIONS = [
  { value: "read", label: "Read" },
  { value: "write", label: "Write" },
  { value: "admin", label: "Admin" },
];

export function ApiKeyCreateDialog({ onCreate, onClose }: ApiKeyCreateDialogProps) {
  const [kind, setKind] = useState<ApiKeyKind>("personal");
  const [name, setName] = useState("");
  const [expiresAt, setExpiresAt] = useState("");
  const [scopes, setScopes] = useState<string[]>(["read", "write"]);
  const [createdKey, setCreatedKey] = useState<ApiKeyCreateResult | null>(null);
  const [creating, setCreating] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectKind = (nextKind: ApiKeyKind) => {
    setKind(nextKind);
    setScopes(nextKind === "service" ? ["admin"] : ["read", "write"]);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim() || scopes.length === 0 || creating) return;
    setCreating(true);
    setError(null);
    try {
      const result = await onCreate({
        kind,
        name: name.trim(),
        expiresAt: expiresAt ? new Date(expiresAt).toISOString() : undefined,
        scopes,
      });
      setCreatedKey(result);
    } catch (createError) {
      setError(
        createError instanceof Error ? createError.message : "Unable to create this API key.",
      );
    } finally {
      setCreating(false);
    }
  };

  const copyCreatedKey = async () => {
    if (!createdKey) return;
    try {
      await navigator.clipboard.writeText(createdKey.key);
      setCopied(true);
    } catch {
      setError("Could not copy the key. Select the value and copy it manually.");
    }
  };

  return (
    <DialogShell
      labelledBy="create-api-key-title"
      maxWidth="md"
      onClose={() => {
        if (!creating) onClose();
      }}
    >
      {createdKey ? (
        <div>
          <header className="border-border flex items-start gap-3 border-b px-5 py-4">
            <span
              className="rounded-button border-success/30 bg-success/10 text-success flex size-9 shrink-0 items-center justify-center border"
              aria-hidden="true"
            >
              <Check size={17} />
            </span>
            <div className="min-w-0">
              <h2 id="create-api-key-title" className="text-text text-base font-semibold">
                API key created
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Copy this value now. It will not be shown again.
              </p>
            </div>
          </header>
          <div className="p-5">
            <label htmlFor="created-api-key" className="text-text text-xs font-medium">
              {createdKey.name}
            </label>
            <div className="mt-1.5 flex min-w-0 gap-2">
              <Input
                id="created-api-key"
                value={createdKey.key}
                readOnly
                className="min-w-0 font-mono text-xs"
                onFocus={(event) => event.currentTarget.select()}
              />
              <Button
                type="button"
                variant="secondary"
                className={`${RESPONSIVE_TOUCH_ACTION_CLASS} shrink-0 gap-1.5`}
                onClick={() => void copyCreatedKey()}
              >
                <Copy size={14} aria-hidden="true" />
                {copied ? "Copied" : "Copy"}
              </Button>
            </div>
            {error && (
              <p role="alert" className="text-danger mt-3 text-sm">
                {error}
              </p>
            )}
          </div>
          <footer className="border-border flex justify-end border-t px-5 py-3">
            <Button type="button" className={RESPONSIVE_TOUCH_ACTION_CLASS} onClick={onClose}>
              Done
            </Button>
          </footer>
        </div>
      ) : (
        <form onSubmit={(event) => void submit(event)}>
          <header className="border-border flex items-start gap-3 border-b px-5 py-4">
            <span
              className="rounded-button border-border bg-surface-raised text-primary flex size-9 shrink-0 items-center justify-center border"
              aria-hidden="true"
            >
              <KeyRound size={17} />
            </span>
            <div className="min-w-0">
              <h2 id="create-api-key-title" className="text-text text-base font-semibold">
                Create API key
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Choose the smallest access scope this integration needs.
              </p>
            </div>
          </header>

          <div className="flex max-h-[calc(100dvh-12rem)] flex-col gap-4 overflow-y-auto p-5">
            <div className="flex flex-col gap-1.5">
              <label htmlFor="api-key-kind" className="text-text text-xs font-medium">
                Key type
              </label>
              <Select
                id="api-key-kind"
                value={kind}
                disabled={creating}
                onChange={(event) => selectKind(event.target.value as ApiKeyKind)}
              >
                <option value="personal">Personal integration</option>
                <option value="service">Service account</option>
              </Select>
              <p className="text-text-muted text-xs">
                Service account keys are admin-managed and always include the admin scope.
              </p>
            </div>

            <div className="flex flex-col gap-1.5">
              <label htmlFor="api-key-name" className="text-text text-xs font-medium">
                Name
              </label>
              <Input
                id="api-key-name"
                value={name}
                maxLength={100}
                disabled={creating}
                placeholder="Production API"
                autoComplete="off"
                onChange={(event) => setName(event.target.value)}
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <label htmlFor="api-key-expiry" className="text-text text-xs font-medium">
                Expiration
              </label>
              <Input
                id="api-key-expiry"
                type="datetime-local"
                value={expiresAt}
                disabled={creating}
                onChange={(event) => setExpiresAt(event.target.value)}
              />
              <p className="text-text-muted text-xs">
                Optional. Short-lived keys reduce exposure if a secret is compromised.
              </p>
            </div>

            <fieldset>
              <legend className="text-text text-xs font-medium">Scopes</legend>
              <ToggleGroup
                type="multiple"
                value={scopes}
                disabled={creating}
                aria-label="API key scopes"
                className="mt-1.5 flex-wrap justify-start"
                onValueChange={(values) => {
                  const nextValues =
                    kind === "service"
                      ? Array.from(new Set([...values, "admin"]))
                      : values;
                  setScopes(nextValues);
                }}
              >
                {API_KEY_SCOPE_OPTIONS.map((scope) => (
                  <ToggleGroupItem
                    key={scope.value}
                    value={scope.value}
                    variant="outline"
                    size="sm"
                    disabled={kind === "service" && scope.value === "admin"}
                    aria-label={`${scope.label} scope`}
                  >
                    {scope.label}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </fieldset>

            {error && (
              <p role="alert" className="text-danger text-sm">
                {error}
              </p>
            )}
          </div>

          <footer className="border-border flex justify-end gap-2 border-t px-5 py-3">
            <Button
              type="button"
              variant="secondary"
              className={RESPONSIVE_TOUCH_ACTION_CLASS}
              disabled={creating}
              onClick={onClose}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              className={RESPONSIVE_TOUCH_ACTION_CLASS}
              disabled={creating || !name.trim() || scopes.length === 0}
            >
              {creating ? "Creating..." : "Create key"}
            </Button>
          </footer>
        </form>
      )}
    </DialogShell>
  );
}
