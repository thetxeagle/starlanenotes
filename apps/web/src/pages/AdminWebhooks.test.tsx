import { describe, expect, it } from "vitest";
import { getAdminWebhookHealth, getWebhookTestResult, type WebhookDeliveryItem } from "./AdminPage";

const testId = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
function attempt(patch: Partial<WebhookDeliveryItem> = {}): WebhookDeliveryItem {
  return {
    id: "delivery-a",
    event_type: "webhook.test",
    test_id: testId,
    response_status: 500,
    success: false,
    attempt_number: 1,
    delivered_at: "2026-10-08T12:00:00Z",
    ...patch,
  };
}

describe("saved webhook test evidence", () => {
  it("derives truthful connection health from the latest recorded attempt", () => {
    expect(getAdminWebhookHealth(true, []).label).toBe("Configured");
    expect(getAdminWebhookHealth(false, []).label).toBe("Inactive");
    expect(
      getAdminWebhookHealth(true, [attempt({ response_status: 204, success: true })]).label,
    ).toBe("Healthy");
    expect(getAdminWebhookHealth(true, [attempt({ attempt_number: 2 })]).label).toBe("Retrying");
    expect(getAdminWebhookHealth(true, [attempt({ attempt_number: 3 })]).label).toBe(
      "Needs attention",
    );

    const health = getAdminWebhookHealth(true, [
      attempt({ id: "older-failure", attempt_number: 3 }),
      attempt({
        id: "newer-success",
        response_status: 202,
        success: true,
        delivered_at: "2026-10-08T12:05:00Z",
      }),
    ]);
    expect(health.label).toBe("Healthy");
    expect(health.detail).toContain("HTTP 202");
    expect(health.className).toContain("text-success");
  });

  it("waits when the requested test has no recorded attempt", () => {
    expect(getWebhookTestResult([], testId)).toBe("waiting");
    expect(
      getWebhookTestResult(
        [
          attempt({ test_id: "another-test", response_status: 200, success: true }),
          attempt({ event_type: "document.updated", response_status: 200, success: true }),
          attempt({ test_id: null, response_status: 200, success: true }),
        ],
        testId,
      ),
    ).toBe("waiting");
  });

  it("does not confuse a failed first attempt with exhausted retries", () => {
    expect(getWebhookTestResult([attempt()], testId)).toBe("retrying");
    expect(getWebhookTestResult([attempt({ attempt_number: 2 })], testId)).toBe("retrying");
    expect(getWebhookTestResult([attempt({ attempt_number: 3 })], testId)).toBe("failed");
  });

  it("requires recorded success and HTTP acceptance from the matching test", () => {
    for (const status of [null, 199, 300, 500]) {
      expect(
        getWebhookTestResult([attempt({ response_status: status, success: true })], testId),
      ).not.toBe("succeeded");
    }
    expect(
      getWebhookTestResult([attempt({ response_status: 204, success: false })], testId),
    ).not.toBe("succeeded");
    for (const status of [200, 202, 204, 299]) {
      expect(
        getWebhookTestResult([attempt({ response_status: status, success: true })], testId),
      ).toBe("succeeded");
    }
  });

  it("recognizes eventual retry success without depending on record ordering", () => {
    const evidence = [
      attempt({ id: "failed-first" }),
      attempt({ id: "failed-final", attempt_number: 3 }),
      attempt({
        id: "successful-redelivery",
        attempt_number: 1,
        response_status: 202,
        success: true,
      }),
      attempt({ id: "unrelated-newer", test_id: "another-test", attempt_number: 3 }),
    ];
    expect(getWebhookTestResult(evidence, testId)).toBe("succeeded");
    expect(getWebhookTestResult([...evidence].reverse(), testId)).toBe("succeeded");
  });

  it("keeps two concurrent tests independent", () => {
    const evidence = [
      attempt({ response_status: 204, success: true }),
      attempt({ id: "delivery-b", test_id: "another-test", attempt_number: 3 }),
    ];
    expect(getWebhookTestResult(evidence, testId)).toBe("succeeded");
    expect(getWebhookTestResult(evidence, "another-test")).toBe("failed");
    expect(getWebhookTestResult(evidence, "not-recorded")).toBe("waiting");
  });
});
