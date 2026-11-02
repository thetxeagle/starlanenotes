import {renderToStaticMarkup} from "react-dom/server";
import {describe,expect,it} from "vitest";
import {DocumentPublicationHistory} from "./DocumentPublicationHistory";
import {snapshotSummary} from "@/lib/documentPublishedHistory.fixtures";
const callbacks={onRetry(){},onPreview(){}};
describe("published snapshot list state",()=>{
 it("distinguishes loading from true empty history and does not infer legacy absence",()=>{
  const loading=renderToStaticMarkup(<DocumentPublicationHistory status="loading" records={[]} {...callbacks}/>);expect(loading).toContain('Loading published snapshots');expect(loading).not.toContain('No published snapshots');
  const empty=renderToStaticMarkup(<DocumentPublicationHistory status="ready" records={[]} {...callbacks}/>);expect(empty).toContain('No published snapshots have been captured');expect(empty).toContain('Legacy change records may still be available');
 });
 it("hides stale captured title on denial and offers no futile permission retry or mutation",()=>{const html=renderToStaticMarkup(<DocumentPublicationHistory status="denied" records={[snapshotSummary]} {...callbacks}/>);expect(html).toContain('requires current Edit access');expect(html).not.toContain(snapshotSummary.content.title);expect(html).not.toContain('Retry snapshots');expect(html).not.toContain('No published snapshots');});
 it("keeps offline reconnect and unavailable retry distinct from an empty result",()=>{for(const status of ['offline','unavailable'] as const){const html=renderToStaticMarkup(<DocumentPublicationHistory status={status} records={[snapshotSummary]} {...callbacks}/>);expect(html).toContain('Retry snapshots');expect(html).not.toContain(snapshotSummary.content.title);expect(html).not.toContain('No published snapshots');}});
 it("provides an explicit read-only Preview action and distinguishes a later baseline capture",()=>{const baseline={...snapshotSummary,id:'op:baseline',kind:'baseline' as const,publication_operation_id:null};const html=renderToStaticMarkup(<DocumentPublicationHistory status="ready" records={[snapshotSummary,baseline]} {...callbacks}/>);expect(html.match(/Preview snapshot/g)?.length).toBe(2);expect(html).toContain('Previous published page captured before replacement');expect(html).toContain('Captured: capture-stamp');expect(html).not.toMatch(/<button[^>]*>\s*(Restore|Recover|Publish|Save)(?:\s|<)/);});
});
