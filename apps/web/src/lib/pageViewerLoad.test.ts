import { describe, expect, it, vi } from "vitest";
import { loadPageViewerContext } from "./pageViewerLoad";
function fixture(pageOnly: boolean) {
  return {
    metadata: vi.fn(async () => ({published_at:"published", ...(pageOnly ? {page_access_source:"direct_page_grant" as const} : {})})),
    content: vi.fn(async () => ({content:"published original"})),space:vi.fn(async () => ({name:"actual space"})),
    templates:vi.fn(async () => ["template"]),folders:vi.fn(async () => ["folder"]),documents:vi.fn(async () => ["document"]),drafts:vi.fn(async () => ["draft"]),
  };
}
describe("actual metadata-first page loader", () => {
  it("opens a direct published viewer without dispatching denied space, draft or catalog requests", async () => {
    const requests=fixture(true);requests.space.mockRejectedValue(new Error("space denied"));
    const result=await loadPageViewerContext(requests,vi.fn());
    expect(result.content).toEqual({content:"published original"});expect(result.space).toBeNull();expect(result.pageOnly).toBe(true);
    for(const request of [requests.space,requests.templates,requests.folders,requests.documents,requests.drafts]) expect(request).not.toHaveBeenCalled();
    expect(await result.supplementary).toEqual([[],[],[]]);expect(await result.drafts).toEqual([]);
  });
  it("preserves ordinary space failure without treating it as a grant", async () => {
    const requests=fixture(false);requests.space.mockRejectedValue(new Error("ordinary space denied"));
    await expect(loadPageViewerContext(requests,vi.fn())).rejects.toThrow("ordinary space denied");expect(requests.space).toHaveBeenCalledTimes(1);
  });
  it("preserves ordinary catalogs/drafts and refuses a guessed or unpublished marker", async () => {
    const requests=fixture(false);const result=await loadPageViewerContext(requests,vi.fn());
    expect(result.space).toEqual({name:"actual space"});expect(await result.drafts).toEqual(["draft"]);expect(await result.supplementary).toEqual([["template"],["folder"],["document"]]);
    const invalid=fixture(true);invalid.metadata.mockResolvedValue({published_at:"",page_access_source:"direct_page_grant"});
    await expect(loadPageViewerContext(invalid,vi.fn())).rejects.toThrow("unavailable");expect(invalid.content).not.toHaveBeenCalled();
  });
  it("checks original selection after metadata and body waits before adopting either response", async () => {
    const requests=fixture(true);let selected=true;requests.metadata.mockImplementation(async()=>{selected=false;return {published_at:"published",page_access_source:"direct_page_grant"};});
    await expect(loadPageViewerContext(requests,()=>{if(!selected)throw new Error("selection changed");})).rejects.toThrow("selection changed");expect(requests.content).not.toHaveBeenCalled();
    const body=fixture(true);selected=true;body.content.mockImplementation(async()=>{selected=false;return {content:"later body"};});
    await expect(loadPageViewerContext(body,()=>{if(!selected)throw new Error("selection changed");})).rejects.toThrow("selection changed");
  });
});
