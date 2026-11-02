import { useCallback, useEffect, useRef, useState } from "react";
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  Home,
  List,
  Moon,
  Pencil,
  Plus,
  Search,
  Settings,
  Star,
  Sun,
  X,
  Clock,
  PanelRight,
  MoreHorizontal,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  DialogShell,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
  DialogCloseAction,
} from "@/components/ui/dialog-shell";
import { htmlToMarkdown, type TipTapEditor } from "@/components/editor/Editor";
import { PrototypeEditor } from "./PrototypeEditor";
import { WORKSPACE_FIXTURE_CONTENT, WORKSPACE_FIXTURE_TREE } from "./fixture";

const fixtureContent = WORKSPACE_FIXTURE_CONTENT.replace(
  "/workspace-prototype-architecture.svg",
  new URL("/workspace-prototype-architecture.svg", window.location.origin).href,
);

type Heading = { id: string; title: string; level: number; element: HTMLElement };
type TreeNode = {
  type: string;
  title: string;
  selected?: boolean;
  id?: string;
  children?: readonly TreeNode[];
};
type Inspector = "Properties" | "Backlinks" | "History" | "Comments";

function TreeRow({
  node,
  depth = 0,
  onOpen,
}: {
  node: TreeNode;
  depth?: number;
  onOpen: (title: string) => void;
}) {
  const [expanded, setExpanded] = useState(true);
  return (
    <li>
      <Button
        variant="ghost"
        className={`prototype-tree-row ${node.selected ? "is-selected" : ""}`}
        style={{ paddingLeft: 12 + depth * 14 }}
        aria-current={node.selected ? "page" : undefined}
        aria-expanded={node.children ? expanded : undefined}
        title={node.title}
        onClick={() => (node.children ? setExpanded(!expanded) : onOpen(node.title))}
      >
        {node.children ? (
          expanded ? (
            <ChevronDown />
          ) : (
            <ChevronRight />
          )
        ) : (
          <span className="prototype-tree-spacer" />
        )}
        {node.type === "folder" ? <Folder /> : <FileText />}
        <span>{node.title}</span>
      </Button>
      {expanded && node.children && (
        <ul>
          {node.children.map((child) => (
            <TreeRow key={child.title} node={child} depth={depth + 1} onOpen={onOpen} />
          ))}
        </ul>
      )}
    </li>
  );
}

export function WorkspacePrototype() {
  const [content, setContent] = useState(fixtureContent);
  const [published, setPublished] = useState(fixtureContent);
  const [saved, setSaved] = useState(fixtureContent);
  const [editing, setEditing] = useState(false);
  const [theme, setTheme] = useState<"light" | "dark">("light");
  const [contentsOpen, setContentsOpen] = useState(() => window.innerWidth >= 1440);
  const [inspector, setInspector] = useState<Inspector | null>(null);
  const [headings, setHeadings] = useState<Heading[]>([]);
  const [activeHeading, setActiveHeading] = useState("");
  const [dialog, setDialog] = useState<
    "search" | "create" | "actions" | "workspace" | "preview" | null
  >(null);
  const [query, setQuery] = useState("");
  const [previewTitle, setPreviewTitle] = useState("");
  const [newTitle, setNewTitle] = useState("");
  const [destination, setDestination] = useState("Offline Workspace and Data Lifecycle");
  const [tree, setTree] = useState<readonly TreeNode[]>(WORKSPACE_FIXTURE_TREE);
  const [favorite, setFavorite] = useState(false);
  const [notice, setNotice] = useState("");
  const [toolbarTarget, setToolbarTarget] = useState<HTMLDivElement | null>(null);
  const [editor, setEditor] = useState<TipTapEditor | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inspectorButtonRef = useRef<HTMLButtonElement>(null);
  const inspectorCloseRef = useRef<HTMLButtonElement>(null);
  const modeScrollRef = useRef(0);
  const initializedRef = useRef(false);
  const onEditorReady = useCallback((instance: TipTapEditor) => {
    setEditor(instance);
    if (!initializedRef.current) {
      initializedRef.current = true;
      const normalized = htmlToMarkdown(instance.getHTML());
      setContent(normalized);
      setSaved(normalized);
      setPublished(normalized);
    }
  }, []);
  const onChange = useCallback((value: string) => setContent(value), []);
  const openPreview = useCallback((title: string) => {
    setPreviewTitle(title);
    setDialog("preview");
  }, []);
  const onDocumentNavigate = useCallback(
    () => openPreview("Retry and reconciliation"),
    [openPreview],
  );

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);
  useEffect(() => {
    const wide = window.matchMedia("(min-width: 1440px)");
    const adapt = () => setContentsOpen(wide.matches);
    adapt();
    wide.addEventListener("change", adapt);
    return () => wide.removeEventListener("change", adapt);
  }, []);
  useEffect(() => {
    if (inspector) inspectorCloseRef.current?.focus();
  }, [inspector]);
  const closeInspector = () => {
    setInspector(null);
    inspectorButtonRef.current?.focus({ preventScroll: true });
  };

  useEffect(() => {
    const scroll = scrollRef.current;
    if (!scroll || !editor) return;
    let frame = 0;
    const collect = () => {
      const elements = Array.from(
        scroll.querySelectorAll<HTMLElement>(
          ".tiptap h1, .tiptap h2, .tiptap h3, .tiptap h4, .tiptap h5, .tiptap h6",
        ),
      );
      const entries = elements
        .filter((element) => element.textContent?.trim())
        .map((element, index) => {
          const id = `prototype-heading-${index}`;
          element.id = id;
          return {
            id,
            title: element.textContent!.trim(),
            level: Number(element.tagName.slice(1)),
            element,
          };
        });
      setHeadings(entries);
    };
    frame = requestAnimationFrame(collect);
    const update = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(collect);
    };
    editor.on("update", update);
    const observer = new MutationObserver(update);
    const canvas = scroll.querySelector(".prototype-editor");
    if (canvas) observer.observe(canvas, { childList: true, subtree: true, characterData: true });
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
      editor.off("update", update);
    };
  }, [editor, editing, content, published]);

  useEffect(() => {
    const scroll = scrollRef.current;
    if (!scroll) return;
    const track = () => {
      const edge = scroll.getBoundingClientRect().top + 32;
      const current =
        headings
          .filter(
            (heading) =>
              heading.element.isConnected && heading.element.getBoundingClientRect().top <= edge,
          )
          .slice(-1)[0] ?? headings[0];
      if (current) setActiveHeading(current.id);
    };
    scroll.addEventListener("scroll", track, { passive: true });
    const frame = requestAnimationFrame(track);
    return () => {
      cancelAnimationFrame(frame);
      scroll.removeEventListener("scroll", track);
    };
  }, [headings, editing]);

  const changeMode = (next: boolean) => {
    modeScrollRef.current = scrollRef.current?.scrollTop ?? 0;
    setEditing(next);
    requestAnimationFrame(() => {
      if (scrollRef.current) scrollRef.current.scrollTop = modeScrollRef.current;
    });
  };
  const goToHeading = (heading: Heading) => {
    const scroll = scrollRef.current;
    if (!scroll) return;
    const element =
      scroll.querySelector<HTMLElement>(`#${heading.id}`) ??
      scroll.querySelectorAll<HTMLElement>(
        ".tiptap h1, .tiptap h2, .tiptap h3, .tiptap h4, .tiptap h5, .tiptap h6",
      )[Number(heading.id.replace("prototype-heading-", ""))];
    if (!element) return;
    scroll.scrollTop +=
      element.getBoundingClientRect().top - scroll.getBoundingClientRect().top - 28;
    setActiveHeading(heading.id);
  };
  const dirty = content !== saved;
  const unpublished = content !== published;
  const filtered = headings.filter((heading) =>
    heading.title.toLowerCase().includes(query.toLowerCase()),
  );

  return (
    <div className="workspace-prototype">
      <aside className="prototype-sidebar" aria-label="Workspace navigation">
        <div className="prototype-brand">
          <BookOpen size={22} />
          <span>Starlane Notes</span>
        </div>
        <Button
          variant="ghost"
          className="prototype-workspace-switch"
          onClick={() => setDialog("workspace")}
        >
          <span>
            <strong>Platform Engineering</strong>
            <small>Example workspace</small>
          </span>
          <ChevronDown size={16} />
        </Button>
        <nav className="prototype-main-nav" aria-label="Main navigation">
          <Button variant="ghost" onClick={() => setDialog("search")}>
            <Search />
            Search<span className="prototype-shortcut">⌘ K</span>
          </Button>
          <Button variant="ghost" onClick={() => openPreview("Home")}>
            <Home />
            Home
          </Button>
          <Button variant="ghost" onClick={() => openPreview("Favorites")}>
            <Star />
            Favorites
          </Button>
          <Button variant="ghost" onClick={() => openPreview("Recent pages")}>
            <Clock />
            Recent
          </Button>
        </nav>
        <div className="prototype-pages-label">
          <span>Pages</span>
          <Button
            variant="ghost"
            aria-label="New page in selected folder"
            title="New page in Offline Workspace and Data Lifecycle"
            onClick={() => setDialog("create")}
          >
            <Plus size={16} />
          </Button>
        </div>
        <nav className="prototype-tree" aria-label="Pages">
          <ul>
            {tree.map((node) => (
              <TreeRow
                key={node.title}
                node={node}
                onOpen={(title) =>
                  title === "Operating the offline workspace"
                    ? scrollRef.current?.scrollTo({ top: 0 })
                    : openPreview(title)
                }
              />
            ))}
            <TreeRow
              node={{
                type: "page",
                title: "Architecture decisions",
                children: [{ type: "page", title: "ADR 004: Durable local writes" }],
              }}
              onOpen={openPreview}
            />
          </ul>
        </nav>
        <div className="prototype-sidebar-footer">
          <Button
            variant="ghost"
            onClick={() => openPreview("Specialist tools: Graph, Journal and Workflows")}
          >
            <MoreHorizontal />
            More tools
          </Button>
          <Button variant="ghost" onClick={() => openPreview("Workspace settings")}>
            <Settings />
            Workspace settings
          </Button>
          <p>Fixture content · no live service</p>
        </div>
      </aside>
      <main className="prototype-main">
        <div className="prototype-review-bar">
          <span>
            <strong>Workspace prototype</strong> · visual review
          </span>
          <span>Changes stay in this tab</span>
          <Button
            variant="ghost"
            aria-label={theme === "light" ? "Switch to dark theme" : "Switch to light theme"}
            onClick={() => setTheme(theme === "light" ? "dark" : "light")}
          >
            {theme === "light" ? <Moon size={16} /> : <Sun size={16} />}
          </Button>
        </div>
        <header className="prototype-page-header">
          <nav aria-label="Breadcrumb">
            <span>Platform Operations</span>
            <ChevronRight size={14} />
            <span>Offline Workspace</span>
          </nav>
          <div className="prototype-page-actions">
            <Button
              variant="ghost"
              aria-label="Contents"
              aria-pressed={contentsOpen}
              onClick={() => setContentsOpen(!contentsOpen)}
            >
              <List size={17} />
              <span>Contents</span>
            </Button>
            <Button
              ref={inspectorButtonRef}
              variant="ghost"
              aria-label="Page details"
              aria-expanded={!!inspector}
              onClick={() => (inspector ? closeInspector() : setInspector("Properties"))}
            >
              <PanelRight size={17} />
              <span>Details</span>
            </Button>
            <Button variant="ghost" aria-label="Page actions" onClick={() => setDialog("actions")}>
              <MoreHorizontal size={17} />
            </Button>
            <Button
              variant={editing ? "secondary" : "primary"}
              onClick={() => changeMode(!editing)}
            >
              {editing ? <BookOpen size={16} /> : <Pencil size={16} />}
              {editing ? "Read" : "Edit"}
            </Button>
            {editing && (
              <Button
                disabled={!unpublished}
                onClick={() => {
                  setPublished(content);
                  setSaved(content);
                  setNotice(
                    "Published to the example revision in this tab. No audience or access changed.",
                  );
                }}
              >
                Publish example
              </Button>
            )}
          </div>
        </header>
        <div className="prototype-state-bar">
          <span>
            {editing ? "Edit · example owner draft" : "Read · example published revision"}
          </span>
          <span>
            {editing
              ? dirty
                ? "Unsaved example changes"
                : "Example draft held in this tab"
              : "Example access: workspace members"}
            {editing && unpublished ? " · Unpublished changes" : ""}
          </span>
          {editing && (
            <Button
              variant="ghost"
              size="sm"
              disabled={!dirty}
              onClick={() => {
                setSaved(content);
                setNotice("Example draft saved in tab memory. Reload resets the fixture.");
              }}
            >
              Save example
            </Button>
          )}
        </div>
        <div className="prototype-toolbar-host" ref={setToolbarTarget} />
        <div className="prototype-scroll" ref={scrollRef}>
          <div
            className="prototype-composition"
            data-contents-open={contentsOpen}
            data-inspector-open={!!inspector}
          >
            <article className="prototype-document" aria-label="Document workspace">
              <div className="prototype-document-identity">
                <p className="prototype-location">Engineering handbook</p>
                <h1>Operating the offline workspace</h1>
                <p className="prototype-metadata">
                  Technical reference · Sample author · 8 min read
                </p>
              </div>
              <PrototypeEditor
                toolbarTarget={toolbarTarget}
                content={editing ? content : published}
                editable={editing}
                onChange={onChange}
                onEditorReady={onEditorReady}
                onDocumentNavigate={onDocumentNavigate}
              />
            </article>
            <aside
              className="prototype-contents"
              aria-label="Contents navigation"
              hidden={!contentsOpen || headings.length < 3}
            >
              <div className="prototype-contents-inner">
                <h2>On this page</h2>
                <nav>
                  {headings.map((heading) => (
                    <Button
                      key={heading.id}
                      variant="ghost"
                      className={activeHeading === heading.id ? "is-current" : ""}
                      style={{ paddingLeft: heading.level > 2 ? 20 : 8 }}
                      aria-current={activeHeading === heading.id ? "location" : undefined}
                      onClick={() => goToHeading(heading)}
                    >
                      {heading.title}
                    </Button>
                  ))}
                </nav>
              </div>
            </aside>
          </div>
        </div>
        {inspector && (
          <aside
            className="prototype-inspector"
            aria-label="Page inspector"
            onKeyDown={(event) => {
              if (event.key === "Escape") closeInspector();
            }}
          >
            <header>
              <h2>Page details</h2>
              <Button
                ref={inspectorCloseRef}
                variant="ghost"
                aria-label="Close page details"
                onClick={closeInspector}
              >
                <X size={18} />
              </Button>
            </header>
            <nav aria-label="Inspector sections">
              {(["Properties", "Backlinks", "History", "Comments"] as Inspector[]).map((tab) => (
                <Button
                  key={tab}
                  variant="ghost"
                  aria-pressed={tab === inspector}
                  onClick={() => setInspector(tab)}
                >
                  {tab}
                </Button>
              ))}
            </nav>
            <div className="prototype-inspector-body">
              <h3>{inspector}</h3>
              {inspector === "Properties" ? (
                <>
                  <dl>
                    <dt>Type</dt>
                    <dd>Technical reference</dd>
                    <dt>Folder</dt>
                    <dd>Offline Workspace and Data Lifecycle</dd>
                    <dt>Example access</dt>
                    <dd>Workspace members</dd>
                    <dt>Draft role</dt>
                    <dd>Example owner</dd>
                  </dl>
                  <p>
                    Access is separate from publishing. These values illustrate placement; they
                    grant no permissions.
                  </p>
                </>
              ) : inspector === "Backlinks" ? (
                <>
                  <Button
                    variant="ghost"
                    onClick={() => openPreview("Release readiness and rollback procedures")}
                  >
                    Release readiness and rollback procedures
                  </Button>
                  <p>One fixture backlink. No live index queried.</p>
                </>
              ) : inspector === "History" ? (
                <>
                  <p>
                    Example published revision and a separate working draft are held in this tab.
                  </p>
                  <p>There is no server history or restore operation in this prototype.</p>
                </>
              ) : (
                <p>
                  No example comments. Live comments are unavailable in this isolated prototype.
                </p>
              )}
            </div>
          </aside>
        )}
        <div className="prototype-live-status" role="status">
          {notice}
        </div>
      </main>
      {dialog && (
        <DialogShell labelledBy="prototype-dialog-title" onClose={() => setDialog(null)}>
          <DialogHeader>
            <DialogTitle id="prototype-dialog-title">
              {dialog === "search"
                ? "Search example workspace"
                : dialog === "create"
                  ? "New example page"
                  : dialog === "actions"
                    ? "Example page actions"
                    : dialog === "workspace"
                      ? "Example workspace"
                      : previewTitle}
            </DialogTitle>
            <DialogCloseAction onClick={() => setDialog(null)} />
          </DialogHeader>
          <DialogBody>
            {dialog === "search" ? (
              <>
                <Input
                  aria-label="Search fixture headings"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Search this document…"
                />
                <p className="prototype-dialog-note">
                  Fixture search searches this document's current headings.
                </p>
                {filtered.map((heading) => (
                  <Button
                    variant="ghost"
                    key={heading.id}
                    className="prototype-search-result"
                    onClick={() => {
                      setDialog(null);
                      goToHeading(heading);
                    }}
                  >
                    {heading.title}
                  </Button>
                ))}
                {filtered.length === 0 && <p>No matching fixture sections.</p>}
              </>
            ) : dialog === "create" ? (
              <>
                <label htmlFor="prototype-new-title">Page title</label>
                <Input
                  id="prototype-new-title"
                  value={newTitle}
                  onChange={(event) => setNewTitle(event.target.value)}
                  placeholder="Name your example page"
                />
                <label htmlFor="prototype-new-destination">Destination</label>
                <Select
                  id="prototype-new-destination"
                  value={destination}
                  onChange={(event) => setDestination(event.target.value)}
                >
                  <option>Offline Workspace and Data Lifecycle</option>
                  <option>Operating the offline workspace (child page)</option>
                  <option>Space root</option>
                </Select>
                <p className="prototype-dialog-note">
                  Creates an example navigation row only. No page is saved to a service.
                </p>
              </>
            ) : dialog === "actions" ? (
              <>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setFavorite(!favorite);
                    setDialog(null);
                    setNotice(favorite ? "Removed example favorite." : "Added example favorite.");
                  }}
                >
                  {favorite ? "Remove example favorite" : "Favorite example page"}
                </Button>
                <p className="prototype-dialog-note">
                  Drafts, move, duplicate, source, archive and access keep their existing production
                  owners. This prototype does not execute those operations.
                </p>
              </>
            ) : dialog === "workspace" ? (
              <p>
                Platform Engineering is the populated example space. Workspace switching and account
                actions retain their existing production behavior.
              </p>
            ) : (
              <>
                <p>
                  This is a labeled fixture destination. The full technical document stays selected
                  so visual comparisons use the same content.
                </p>
                <p className="prototype-dialog-note">
                  No navigation request or document write has been sent.
                </p>
              </>
            )}
          </DialogBody>
          {dialog === "create" && (
            <DialogFooter>
              <Button variant="secondary" onClick={() => setDialog(null)}>
                Cancel
              </Button>
              <Button
                disabled={!newTitle.trim()}
                onClick={() => {
                  const row: TreeNode = { type: "page", title: newTitle.trim() };
                  const parentTitle =
                    destination === "Operating the offline workspace (child page)"
                      ? "Operating the offline workspace"
                      : destination;
                  const append = (nodes: readonly TreeNode[]): TreeNode[] =>
                    nodes.map((node) =>
                      node.title === parentTitle
                        ? { ...node, children: [...(node.children ?? []), row] }
                        : node.children
                          ? { ...node, children: append(node.children) }
                          : node,
                    );
                  setTree(destination === "Space root" ? [...tree, row] : append(tree));
                  setNotice(
                    `Example row added for ${newTitle.trim()}; selected destination: ${destination}.`,
                  );
                  setNewTitle("");
                  setDialog(null);
                }}
              >
                Create example row
              </Button>
            </DialogFooter>
          )}
        </DialogShell>
      )}
    </div>
  );
}
