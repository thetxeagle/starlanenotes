import { Node, mergeAttributes } from "@tiptap/core";
import { NodeViewWrapper, ReactNodeViewRenderer, type ReactNodeViewProps } from "@tiptap/react";
import { BookOpenText, Braces } from "lucide-react";
import { Button } from "@/components/ui/button";

function replaceMarkerWithParagraph(props: ReactNodeViewProps) {
  const position = props.getPos();
  if (typeof position !== "number") return;
  const end = position + props.node.nodeSize;
  props.editor
    .chain()
    .focus()
    .insertContentAt({ from: position, to: end }, { type: "paragraph" })
    .setTextSelection(position + 1)
    .run();
}

function TemplateMarkerView(props: ReactNodeViewProps) {
  const { node } = props;
  const kind = String(node.attrs.kind || "opaque");

  if (kind === "guidance") {
    return (
      <NodeViewWrapper
        data-starlane-template-node="guidance"
        className="my-3 flex items-start gap-3 rounded-card border border-border bg-surface-raised px-3 py-2.5 text-sm text-text-muted"
        contentEditable={false}
      >
        <BookOpenText aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
        <div className="min-w-0 flex-1">
          <p className="m-0 font-medium text-text">Writing guidance</p>
          <p className="mb-2 mt-1 whitespace-pre-wrap">{String(node.attrs.text || "")}</p>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-auto min-h-0 rounded-button px-2 py-1 text-primary underline-offset-4 hover:underline"
            aria-label={`Replace writing guidance: ${String(node.attrs.text || "instruction")}`}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => replaceMarkerWithParagraph(props)}
          >
            Replace guidance
          </Button>
        </div>
      </NodeViewWrapper>
    );
  }

  if (kind === "field") {
    return (
      <NodeViewWrapper
        data-starlane-template-node="field"
        className="my-2 inline-flex max-w-full items-center gap-2 rounded-input border border-border bg-surface-raised px-2.5 py-1.5 text-sm text-text"
        contentEditable={false}
      >
        <Braces aria-hidden="true" className="h-4 w-4 shrink-0 text-primary" />
        <span className="truncate">{String(node.attrs.label || "Named input")}</span>
      </NodeViewWrapper>
    );
  }

  return (
    <NodeViewWrapper
      data-starlane-template-node="opaque"
      className="my-2 rounded-card border border-border bg-surface-raised px-3 py-2 text-xs text-text-muted"
      contentEditable={false}
      aria-label="Unrecognized template marker preserved in Markdown source"
    >
      Unrecognized template marker preserved in source
    </NodeViewWrapper>
  );
}

export const TemplateMarkerNode = Node.create({
  name: "templateMarker",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,
  addAttributes() {
    return {
      kind: { default: "opaque" },
      id: { default: "" },
      text: { default: "" },
      fieldId: { default: "" },
      label: { default: "" },
      raw: { default: "" },
    };
  },
  parseHTML() {
    return [
      { tag: 'div[data-starlane-template-marker="guidance"]', getAttrs: (element) => attrs("guidance", element) },
      { tag: 'div[data-starlane-template-marker="field"]', getAttrs: (element) => attrs("field", element) },
      { tag: 'div[data-starlane-template-marker="opaque"]', getAttrs: (element) => attrs("opaque", element) },
    ];
  },
  renderHTML({ node, HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, markerDomAttributes(node.attrs))];
  },
  addNodeView() {
    return ReactNodeViewRenderer(TemplateMarkerView);
  },
});

export const TemplateValueNode = Node.create({
  name: "templateValue",
  inline: true,
  group: "inline",
  content: "text*",
  defining: true,
  addAttributes() {
    return { version: { default: "v1" } };
  },
  parseHTML() {
    return [{ tag: 'span[data-starlane-template-value="v1"]', preserveWhitespace: "full" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-starlane-template-value": "v1",
        class: "whitespace-pre-wrap",
      }),
      0,
    ];
  },
});

function attrs(kind: string, element: HTMLElement) {
  return {
    kind,
    id: element.getAttribute("data-template-marker-id") || "",
    text: element.getAttribute("data-template-marker-text") || "",
    fieldId: element.getAttribute("data-template-marker-field-id") || "",
    label: element.getAttribute("data-template-marker-field-label") || "",
    raw: element.getAttribute("data-template-marker-raw") || "",
  };
}

function markerDomAttributes(attributes: Record<string, unknown>) {
  const kind = String(attributes.kind || "opaque");
  return {
    "data-starlane-template-marker": kind,
    ...(attributes.id ? { "data-template-marker-id": String(attributes.id) } : {}),
    ...(attributes.text ? { "data-template-marker-text": String(attributes.text) } : {}),
    ...(attributes.fieldId ? { "data-template-marker-field-id": String(attributes.fieldId) } : {}),
    ...(attributes.label ? { "data-template-marker-field-label": String(attributes.label) } : {}),
    ...(attributes.raw ? { "data-template-marker-raw": String(attributes.raw) } : {}),
  };
}
