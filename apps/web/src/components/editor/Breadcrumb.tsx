import { ChevronRight, MoreHorizontal } from "lucide-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Link } from "react-router-dom";

export interface ParentPageBreadcrumb {
  id: string;
  title: string;
  href: string;
}

interface BreadcrumbProps {
  spaceName: string;
  folderPath?: string[];
  parentPages?: ParentPageBreadcrumb[];
  title: string;
  spaceHref?: string;
  className?: string;
  showSpace?: boolean;
}

export function buildBreadcrumbLabels(
  spaceName: string,
  folderPath: string[] = [],
  title: string,
  parentPages: ParentPageBreadcrumb[] = [],
): string[] {
  const spaceLabel = spaceName.trim() || "Space";
  const folderLabels = folderPath.map((folder) => folder.trim()).filter(Boolean);
  const titleLabel = title.trim() || "Untitled";
  return [
    spaceLabel,
    ...folderLabels,
    ...parentPages.map((page) => page.title.trim() || "Untitled"),
    titleLabel,
  ];
}

export function Breadcrumb({
  spaceName,
  folderPath = [],
  parentPages = [],
  title,
  spaceHref,
  className = "mb-4",
  showSpace = true,
}: BreadcrumbProps) {
  const crumbs = buildBreadcrumbLabels(spaceName, folderPath, title, parentPages);
  const folderCount = folderPath.map((folder) => folder.trim()).filter(Boolean).length;
  const visibleCrumbs = showSpace ? crumbs : crumbs.slice(1);

  const collapsed = visibleCrumbs.length > 6;
  const hrefFor = (i: number): string | undefined => {
    const originalIndex = showSpace ? i : i + 1;
    if (i === visibleCrumbs.length - 1) return undefined;
    if (showSpace && i === 0) return spaceHref;
    return parentPages[originalIndex - 1 - folderCount]?.href;
  };
  const inlineStart = collapsed ? visibleCrumbs.length - 2 : 0;

  return (
    <nav className={`${className} flex min-w-0 flex-1 items-center gap-1 text-sm`} aria-label="Breadcrumb">
      {visibleCrumbs.length > 1 && (
        <Popover>
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label="Show full page path"
              className={`rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 shrink-0 items-center justify-center ${collapsed ? "" : "sm:hidden"}`}
            >
              <MoreHorizontal size={16} />
            </button>
          </PopoverTrigger>
          <PopoverContent align="start" aria-label="Full page path" className="max-h-72 w-80 max-w-[calc(100vw-2rem)] overflow-y-auto p-2">
            <ol className="space-y-1 text-sm">
              {visibleCrumbs.map((crumb, i) => (
                <li key={`${crumb}-${i}`} className="min-w-0 break-words px-2 py-1">
                  {hrefFor(i) ? (
                    <Link to={hrefFor(i)!} className="text-text-muted hover:text-text">{crumb}</Link>
                  ) : (
                    <span className={i === visibleCrumbs.length - 1 ? "text-text" : "text-text-muted"} aria-current={i === visibleCrumbs.length - 1 ? "page" : undefined}>{crumb}</span>
                  )}
                </li>
              ))}
            </ol>
          </PopoverContent>
        </Popover>
      )}
      <ol className="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
        {visibleCrumbs.slice(inlineStart).map((crumb, offset) => {
          const i = inlineStart + offset;
          const isCurrent = i === visibleCrumbs.length - 1;
          const linkHref = hrefFor(i);
          return (
            <li key={`${crumb}-${i}`} className={isCurrent ? "flex min-w-0 flex-1 items-center gap-1" : "hidden min-w-0 max-w-48 shrink items-center gap-1 sm:flex"}>
              {i > 0 && <ChevronRight size={12} className="text-text-muted shrink-0" />}
              {linkHref ? (
                <Link to={linkHref} title={crumb} className="text-text-muted hover:text-text min-w-0 truncate">
                  {crumb}
                </Link>
              ) : (
                <span title={crumb} aria-current={isCurrent ? "page" : undefined} className={isCurrent ? "text-text min-w-0 truncate" : "text-text-muted min-w-0 truncate"}>
                  {crumb}
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
