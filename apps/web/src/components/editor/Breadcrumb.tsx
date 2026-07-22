import { ChevronRight } from "lucide-react";
import { Link } from "react-router-dom";

interface BreadcrumbProps {
  spaceName: string;
  folderPath?: string[];
  title: string;
  spaceHref?: string;
  className?: string;
}

export function buildBreadcrumbLabels(
  spaceName: string,
  folderPath: string[] = [],
  title: string,
): string[] {
  const spaceLabel = spaceName.trim() || "Space";
  const folderLabels = folderPath.map((folder) => folder.trim()).filter(Boolean);
  const titleLabel = title.trim() || "Untitled";
  return [spaceLabel, ...folderLabels, titleLabel];
}

export function Breadcrumb({
  spaceName,
  folderPath = [],
  title,
  spaceHref,
  className = "mb-4",
}: BreadcrumbProps) {
  const crumbs = buildBreadcrumbLabels(spaceName, folderPath, title);

  return (
    <nav className={`${className} min-w-0 flex-1 text-sm`} aria-label="Breadcrumb">
      <ol className="flex min-w-0 items-center gap-1 overflow-hidden">
        {crumbs.map((crumb, i) => {
          const isCurrent = i === crumbs.length - 1;
          const isSpaceLink = i === 0 && typeof spaceHref === "string" && !isCurrent;
          return (
            <li key={`${crumb}-${i}`} className="flex min-w-0 items-center gap-1">
              {i > 0 && <ChevronRight size={12} className="shrink-0 text-text-muted" />}
              {isSpaceLink ? (
                <Link
                  to={spaceHref ?? ""}
                  className="min-w-0 truncate text-text-muted hover:text-text"
                >
                  {crumb}
                </Link>
              ) : (
                <span
                  aria-current={isCurrent ? "page" : undefined}
                  className={
                    isCurrent
                      ? "min-w-0 truncate text-text"
                      : "min-w-0 truncate text-text-muted"
                  }
                >
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
