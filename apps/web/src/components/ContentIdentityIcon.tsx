import {
  Archive,
  Bell,
  Bookmark,
  BookOpen,
  Brain,
  Briefcase,
  CalendarDays,
  ChartNoAxesColumn,
  CircleCheck,
  CircleHelp,
  ClipboardList,
  Clock3,
  Code2,
  Compass,
  Construction,
  CookingPot,
  Database,
  Dumbbell,
  FileText,
  Flame,
  FlaskConical,
  GraduationCap,
  Handshake,
  Heart,
  House,
  KeyRound,
  Library,
  Lightbulb,
  Link2,
  LockKeyhole,
  Map as MapIcon,
  MessageCircle,
  NotebookPen,
  Package,
  Palette,
  Paperclip,
  Plane,
  Rocket,
  Search,
  Settings,
  Sprout,
  Star,
  Target,
  TrendingUp,
  TriangleAlert,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";

export type ContentIconSource = "emoji" | "lucide" | "asset" | "legacy";

export interface ContentIconOption {
  value: `lucide:${string}`;
  label: string;
  keywords: string;
  category: string;
  icon: LucideIcon;
}

export interface ParsedContentIcon {
  source: ContentIconSource;
  value: string;
}

export const CONTENT_ICON_CATALOG: ContentIconOption[] = [
  {
    value: "lucide:file-text",
    label: "Page",
    keywords: "document paper file",
    category: "Pages",
    icon: FileText,
  },
  {
    value: "lucide:notebook-pen",
    label: "Notes",
    keywords: "write pencil memo",
    category: "Pages",
    icon: NotebookPen,
  },
  {
    value: "lucide:book-open",
    label: "Book",
    keywords: "reference guide reading",
    category: "Pages",
    icon: BookOpen,
  },
  {
    value: "lucide:library",
    label: "Library",
    keywords: "books knowledge reference",
    category: "Pages",
    icon: Library,
  },
  {
    value: "lucide:bookmark",
    label: "Bookmark",
    keywords: "saved reference",
    category: "Pages",
    icon: Bookmark,
  },
  {
    value: "lucide:archive",
    label: "Archive",
    keywords: "storage file",
    category: "Pages",
    icon: Archive,
  },
  {
    value: "lucide:clipboard-list",
    label: "Checklist",
    keywords: "tasks list",
    category: "Pages",
    icon: ClipboardList,
  },
  {
    value: "lucide:lightbulb",
    label: "Idea",
    keywords: "insight thought",
    category: "Ideas",
    icon: Lightbulb,
  },
  {
    value: "lucide:brain",
    label: "Brain",
    keywords: "thinking knowledge",
    category: "Ideas",
    icon: Brain,
  },
  {
    value: "lucide:search",
    label: "Research",
    keywords: "inspect find",
    category: "Ideas",
    icon: Search,
  },
  {
    value: "lucide:flask-conical",
    label: "Experiment",
    keywords: "test lab science",
    category: "Ideas",
    icon: FlaskConical,
  },
  {
    value: "lucide:target",
    label: "Goal",
    keywords: "objective outcome",
    category: "Ideas",
    icon: Target,
  },
  {
    value: "lucide:compass",
    label: "Direction",
    keywords: "roadmap navigation",
    category: "Ideas",
    icon: Compass,
  },
  {
    value: "lucide:map",
    label: "Map",
    keywords: "plan roadmap",
    category: "Ideas",
    icon: MapIcon,
  },
  {
    value: "lucide:briefcase",
    label: "Work",
    keywords: "business project",
    category: "Work",
    icon: Briefcase,
  },
  {
    value: "lucide:chart-no-axes-column",
    label: "Report",
    keywords: "analytics data",
    category: "Work",
    icon: ChartNoAxesColumn,
  },
  {
    value: "lucide:trending-up",
    label: "Growth",
    keywords: "progress trend",
    category: "Work",
    icon: TrendingUp,
  },
  {
    value: "lucide:calendar-days",
    label: "Plan",
    keywords: "schedule date",
    category: "Work",
    icon: CalendarDays,
  },
  {
    value: "lucide:handshake",
    label: "Meeting",
    keywords: "team agreement",
    category: "Work",
    icon: Handshake,
  },
  {
    value: "lucide:rocket",
    label: "Launch",
    keywords: "ship release",
    category: "Work",
    icon: Rocket,
  },
  {
    value: "lucide:wrench",
    label: "Build",
    keywords: "tools engineering",
    category: "Work",
    icon: Wrench,
  },
  {
    value: "lucide:code-2",
    label: "Code",
    keywords: "development programming",
    category: "Work",
    icon: Code2,
  },
  {
    value: "lucide:circle-check",
    label: "Complete",
    keywords: "done success",
    category: "Status",
    icon: CircleCheck,
  },
  {
    value: "lucide:clock-3",
    label: "In progress",
    keywords: "time waiting",
    category: "Status",
    icon: Clock3,
  },
  {
    value: "lucide:construction",
    label: "Blocked",
    keywords: "work roadblock",
    category: "Status",
    icon: Construction,
  },
  {
    value: "lucide:triangle-alert",
    label: "Warning",
    keywords: "attention risk",
    category: "Status",
    icon: TriangleAlert,
  },
  {
    value: "lucide:flame",
    label: "Priority",
    keywords: "hot urgent",
    category: "Status",
    icon: Flame,
  },
  {
    value: "lucide:star",
    label: "Star",
    keywords: "favorite important",
    category: "Status",
    icon: Star,
  },
  {
    value: "lucide:circle-help",
    label: "Question",
    keywords: "help unknown",
    category: "Status",
    icon: CircleHelp,
  },
  {
    value: "lucide:message-circle",
    label: "Discussion",
    keywords: "chat comment",
    category: "Status",
    icon: MessageCircle,
  },
  {
    value: "lucide:house",
    label: "Home",
    keywords: "personal house",
    category: "Life",
    icon: House,
  },
  {
    value: "lucide:heart",
    label: "Heart",
    keywords: "love favorite",
    category: "Life",
    icon: Heart,
  },
  {
    value: "lucide:sprout",
    label: "Growing",
    keywords: "plant garden progress",
    category: "Life",
    icon: Sprout,
  },
  {
    value: "lucide:cooking-pot",
    label: "Recipe",
    keywords: "food cooking",
    category: "Life",
    icon: CookingPot,
  },
  {
    value: "lucide:dumbbell",
    label: "Fitness",
    keywords: "workout health",
    category: "Life",
    icon: Dumbbell,
  },
  {
    value: "lucide:plane",
    label: "Travel",
    keywords: "flight trip",
    category: "Life",
    icon: Plane,
  },
  {
    value: "lucide:graduation-cap",
    label: "Learning",
    keywords: "school education",
    category: "Life",
    icon: GraduationCap,
  },
  {
    value: "lucide:palette",
    label: "Creative",
    keywords: "art design",
    category: "Life",
    icon: Palette,
  },
  {
    value: "lucide:lock-keyhole",
    label: "Secure",
    keywords: "private security",
    category: "Objects",
    icon: LockKeyhole,
  },
  {
    value: "lucide:key-round",
    label: "Key",
    keywords: "access password",
    category: "Objects",
    icon: KeyRound,
  },
  {
    value: "lucide:settings",
    label: "Settings",
    keywords: "gear configuration",
    category: "Objects",
    icon: Settings,
  },
  {
    value: "lucide:bell",
    label: "Notification",
    keywords: "alert",
    category: "Objects",
    icon: Bell,
  },
  {
    value: "lucide:paperclip",
    label: "Attachment",
    keywords: "file upload",
    category: "Objects",
    icon: Paperclip,
  },
  {
    value: "lucide:link-2",
    label: "Link",
    keywords: "chain url",
    category: "Objects",
    icon: Link2,
  },
  {
    value: "lucide:package",
    label: "Package",
    keywords: "box release",
    category: "Objects",
    icon: Package,
  },
  {
    value: "lucide:database",
    label: "Database",
    keywords: "storage data",
    category: "Objects",
    icon: Database,
  },
];

const CONTENT_ICON_BY_VALUE = new Map(
  CONTENT_ICON_CATALOG.map((option) => [option.value, option] as const),
);

export function parseContentIcon(value?: string | null): ParsedContentIcon | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("emoji:")) {
    return { source: "emoji", value: trimmed.slice("emoji:".length) };
  }
  if (trimmed.startsWith("lucide:")) {
    return { source: "lucide", value: trimmed };
  }
  if (trimmed.startsWith("asset:")) {
    return { source: "asset", value: trimmed.slice("asset:".length) };
  }
  return { source: "legacy", value: trimmed };
}

export function getContentIconLabel(value?: string | null): string | null {
  const parsed = parseContentIcon(value);
  if (!parsed) return null;
  if (parsed.source === "lucide") {
    return CONTENT_ICON_BY_VALUE.get(parsed.value as ContentIconOption["value"])?.label ?? "Icon";
  }
  if (parsed.source === "asset") return "Custom icon";
  return parsed.value;
}

interface ContentIdentityIconProps {
  value?: string | null;
  size?: number;
  className?: string;
  emojiClassName?: string;
  fallback?: ReactNode;
}

export function ContentIdentityIcon({
  value,
  size = 16,
  className = "",
  emojiClassName = "",
  fallback = null,
}: ContentIdentityIconProps) {
  const parsed = parseContentIcon(value);
  if (!parsed) return <>{fallback}</>;

  if (parsed.source === "lucide") {
    const option = CONTENT_ICON_BY_VALUE.get(parsed.value as ContentIconOption["value"]);
    if (!option) return <>{fallback}</>;
    const Icon = option.icon;
    return <Icon size={size} strokeWidth={1.75} className={className} aria-hidden="true" />;
  }

  if (parsed.source === "asset") {
    return <>{fallback}</>;
  }

  return (
    <span
      className={`inline-flex shrink-0 items-center justify-center leading-none ${className} ${emojiClassName}`.trim()}
      style={{ width: size, height: size, fontSize: size }}
      aria-hidden="true"
    >
      {parsed.value}
    </span>
  );
}
