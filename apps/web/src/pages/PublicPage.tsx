import { MarkdownPreview } from "@/components/editor/MarkdownPreview";
import { ApiError, api } from "@/lib/api";
import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";

interface PublicPageRecord {
  title: string;
  icon?: string | null;
  cover_image?: string | null;
  content: string;
  space_name: string;
  space_accent_color?: string | null;
  author: string;
  updated_at: string;
}

export function describePublicPageError(error: unknown): string {
  if (error instanceof ApiError && (error.status === 400 || error.status === 404)) {
    return "This public page is unavailable or has been unpublished.";
  }
  return "This public page could not be loaded. Please try again later.";
}

function formatPublicPageDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? "Recently" : date.toLocaleDateString();
}

export function PublicPage() {
  const { slug = "" } = useParams<{ slug: string }>();
  const [page, setPage] = useState<PublicPageRecord | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setPage(null);
    setError(null);

    api
      .fetch<PublicPageRecord>(`/p/${encodeURIComponent(slug)}`)
      .then((result) => {
        if (!cancelled) setPage(result);
      })
      .catch((reason) => {
        if (!cancelled) setError(describePublicPageError(reason));
      });

    return () => {
      cancelled = true;
    };
  }, [slug]);

  useEffect(() => {
    if (page?.title) document.title = page.title;
  }, [page?.title]);

  if (error) {
    return (
      <main className="app-background text-text flex min-h-screen items-center justify-center px-4 py-12">
        <section className="w-full max-w-xl rounded-card border border-border bg-surface p-4 shadow-card">
          <h1 className="text-base font-semibold">Public page unavailable</h1>
          <p className="mt-2 text-sm text-text-muted">{error}</p>
        </section>
      </main>
    );
  }

  if (!page) {
    return (
      <main className="app-background text-text flex min-h-screen items-center justify-center px-4 py-12">
        <p className="text-sm text-text-muted">Loading public page…</p>
      </main>
    );
  }

  return (
    <main className="app-background text-text min-h-screen px-4 py-8 sm:px-6 sm:py-12">
      <article className="mx-auto max-w-3xl">
        <header className="border-border mb-8 border-b pb-6">
          {page.space_name && <p className="text-sm text-text-muted">{page.space_name}</p>}
          <h1 className="mt-2 text-2xl font-semibold tracking-tight sm:text-3xl">{page.title}</h1>
          <p className="mt-3 text-xs text-text-muted">
            Published by {page.author} · Updated {formatPublicPageDate(page.updated_at)}
          </p>
        </header>
        <MarkdownPreview content={page.content} />
      </article>
    </main>
  );
}
