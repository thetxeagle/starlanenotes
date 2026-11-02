import { cn } from "@/lib/utils";

interface LoadingStateProps {
  label?: string;
  viewport?: boolean;
}

const SKELETON_BLOCK_CLASS =
  "animate-pulse rounded-input bg-surface-raised motion-reduce:animate-none";

export function LoadingState({ label = "Loading", viewport = false }: LoadingStateProps) {
  return (
    <div
      className={cn(
        "flex items-start justify-center p-4 pt-[clamp(3rem,12vh,7rem)]",
        viewport ? "app-viewport app-background" : "h-full",
      )}
      role="status"
      aria-live="polite"
    >
      <span className="sr-only">{label}</span>
      <div className="w-full max-w-3xl" aria-hidden="true">
        <div className={`${SKELETON_BLOCK_CLASS} h-4 w-24`} />
        <div className={`${SKELETON_BLOCK_CLASS} mt-5 h-9 w-2/3 max-w-lg`} />
        <div className={`${SKELETON_BLOCK_CLASS} mt-3 h-3 w-52 max-w-full`} />
        <div className="border-border mt-8 space-y-3 border-t pt-6">
          <div className={`${SKELETON_BLOCK_CLASS} h-3 w-full`} />
          <div className={`${SKELETON_BLOCK_CLASS} h-3 w-11/12`} />
          <div className={`${SKELETON_BLOCK_CLASS} h-3 w-4/5`} />
        </div>
      </div>
    </div>
  );
}
