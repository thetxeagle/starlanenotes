import { rewriteExactAssetReferences } from "./nativeArchiveReferences";
function legacySource(reference: string, serverBase: string): URL {
  // A root-relative stored path carries no separate network authority. This
  // placeholder is parsing-only; every byte request uses the captured server.
  if (reference.startsWith("/api/v1/files/")) return new URL(reference,"https://relative-reference.invalid");
  let base: URL;
  try { base=new URL(serverBase,typeof location!=="undefined"?location.href:undefined); }
  catch {throw new Error("Archive original same-server URL authority unavailable");}
  const source=new URL(reference,base);
  if(source.origin!==base.origin || source.username || source.password)throw new Error("Archive legacy attachment requires original same-server exact ownership");
  return source;
}
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
/** Semantic exact immutable pins and original same-server v1 file UUIDs; no filename/head resolution. */
export function portableAttachmentReferences(content: string, serverBase: string): Map<string,string> {
  const result = new Map<string,string>();
  const candidates = new Set(content.match(/(?:starlane-attachment:\/\/[^\s<>()"'\\]+|(?:(?:https?:)?\/\/[^\s<>()"'\\]*?)?\/api\/v1\/files\/[^\s<>()"'\\]+)/g) ?? []);
  for (const reference of candidates) {
    if (rewriteExactAssetReferences(content,new Map([[reference,"owned-export-reference"]]))===content) continue;
    let id:string;
    if(reference.startsWith("starlane-attachment://")) id=reference.slice("starlane-attachment://".length);
    else {
      const source=legacySource(reference,serverBase);
      if(source.search || source.hash || !source.pathname.startsWith("/api/v1/files/"))
        throw new Error("Archive legacy attachment requires original same-server exact ownership");
      const stored=decodeURIComponent(source.pathname.slice("/api/v1/files/".length));
      const match=/^spaces\/[^/]+\/attachments\/[^/]+\/([0-9a-f-]{36})\.[A-Za-z0-9]{1,16}$/.exec(stored);
      if(!match)throw new Error("Archive legacy attachment requires exact supported storage receipt");
      id=match[1];
    }
    if(!uuid.test(id))throw new Error("Archive attachment reference requires exact immutable UUID ownership");
    result.set(reference,id);
  }
  return result;
}

/** A legacy UUID filename is only a lookup hint; the complete original path must match its immutable receipt. */
export function verifyPortableAttachmentReference(reference:string,rawVersion:string,serverBase:string):void {
  if(reference.startsWith("starlane-attachment://"))return;
  const source=legacySource(reference,serverBase),version=JSON.parse(rawVersion),stored=decodeURIComponent(source.pathname.slice("/api/v1/files/".length));
  if(source.search || source.hash || typeof version.original_storage_path!=="string" ||
    stored!==version.original_storage_path || typeof version.original_filename!=="string" || stored.slice(stored.lastIndexOf("/")+1)!==version.original_filename ||
    !version.original_filename.startsWith(version.version_id+"."))
    throw new Error("Archive legacy reference differs from immutable original storage receipt");
}
