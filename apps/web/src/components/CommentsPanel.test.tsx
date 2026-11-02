import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  claimCommentMutation,
  CommentsPanel,
  releaseCommentMutation,
  settleCurrentCommentsRequest,
  shouldSubmitCommentOnEnter,
} from "./CommentsPanel";
import type { DocumentComment } from "@/lib/documentComments";

function makeComment(documentId: string, content: string): DocumentComment {
  return {
    id: `comment-${documentId}`,
    document_id: documentId,
    user_id: "user-1",
    user_name: "Test user",
    content,
    resolved: false,
    created_at: "2026-10-03T12:00:00.000Z",
    replies: [],
  };
}

describe("CommentsPanel", () => {
  it("uses the shared input control for comment entry", () => {
    const html = renderToStaticMarkup(<CommentsPanel spaceId="space-1" docId="doc-1" />);

    expect(html).toContain('aria-label="New comment"');
    expect(html).toContain('aria-label="Send comment"');
    expect(html).toContain("Add a comment...");
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
  });

  it("exposes document reactions as toggle controls", () => {
    const html = renderToStaticMarkup(<CommentsPanel spaceId="space-1" docId="doc-1" />);

    expect(html).toContain('role="group"');
    expect(html).toContain('aria-label="Document reactions"');
    expect(html.match(/aria-pressed="false"/g)).toHaveLength(3);
  });

  it("connects each reply disclosure to its composer", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/CommentsPanel.tsx"), "utf8");

    expect(source).toContain("aria-expanded={replyTo === comment.id}");
    expect(source).toContain("`comment-reply-composer-${comment.id}`");
  });

  it("announces comment failures", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/CommentsPanel.tsx"), "utf8");

    expect(source).toContain('text-center text-sm" role="alert"');
  });

  it("announces the initial comment loading state", () => {
    const html = renderToStaticMarkup(<CommentsPanel spaceId="space-1" docId="doc-1" />);

    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain("Loading comments...");
  });

  it("pairs reaction failure context with its retry action", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/CommentsPanel.tsx"), "utf8");

    expect(source).toContain('className="flex items-center gap-2 text-xs" role="alert"');
    expect(source).toContain("Unable to load reactions.");
  });

  it("keeps document B comments when document A resolves after B", async () => {
    let resolveA!: (comments: DocumentComment[]) => void;
    let resolveB!: (comments: DocumentComment[]) => void;
    const requestA = new Promise<DocumentComment[]>((resolve) => {
      resolveA = resolve;
    });
    const requestB = new Promise<DocumentComment[]>((resolve) => {
      resolveB = resolve;
    });
    let currentRequestId = 0;
    let visibleComments: DocumentComment[] = [];
    let loading = false;
    let error = "";
    let completionCount = 0;

    const startRequest = (request: Promise<DocumentComment[]>) => {
      const requestId = ++currentRequestId;
      loading = true;
      error = "";
      return settleCurrentCommentsRequest(
        request,
        () => requestId === currentRequestId,
        (comments) => {
          visibleComments = comments;
        },
        () => {
          error = "Unable to load comments.";
        },
        () => {
          loading = false;
          completionCount += 1;
        },
      );
    };

    const loadA = startRequest(requestA);
    const loadB = startRequest(requestB);
    resolveB([makeComment("document-b", "B comment")]);
    await loadB;

    expect(visibleComments.map((comment) => comment.content)).toEqual(["B comment"]);
    expect(loading).toBe(false);
    expect(completionCount).toBe(1);

    resolveA([makeComment("document-a", "A comment")]);
    await loadA;

    expect(visibleComments.map((comment) => comment.content)).toEqual(["B comment"]);
    expect(loading).toBe(false);
    expect(error).toBe("");
    expect(completionCount).toBe(1);
  });

  it("keeps document B reactions when document A resolves after B", async () => {
    type TestReaction = { emoji: string; count: number; user_reacted: boolean };
    let resolveA!: (reactions: TestReaction[]) => void;
    let resolveB!: (reactions: TestReaction[]) => void;
    const requestA = new Promise<TestReaction[]>((resolve) => {
      resolveA = resolve;
    });
    const requestB = new Promise<TestReaction[]>((resolve) => {
      resolveB = resolve;
    });
    let currentRequestId = 0;
    let visibleReactions: TestReaction[] = [];
    let reactionsError = false;

    const startRequest = (request: Promise<TestReaction[]>) => {
      const requestId = ++currentRequestId;
      return settleCurrentCommentsRequest(
        request,
        () => requestId === currentRequestId,
        (reactions) => {
          visibleReactions = reactions;
        },
        () => {
          reactionsError = true;
        },
        () => undefined,
      );
    };

    const loadA = startRequest(requestA);
    const loadB = startRequest(requestB);
    const documentBReactions = [{ emoji: "👍", count: 1, user_reacted: true }];
    resolveB(documentBReactions);
    await loadB;
    resolveA([{ emoji: "❤️", count: 3, user_reacted: false }]);
    await loadA;

    expect(visibleReactions).toEqual(documentBReactions);
    expect(reactionsError).toBe(false);
  });

  it("ignores document A reaction failures after document B loads", async () => {
    type TestReaction = { emoji: string; count: number; user_reacted: boolean };
    let rejectA!: (error: Error) => void;
    let resolveB!: (reactions: TestReaction[]) => void;
    const requestA = new Promise<TestReaction[]>((_, reject) => {
      rejectA = reject;
    });
    const requestB = new Promise<TestReaction[]>((resolve) => {
      resolveB = resolve;
    });
    let currentRequestId = 0;
    let visibleReactions: TestReaction[] = [];
    let reactionsError = false;

    const startRequest = (request: Promise<TestReaction[]>) => {
      const requestId = ++currentRequestId;
      return settleCurrentCommentsRequest(
        request,
        () => requestId === currentRequestId,
        (reactions) => {
          visibleReactions = reactions;
        },
        () => {
          reactionsError = true;
        },
        () => undefined,
      );
    };

    const loadA = startRequest(requestA);
    const loadB = startRequest(requestB);
    const documentBReactions = [{ emoji: "🎉", count: 2, user_reacted: false }];
    resolveB(documentBReactions);
    await loadB;
    rejectA(new Error("stale document A failure"));
    await loadA;

    expect(visibleReactions).toEqual(documentBReactions);
    expect(reactionsError).toBe(false);
  });

  it("does not publish mutation completion after the active document changes", async () => {
    let resolveMutation!: () => void;
    const mutation = new Promise<void>((resolve) => {
      resolveMutation = resolve;
    });
    let activeScope = "space-1\u0000document-a";
    const requestScope = activeScope;
    let successCount = 0;
    let settledCount = 0;

    const pending = settleCurrentCommentsRequest(
      mutation,
      () => requestScope === activeScope,
      () => {
        successCount += 1;
      },
      () => undefined,
      () => {
        settledCount += 1;
      },
    );

    activeScope = "space-1\u0000document-b";
    resolveMutation();
    await pending;

    expect(successCount).toBe(0);
    expect(settledCount).toBe(0);
  });

  it("binds comment mutations to the active document scope", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/CommentsPanel.tsx"), "utf8");

    expect(source).toContain("const mutationRequestIdRef = useRef(0)");
    expect(source).toContain("requestScope === activeDocumentScopeRef.current");
    expect(source).toContain("if (isCurrent()) emitDocumentCommentsChanged(spaceId, docId)");
  });

  it("claims one mutation synchronously and releases only its owner", () => {
    const lock = { scope: "space-1\u0000document-a", action: "" };

    expect(claimCommentMutation(lock, lock.scope, "comment")).toBe(true);
    expect(claimCommentMutation(lock, lock.scope, "comment")).toBe(false);
    expect(claimCommentMutation(lock, lock.scope, "resolve:comment-1")).toBe(false);
    expect(releaseCommentMutation(lock, lock.scope, "resolve:comment-1")).toBe(false);
    expect(releaseCommentMutation(lock, lock.scope, "comment")).toBe(true);
    expect(claimCommentMutation(lock, lock.scope, "resolve:comment-1")).toBe(true);
  });

  it("does not carry a pending mutation lock into another document", () => {
    const lock = { scope: "space-1\u0000document-a", action: "comment" };

    expect(claimCommentMutation(lock, "space-1\u0000document-b", "comment")).toBe(true);
    expect(lock).toEqual({ scope: "space-1\u0000document-b", action: "comment" });
  });

  it("submits completed Enter presses without interrupting IME composition", () => {
    expect(shouldSubmitCommentOnEnter("Enter", false)).toBe(true);
    expect(shouldSubmitCommentOnEnter("Enter", true)).toBe(false);
    expect(shouldSubmitCommentOnEnter("Escape", false)).toBe(false);
  });

  it("prevents the default action before keyboard submission", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/CommentsPanel.tsx"), "utf8");

    expect(source.match(/event\.preventDefault\(\)/g)).toHaveLength(2);
    expect(source.match(/event\.nativeEvent\.isComposing/g)).toHaveLength(2);
  });
});
