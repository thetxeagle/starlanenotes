// The installed markdown-it package exports this exact ESM rule, but its
// existing @types package declares RuleBlock without individual rule modules.
declare module "markdown-it/lib/rules_block/reference.mjs" {
  import type { RuleBlock } from "markdown-it/lib/parser_block.mjs";
  const reference: RuleBlock;
  export default reference;
}

declare module "markdown-it/lib/rules_inline/autolink.mjs" {
  import type { RuleInline } from "markdown-it/lib/parser_inline.mjs";
  const autolink: RuleInline;
  export default autolink;
}
