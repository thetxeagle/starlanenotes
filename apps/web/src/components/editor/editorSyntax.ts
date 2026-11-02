import bash from "highlight.js/lib/languages/bash";
import css from "highlight.js/lib/languages/css";
import html from "highlight.js/lib/languages/xml";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import ocaml from "highlight.js/lib/languages/ocaml";
import powershell from "highlight.js/lib/languages/powershell";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import yaml from "highlight.js/lib/languages/yaml";
import type { createLowlight } from "lowlight";

type Lowlight = ReturnType<typeof createLowlight>;

/** Register only the grammars exposed by the code-block picker. */
export function registerEditorSyntax(lowlight: Lowlight): void {
  lowlight.register("bash", bash);
  lowlight.register("shell", bash);
  lowlight.register("sh", bash);
  lowlight.register("css", css);
  lowlight.register("html", html);
  lowlight.register("xml", html);
  lowlight.register("ocaml", ocaml);
  lowlight.register("ml", ocaml);
  lowlight.register("javascript", javascript);
  lowlight.register("js", javascript);
  lowlight.register("json", json);
  lowlight.register("markdown", markdown);
  lowlight.register("md", markdown);
  lowlight.register("powershell", powershell);
  lowlight.register("ps1", powershell);
  lowlight.register("python", python);
  lowlight.register("py", python);
  lowlight.register("rust", rust);
  lowlight.register("rs", rust);
  lowlight.register("sql", sql);
  lowlight.register("typescript", typescript);
  lowlight.register("ts", typescript);
  lowlight.register("tsx", typescript);
  lowlight.register("yaml", yaml);
  lowlight.register("yml", yaml);
}
