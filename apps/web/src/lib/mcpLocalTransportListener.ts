import {
  executeConfirmedLocalMcpDocumentCreate,
  handleLocalMcpEnabledTransportRequestEnvelope,
  type LocalMcpTransportAuthPreviewResponseEnvelope,
  type LocalMcpTransportKind,
  type LocalMcpTransportRequestEnvelope,
} from "./mcpLocalRuntime";

export type LocalMcpTransportListenerStatus = "starting" | "running" | "stopping" | "stopped";

export interface LocalMcpTransportListenerState {
  transport: LocalMcpTransportKind;
  status: LocalMcpTransportListenerStatus;
  requests_received: number;
  responses_emitted: number;
  invalid_messages: number;
  last_error?: "invalid_request" | "listener_error";
}

export interface LocalMcpStdioTransportBinding {
  /** A JSON object per line, supplied by the native or CLI host's stdin. */
  input: AsyncIterable<string>;
  /** Receives one JSON response per line for the native or CLI host's stdout. */
  write: (line: string) => void | Promise<void>;
}

export interface LocalMcpSseTransportBinding {
  /** Registers the request-event callback and returns its unsubscriber. */
  subscribe: (listener: (payload: string) => void | Promise<void>) => () => void;
  /** Receives a complete Server-Sent Event frame for the host's response stream. */
  send: (frame: string) => void | Promise<void>;
}

export interface LocalMcpTransportListenerOptions {
  checkedAt?: () => string;
  /** Explicit host opt-in. Without this, listeners remain read-only. */
  allowConfirmedDocumentCreate?: boolean;
}

export interface LocalMcpTransportListener {
  readonly state: LocalMcpTransportListenerState;
  stop(): Promise<void>;
  /** Wait for input that was accepted before stop/drain to finish emitting. */
  drain(): Promise<void>;
}

type LocalMcpWireResponse =
  | LocalMcpTransportAuthPreviewResponseEnvelope
  | { id: string; transport: LocalMcpTransportKind; runtime_mode: "local_vault"; executed: true; reason: "dispatch_allowed"; result: Awaited<ReturnType<typeof executeConfirmedLocalMcpDocumentCreate>> }
  | {
      runtime_mode: "local_vault";
      executed: false;
      reason: "request_invalid";
      error_code: "invalid_request";
    };

const INVALID_REQUEST_RESPONSE: LocalMcpWireResponse = {
  runtime_mode: "local_vault",
  executed: false,
  reason: "request_invalid",
  error_code: "invalid_request",
};

class LocalMcpTransportListenerRuntime implements LocalMcpTransportListener {
  readonly state: LocalMcpTransportListenerState;
  private readonly checkedAt: () => string;
  private readonly emit: (response: LocalMcpWireResponse) => Promise<void>;
  private readonly allowConfirmedDocumentCreate: boolean;
  private accepting = true;
  private readonly pending = new Set<Promise<void>>();
  private unsubscribe?: () => void;

  constructor(
    transport: LocalMcpTransportKind,
    emit: (response: LocalMcpWireResponse) => Promise<void>,
    options: LocalMcpTransportListenerOptions,
  ) {
    this.state = {
      transport,
      status: "starting",
      requests_received: 0,
      responses_emitted: 0,
      invalid_messages: 0,
    };
    this.emit = emit;
    this.checkedAt = options.checkedAt ?? (() => new Date().toISOString());
    this.allowConfirmedDocumentCreate = options.allowConfirmedDocumentCreate === true;
  }

  start(): void {
    this.state.status = "running";
  }

  setUnsubscribe(unsubscribe: () => void): void {
    this.unsubscribe = unsubscribe;
  }

  receive(payload: string): void {
    if (!this.accepting) return;
    const pending = this.dispatch(payload).finally(() => this.pending.delete(pending));
    this.pending.add(pending);
  }

  async drain(): Promise<void> {
    // Let an async stdio iterator hand its first line to receive() before
    // snapshotting pending work. This keeps drain deterministic for short-lived
    // CLI streams without waiting for an intentionally long-lived listener.
    await Promise.resolve();
    await Promise.all([...this.pending]);
  }

  async stop(): Promise<void> {
    if (this.state.status === "stopped") return;
    this.state.status = "stopping";
    this.accepting = false;
    this.unsubscribe?.();
    await this.drain();
    this.state.status = "stopped";
  }

  private async dispatch(payload: string): Promise<void> {
    this.state.requests_received += 1;
    const envelope = parseEnvelope(payload, this.state.transport);
    if (!envelope) this.state.invalid_messages += 1;

    try {
      const checkedAt = this.checkedAt();
      const response = !envelope
        ? INVALID_REQUEST_RESPONSE
        : this.allowConfirmedDocumentCreate && envelope.tool === "starlane.documents.create"
          ? { id: envelope.id, transport: envelope.transport, runtime_mode: "local_vault" as const, executed: true as const, reason: "dispatch_allowed" as const, result: await executeConfirmedLocalMcpDocumentCreate(envelope, checkedAt) }
          : await handleLocalMcpEnabledTransportRequestEnvelope(envelope, checkedAt);
      await this.emit(response);
      this.state.responses_emitted += 1;
    } catch {
      this.state.last_error = "listener_error";
    }
  }
}

/**
 * Starts a real JSON-lines request loop over host-provided stdio streams.
 * The caller owns process creation; this adapter owns request parsing, dispatch,
 * redacted response emission, and shutdown.
 */
export function startLocalMcpStdioListener(
  binding: LocalMcpStdioTransportBinding,
  options: LocalMcpTransportListenerOptions = {},
): LocalMcpTransportListener {
  const runtime = new LocalMcpTransportListenerRuntime(
    "stdio",
    async (response) => binding.write(`${JSON.stringify(response)}\n`),
    options,
  );
  runtime.start();
  void (async () => {
    try {
      for await (const line of binding.input) runtime.receive(line);
      await runtime.drain();
    } catch {
      runtime.state.last_error = "listener_error";
    }
  })();
  return runtime;
}

/**
 * Registers a request listener on a host-provided SSE endpoint and emits each
 * response as an RFC 8895-compatible event frame. This remains read-only: the
 * canonical MCP dispatch gate still rejects writes after journal recording.
 */
export function startLocalMcpSseListener(
  binding: LocalMcpSseTransportBinding,
  options: LocalMcpTransportListenerOptions = {},
): LocalMcpTransportListener {
  const runtime = new LocalMcpTransportListenerRuntime(
    "sse",
    async (response) => binding.send(`event: message\ndata: ${JSON.stringify(response)}\n\n`),
    options,
  );
  runtime.setUnsubscribe(binding.subscribe((payload) => runtime.receive(payload)));
  runtime.start();
  return runtime;
}

function parseEnvelope(
  payload: string,
  transport: LocalMcpTransportKind,
): LocalMcpTransportRequestEnvelope | undefined {
  try {
    const parsed: unknown = JSON.parse(payload);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return undefined;
    const value = parsed as Record<string, unknown>;
    if (value.transport !== transport || typeof value.id !== "string" || typeof value.tool !== "string") {
      return undefined;
    }
    if (value.request !== undefined && (!value.request || typeof value.request !== "object" || Array.isArray(value.request))) {
      return undefined;
    }
    return value as unknown as LocalMcpTransportRequestEnvelope;
  } catch {
    return undefined;
  }
}
