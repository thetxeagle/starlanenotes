import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  AdminApiKeyManagementCard,
  formatApiKeyActivity,
  isApiKeyExpired,
  type ApiKeyDisplayItem,
} from "./ApiKeyManagementCard";

const keys: ApiKeyDisplayItem[] = [
  {
    id: "production",
    name: "Production API",
    key_prefix: "sk_live_8mQ",
    is_service_account: false,
    scopes: ["read", "write"],
    last_used_at: "2026-07-25T16:48:00.000Z",
    expires_at: "2026-08-25T17:00:00.000Z",
    created_at: "2026-07-15T12:00:00.000Z",
  },
  {
    id: "pipeline",
    name: "CI/CD Pipeline",
    key_prefix: "sk_service_T3n",
    is_service_account: true,
    scopes: ["admin"],
    created_at: "2026-07-20T12:00:00.000Z",
  },
];

describe("AdminApiKeyManagementCard", () => {
  it("renders compact key metadata without exposing a full secret", () => {
    const html = renderToStaticMarkup(
      <AdminApiKeyManagementCard
        keys={keys}
        localMode={false}
        onCreate={vi.fn()}
        onRefresh={vi.fn()}
        onRevoke={vi.fn()}
      />,
    );

    expect(html).toContain("Create and manage keys for API access.");
    expect(html).toContain("1 key active");
    expect(html).toContain("Production API");
    expect(html).toContain("CI/CD Pipeline");
    expect(html).toContain("service");
    expect(html).toContain("read");
    expect(html).toContain("write");
    expect(html).toContain("admin");
    expect(html).toContain("sk_live_8mQ••••••••");
    expect(html).toContain("Last used");
    expect(html).toContain("Created");
    expect(html).toContain("Expired");
    expect(html).toContain("Revoke");
    expect(html).not.toContain("sk_live_8mQ_full_secret");
  });

  it("renders the intentional empty state", () => {
    const html = renderToStaticMarkup(
      <AdminApiKeyManagementCard
        keys={[]}
        localMode={false}
        onCreate={vi.fn()}
        onRefresh={vi.fn()}
        onRevoke={vi.fn()}
      />,
    );

    expect(html).toContain("0 keys active");
    expect(html).toContain("No API keys yet");
    expect(html).toContain("scripts, integrations, or service automation");
  });

  it("formats activity and expiration state predictably", () => {
    const now = Date.parse("2026-07-25T17:00:00.000Z");
    expect(formatApiKeyActivity(undefined, now)).toBe("Never");
    expect(formatApiKeyActivity("2026-07-25T16:48:00.000Z", now)).toBe("12m ago");
    expect(
      isApiKeyExpired(
        {
          ...keys[0],
          expires_at: "2026-07-25T16:59:59.000Z",
        },
        now,
      ),
    ).toBe(true);
  });
});
