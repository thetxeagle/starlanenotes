/** Only server-issued page provenance selects the page-only loading branch. */
export interface PageViewerMetadata { published_at?: string; page_access_source?: "direct_page_grant" }
export async function loadPageViewerContext<M extends PageViewerMetadata, C, S, T, F, D, R>(requests: {
  metadata(): Promise<M>; content(): Promise<C>; space(): Promise<S>;
  templates(): Promise<T[]>; folders(): Promise<F[]>; documents(): Promise<D[]>; drafts(): Promise<R[]>;
}, verifyCurrent: () => void) {
  const metadata = await requests.metadata();
  verifyCurrent();
  const pageOnly = metadata.page_access_source === "direct_page_grant";
  if (pageOnly && !metadata.published_at) throw new Error("Shared published page unavailable.");
  // No supplementary space/draft request is dispatched on the direct-page branch.
  const supplementary = pageOnly ? Promise.resolve<[T[], F[], D[]]>([[], [], []]) :
    Promise.all([requests.templates().catch(() => []), requests.folders().catch(() => []), requests.documents().catch(() => [])]);
  const drafts = pageOnly ? Promise.resolve<R[] | null>([]) : requests.drafts().catch(() => null);
  const [content, space] = await Promise.all([requests.content(), pageOnly ? Promise.resolve(null) : requests.space()]);
  verifyCurrent();
  return { metadata, content, space, pageOnly, supplementary, drafts };
}
