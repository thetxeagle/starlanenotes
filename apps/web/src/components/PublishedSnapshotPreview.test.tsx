import {renderToStaticMarkup} from "react-dom/server";
import {describe,expect,it} from "vitest";
import {PublishedSnapshotPreview,SnapshotSource} from "./PublishedSnapshotPreview";
import {snapshotDetail} from "@/lib/documentPublishedHistory.fixtures";
const callbacks={onClose(){},onReadRaw(){},onDownloadRaw(){}};
const render=(detail=structuredClone(snapshotDetail))=>renderToStaticMarkup(<PublishedSnapshotPreview detail={detail} raw={null} loading={false} rawLoading={false} {...callbacks}/>);
describe("read-only published snapshot preview",()=>{
 it("displays escaped exact source and presentation without fetching remote covers, attachments, CSS or embedded HTML",()=>{
  const html=render();expect(html).toContain(' Exact 🦅 title ');expect(html).toContain('  icon  ');expect(html).toContain('https://remote.invalid/cover');expect(html).toContain('&lt;script&gt;never()&lt;/script&gt;');expect(html).toContain('# Exact heading\r\n  trailing  ');expect(html).not.toMatch(/<(img|iframe|script|video|audio|link)\b/);expect(html).not.toContain('href=');expect(html).not.toContain('dangerouslySetInnerHTML');
 });
 it("keeps captured properties/reference observations comparison-only without mutation or user/relation expansion",()=>{
  const html=render();expect(html).toContain('comparison only');expect(html).toContain('future_unknown');expect(html).toContain('private-user-reference');expect(html).toContain('deleted-property');expect(html).toContain('bytes not included');expect(html).toContain('Availability is not checked');expect(html).not.toMatch(/<(input|select|textarea)\b/);expect(html).not.toMatch(/<button[^>]*>\s*(Publish|Restore|Recover|Apply|Save)(?:\s|<)/);
 });
 it("never attributes a later baseline capture as the original publisher or its comparison-time metadata",()=>{
  const detail={...snapshotDetail,kind:'baseline' as const,publication_operation_id:null};const html=render(detail);expect(html).toContain('original publisher is not recorded');expect(html).toContain('observed at capture');expect(html).toContain('Published:');expect(html).toContain('published-stamp');expect(html).toContain('Captured:');expect(html).toContain('capture-stamp');
 });
 it("shows an unproven legacy header verbatim after explicit raw read, without guessing a portable body",()=>{
  const detail={...snapshotDetail,content:{...snapshotDetail.content,extraction:'raw_only' as const,portable_markdown:null}};
  const text='---\r\ntitle: Ambiguous\r\ntitle: Duplicate\r\n---\r\n\r\n  raw body  ';
  const html=renderToStaticMarkup(<PublishedSnapshotPreview detail={detail} raw={{bytes:new TextEncoder().encode(text),text}} loading={false} rawLoading={false} {...callbacks}/>);
  expect(html).toContain('Original bytes are preserved');expect(html).toContain(text);expect(html).not.toContain('Captured portable Markdown');expect(html).toContain('Download preserved Markdown');
 });
 it("does not expose stale source while loading or denied, and preserves non-text bytes as a download-only state",()=>{
  for(const state of [{loading:true,error:undefined},{loading:false,error:'Current Edit access required'}]){const html=renderToStaticMarkup(<PublishedSnapshotPreview detail={snapshotDetail} raw={null} rawLoading={false} {...state} {...callbacks}/>);expect(html).not.toContain('Exact heading');expect(html).not.toContain('private-user-reference');expect(html).not.toContain('Download preserved Markdown');}
  const detail={...snapshotDetail,content:{...snapshotDetail.content,extraction:'raw_only' as const,portable_markdown:null}};const html=renderToStaticMarkup(<PublishedSnapshotPreview detail={detail} raw={{bytes:new Uint8Array([255]),text:null}} loading={false} rawLoading={false} {...callbacks}/>);expect(html).toContain('not valid UTF-8 text');expect(html).not.toContain('�');expect(html).toContain('Download preserved Markdown');
 });
 it("renders empty source as an actual empty captured value without inserting new body text",()=>{const html=renderToStaticMarkup(<SnapshotSource label="Exact source" value=""/>);expect(html).toMatch(/<pre[^>]*><\/pre>/);expect(html).not.toContain('Empty body');});
});

describe("optional reviewed recovery affordance",()=>{
 it("offers a separate draft review action with honest exclusions and no automatic activation",()=>{const html=renderToStaticMarkup(<PublishedSnapshotPreview detail={snapshotDetail} raw={null} loading={false} rawLoading={false} {...callbacks} onRecover={()=>{}}/>);expect(html).toContain("Recover into a private draft");expect(html).toContain("Properties, audience and asset bytes are not copied");expect(html).toContain("current editor is kept unchanged");expect(html).not.toMatch(/<button[^>]*>\s*(Publish|Restore|Apply|Save)(?:\s|<)/);});
 it("retains download with explicit raw-only and bounds exclusions instead of offering guessed draft content",()=>{for(const content of [{...snapshotDetail.content,extraction:"raw_only" as const,portable_markdown:null},{...snapshotDetail.content,title:"🦅".repeat(126)},{...snapshotDetail.content,portable_markdown:"x".repeat(5_242_881)}]){const html=renderToStaticMarkup(<PublishedSnapshotPreview detail={{...snapshotDetail,content}} raw={null} loading={false} rawLoading={false} {...callbacks} onRecover={()=>{}}/>);expect(html).not.toContain("Recover into a private draft");expect(html).toContain("Download preserved Markdown");expect(html).toMatch(/cannot be safely extracted|outside supported draft bounds|exceeds supported draft bounds/);}});
 it("never offers recovery from loading or denied preview source",()=>{for(const state of [{loading:true,error:undefined},{loading:false,error:"Current Edit required"}]){const html=renderToStaticMarkup(<PublishedSnapshotPreview detail={snapshotDetail} raw={null} rawLoading={false} {...state} {...callbacks} onRecover={()=>{}}/>);expect(html).not.toContain("Recover into a private draft");}});
});
