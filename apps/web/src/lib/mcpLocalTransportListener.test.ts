import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { __resetLocalDataForTests, dataClient } from "./dataClient";
import { setCurrentLaunchMode } from "./launchMode";
import {
  acceptLocalMcpCapabilityTokenRecord,
  clearLocalMcpCapabilityTokenRecords,
  clearLocalMcpOperationJournalRecords,
  issueLocalMcpCapabilityToken,
} from "./mcpLocalRuntime";
import { startLocalMcpSseListener, startLocalMcpStdioListener } from "./mcpLocalTransportListener";
import { setSyncProvider } from "./storageProviders";

describe("local MCP transport listeners", () => {
  beforeEach(async () => {
    const values = new Map<string, string>();
    const localStorageMock = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    };
    (globalThis as { window?: unknown }).window = { localStorage: localStorageMock };
    (globalThis as { localStorage?: unknown }).localStorage = localStorageMock;
    setCurrentLaunchMode("local");
    setSyncProvider("none");
    clearLocalMcpCapabilityTokenRecords();
    clearLocalMcpOperationJournalRecords();
    await __resetLocalDataForTests();
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { localStorage?: unknown }).localStorage;
  });

  it("runs accepted read-only JSONL requests and never leaks their raw token", async () => {
    const token = "listener-stdio-raw-token";
    await issueAndAccept("stdio", token);
    const responses: string[] = [];
    const listener = startLocalMcpStdioListener(
      {
        input: lines([
          JSON.stringify({
            id: "stdio-read-1",
            transport: "stdio",
            tool: "starlane.spaces.list",
            request: { capability_token: token },
          }),
        ]),
        write: (line) => {
          responses.push(line);
        },
      },
      { checkedAt: () => "2026-07-21T11:59:00.000Z" },
    );

    await listener.drain();
    await listener.stop();

    expect(listener.state).toMatchObject({
      transport: "stdio",
      status: "stopped",
      requests_received: 1,
      responses_emitted: 1,
      invalid_messages: 0,
    });
    expect(responses).toHaveLength(1);
    expect(responses[0]).toContain('"executed":true');
    expect(responses[0]).not.toContain(token);
  });

  it("registers an SSE listener, frames responses, and detaches cleanly", async () => {
    const token = "listener-sse-raw-token";
    await issueAndAccept("sse", token);
    const frames: string[] = [];
    let receive: ((payload: string) => void | Promise<void>) | undefined;
    let unsubscribed = false;
    const listener = startLocalMcpSseListener(
      {
        subscribe: (callback) => {
          receive = callback;
          return () => {
            unsubscribed = true;
          };
        },
        send: (frame) => {
          frames.push(frame);
        },
      },
      { checkedAt: () => "2026-07-21T11:59:00.000Z" },
    );

    await receive?.(
      JSON.stringify({
        id: "sse-read-1",
        transport: "sse",
        tool: "starlane.spaces.list",
        request: { capability_token: token },
      }),
    );
    await Promise.resolve();
    await listener.drain();
    await listener.stop();

    expect(frames).toHaveLength(1);
    expect(frames[0]).toMatch(/^event: message\ndata: /);
    expect(frames[0]).toContain('"executed":true');
    expect(frames[0]).not.toContain(token);
    expect(unsubscribed).toBe(true);
  });

  it("returns a safe invalid-request frame instead of dispatching malformed input", async () => {
    const responses: string[] = [];
    const listener = startLocalMcpStdioListener({
      input: lines(["{ definitely not json"]),
      write: (line) => {
        responses.push(line);
      },
    });

    await listener.drain();
    await listener.stop();

    expect(listener.state).toMatchObject({ invalid_messages: 1, responses_emitted: 1 });
    expect(responses).toEqual([
      '{"runtime_mode":"local_vault","executed":false,"reason":"request_invalid","error_code":"invalid_request"}\n',
    ]);
  });

  it("lets an embedding host opt into only confirmed document creation", async () => {
    const token = "listener-stdio-write-token";
    const space = await dataClient.createSpace("MCP host writes");
    await issueAndAccept("stdio", token, "starlane.documents.create");
    const responses: string[] = [];
    const listener = startLocalMcpStdioListener(
      {
        input: lines([
          JSON.stringify({
            id: "host-write-1",
            transport: "stdio",
            tool: "starlane.documents.create",
            request: {
              capability_token: token,
              confirmation_text: "confirm local mcp starlane.documents.create",
              input: { space_id: space.id, title: "Host created" },
            },
          }),
        ]),
        write: (line) => {
          responses.push(line);
        },
      },
      { checkedAt: () => "2026-07-21T11:59:00.000Z", allowConfirmedDocumentCreate: true },
    );
    await Promise.resolve();
    await listener.drain();
    await listener.stop();
    expect(responses[0]).toContain('"read_only":false');
    expect(await dataClient.listDocuments(space.id)).toEqual([
      expect.objectContaining({
        title: "Host created",
        published_at: undefined,
        draft_id: expect.any(String),
      }),
    ]);
    expect(await dataClient.listDocumentDrafts(space.id)).toHaveLength(1);
  });
});

async function issueAndAccept(
  transport: "stdio" | "sse",
  token: string,
  tool = "starlane.spaces.list",
): Promise<void> {
  await issueLocalMcpCapabilityToken(
    {
      request_id: `listener-${transport}-token`,
      created_at: "2026-07-21T11:00:00.000Z",
      runtime_mode: "local_vault",
      transport,
      scope: "notes:read",
      vault_fingerprint: "vault-sha256:listener-test",
      confirmation_text: "confirm local mcp capability token",
    },
    token,
  );
  await acceptLocalMcpCapabilityTokenRecord(
    {
      id: `listener-${transport}-accept`,
      transport,
      tool,
      request: { capability_token: token },
    },
    "2026-07-21T11:30:00.000Z",
    { acceptance_enabled: true },
  );
}

async function* lines(values: string[]): AsyncGenerator<string> {
  yield* values;
}
