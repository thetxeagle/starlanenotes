export function formatDateMetadata(
  value: string | undefined,
  label: string,
  unavailableLabel: string,
): string {
  if (!value) return unavailableLabel;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return unavailableLabel;
  const formatted = date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
  return `${label} ${formatted}`;
}
