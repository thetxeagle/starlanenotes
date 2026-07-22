export interface StartupSpaceFallbackInput {
  pathname: string;
  isMobileRuntime: boolean;
  checkingDocumentRestore: boolean;
  documentRestorePath: string | null;
}

export function shouldResolveDesktopStartupSpaceFallback({
  pathname,
  isMobileRuntime,
  checkingDocumentRestore,
  documentRestorePath,
}: StartupSpaceFallbackInput): boolean {
  return (
    pathname === "/" &&
    !isMobileRuntime &&
    !checkingDocumentRestore &&
    !documentRestorePath
  );
}
