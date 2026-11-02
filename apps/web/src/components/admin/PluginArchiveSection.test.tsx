import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PluginArchiveSection } from "./PluginArchiveSection";
import { PLUGIN_ARCHIVE_STORAGE_KEYS } from "@/lib/pluginInstallRecords";

afterEach(() => vi.unstubAllGlobals());
describe("retained plugin archive rendering", () => {
  it("exposes exact readable retained text without executing markup or mutating storage", () => {
    const values = [' [{"unknown":"<script>unsafe()</script>"}] ', "", "not JSON"];
    const setItem = vi.fn();
    const removeItem = vi.fn();
    vi.stubGlobal("window", {localStorage:{getItem:(key:string)=>values[PLUGIN_ARCHIVE_STORAGE_KEYS.findIndex(item=>item===key)]??null,setItem,removeItem}});
    const html = renderToStaticMarkup(<PluginArchiveSection/>);
    expect(html.match(/readonly=""/gi)).toHaveLength(3);
    expect(html).toContain("&lt;script&gt;unsafe()&lt;/script&gt;");
    expect(html).not.toContain("<script>");
    expect(html).toContain("not JSON");
    expect(html).toContain("stored empty string");
    expect(html).not.toContain("<button");
    expect(setItem).not.toHaveBeenCalled();
    expect(removeItem).not.toHaveBeenCalled();
  });
  it("distinguishes absent data from blocked reads", () => {
    vi.stubGlobal("window", {localStorage:{getItem:(key:string)=>{if(key===PLUGIN_ARCHIVE_STORAGE_KEYS[0])throw new Error("Denied");return null;}}});
    const html = renderToStaticMarkup(<PluginArchiveSection/>);
    expect(html).toContain("This does not mean the record is missing");
    expect(html).toContain("No stored value in this browser profile");
  });
});
