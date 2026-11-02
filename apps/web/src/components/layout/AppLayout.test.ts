import { describe, expect, it } from "vitest";
import {
  prepareMobileSidebarMoveHandoff,
  getDesktopSidebarToggleLabel,
  getMobileSidebarCreateEvent,
  isDocumentPanelRoute,
} from "./AppLayout";
import {
  OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT,
  OPEN_SIDEBAR_CREATE_FROM_TEMPLATE_EVENT,
  OPEN_SIDEBAR_DAILY_NOTE_EVENT,
  OPEN_SIDEBAR_CREATE_FOLDER_EVENT,
} from "./Sidebar";
import { MOBILE_DOCUMENT_PANEL_CLASS } from "./MobileDocumentPanel";

describe("isDocumentPanelRoute", () => {
  it("allows the properties panel only on document routes", () => {
    expect(isDocumentPanelRoute("/spaces/space-1/docs/doc-1")).toBe(true);
    expect(isDocumentPanelRoute("/spaces/space-1/docs/doc-1/")).toBe(true);
  });

  it("blocks the properties panel on non-document app routes", () => {
    expect(isDocumentPanelRoute("/")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/docs")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/graph")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/journal")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/starred")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/workflows")).toBe(false);
    expect(isDocumentPanelRoute("/admin")).toBe(false);
    expect(isDocumentPanelRoute("/mobile/profile")).toBe(false);
  });
});

describe("getMobileSidebarCreateEvent", () => {
  it("keeps command-menu creation routed through the shared Files drawer actions", () => {
    expect(getMobileSidebarCreateEvent("document")).toBe(OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT);
    expect(getMobileSidebarCreateEvent("document-template")).toBe(
      OPEN_SIDEBAR_CREATE_FROM_TEMPLATE_EVENT,
    );
    expect(getMobileSidebarCreateEvent("folder")).toBe(OPEN_SIDEBAR_CREATE_FOLDER_EVENT);
    expect(getMobileSidebarCreateEvent("daily-note")).toBe(OPEN_SIDEBAR_DAILY_NOTE_EVENT);
  });
});

describe("desktop sidebar toggle", () => {
  it("keeps the compact-state labels explicit", () => {
    expect(getDesktopSidebarToggleLabel(false)).toBe("Collapse sidebar");
    expect(getDesktopSidebarToggleLabel(true)).toBe("Expand sidebar");
  });
});

describe("MOBILE_DOCUMENT_PANEL_CLASS", () => {
  it("anchors mobile document properties as a safe-area-aware bottom sheet", () => {
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("inset-x-0 bottom-0");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("h-[min(42rem,calc(100dvh-1rem))]");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("rounded-t-card");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("pb-[env(safe-area-inset-bottom)]");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).not.toContain("-translate");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("flex-col");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("overflow-hidden");
  });
});

describe("consumed mobile move handoff", () => {
  const original = { mobile: true, open: false, pathname: "/spaces/s/docs/page", spaceId: "s", docId: "page" };
  it("delivers the original page exactly once after drawer mount without recursive redispatch", () => {
    let target = {...original}; const received: string[] = [];
    const pending = prepareMobileSidebarMoveHandoff("page", () => target, () => {}, id => received.push(id));
    expect(pending).not.toBeNull(); expect(received).toEqual([]);
    target = {...target, open:true}; expect(pending?.deliver()).toBe(true); expect(pending?.deliver()).toBe(false);
    expect(received).toEqual(["page"]);
    expect(prepareMobileSidebarMoveHandoff("page", () => target, () => {}, () => received.push("loop"))).toBeNull();
  });
  it("leaves desktop/already-open/wrong-page/non-page actions to existing behavior", () => {
    for (const target of [{...original,mobile:false},{...original,open:true},{...original,docId:"other"},{...original,pathname:"/spaces/s/docs"}])
      expect(prepareMobileSidebarMoveHandoff("page", () => target, () => {}, () => {throw new Error("must not dispatch");})).toBeNull();
  });
  it("consumes stale route, closed drawer, switched viewport and account actions without delivery", () => {
    for (const changed of [{...original,open:true,spaceId:"foreign"},{...original,open:true,docId:"other"},{...original,open:true,pathname:"/spaces/s/docs/page/other"},{...original,open:false},{...original,open:true,mobile:false}]) {
      let target = {...original}; let deliveries=0;
      const pending=prepareMobileSidebarMoveHandoff("page",()=>target,()=>{},()=>{deliveries++;});
      target=changed; expect(pending?.deliver()).toBe(false); expect(deliveries).toBe(0);
    }
    let target={...original}; let current=true; let deliveries=0;
    const pending=prepareMobileSidebarMoveHandoff("page",()=>target,()=>{if(!current)throw new Error("account changed");},()=>{deliveries++;});
    target={...target,open:true};current=false;expect(pending?.deliver()).toBe(false);expect(deliveries).toBe(0);
  });
});
