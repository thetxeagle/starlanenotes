import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PageAccessReport } from "./PageShareDialog";

describe("page viewer presentation and menu ownership",()=>{
  it("renders actual direct page provenance without space membership, private draft or capability invention",()=>{
    const html=renderToStaticMarkup(<PageAccessReport report={{schema_version:1,space_id:"space",page_id:"page",checked_at:"2026-10-09T12:00:00Z",capabilities:{view:true,comment:false,edit:false,move:false,manage_lock:false,enable_public_link:false,disable_public_link:false},page_permissions:["view"],page_permission_sources:[{kind:"direct_page_viewer",grant_id:"grant",revision:"revision",permissions:["view"]}],space_permission_sources:[],restrictions:{local:"inherited",ancestor_restricted:false},public_link:{configured:false,policy_allows_anonymous:false,reason:"not_enabled"},selected_private_draft:{scope:"none",view:false,edit:false},sharing:{people_grants_available:true,guests_available:false}}}/>);
    expect(html).toContain("Direct published-page viewer grant");expect(html).toContain("No space permission contributions reported");expect(html).toContain("No accessible private draft reported");expect(html).toContain("Not allowed");expect(html).not.toContain("Instance administrator access");
  });
});
