import {describe,it,expect} from "vitest";
import {coverAttachmentReferences,selectedPublishedMetadata} from "./publishedCover";
const id="11111111-1111-4111-8111-111111111111",base="https://owned.invalid/api/v1";
describe("exact selected cover references",()=>{
 it("accepts exact immutable pin and original same-server path only",()=>{
  expect(coverAttachmentReferences(`starlane-attachment://${id}`,base).get(`starlane-attachment://${id}`)).toBe(id);
  const path=`/api/v1/files/spaces/original/attachments/page/${id}.png`;
  expect(coverAttachmentReferences(path,base).get(path)).toBe(id);
  expect(()=>coverAttachmentReferences(`https://foreign.invalid${path}`,base)).toThrow("same-server");
 });
 it("refuses encoded or malformed managed scalar paths rather than exporting omission",()=>{
  for(const v of [`starlane-attachment://${id}?latest=1`,`/api/v1/files/spaces/s/attachments/page/name.png`,`/api/v1/files/spaces/s/attachments/page/${id}.png?token=1`])expect(()=>coverAttachmentReferences(v,base)).toThrow();
  expect(coverAttachmentReferences("https://images.invalid/ordinary.png",base).size).toBe(0);
 });
 it("requires selected metadata matching exact title and explicit nullable fields",()=>{
  const v={title:"Original",icon:null,cover_image:`starlane-attachment://${id}`};expect(selectedPublishedMetadata(v,"Original")).toEqual(v);
  for(const bad of [undefined,{title:"Original"},{...v,title:"Later"},{...v,cover_image:{}},{...v,latest:true}])expect(()=>selectedPublishedMetadata(bad,"Original")).toThrow();
 });
});
