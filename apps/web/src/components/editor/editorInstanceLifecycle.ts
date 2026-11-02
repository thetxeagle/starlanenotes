export interface EditorInstanceReference<T> {
  current: T | null;
}

export function clearEditorInstanceIfCurrent<T>(
  reference: EditorInstanceReference<T>,
  destroyedEditor: T,
): void {
  if (reference.current === destroyedEditor) {
    reference.current = null;
  }
}
