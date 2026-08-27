function removeWindowsNamespace(path: string): string {
  return path.startsWith("//?/") ? path.slice(4) : path;
}

export function normalizeFilePath(rawPath: string): string {
  return removeWindowsNamespace(rawPath.replace(/\\/g, "/")).replace(
    /\/{2,}/g,
    "/",
  );
}

function isWindowsDrivePath(path: string): boolean {
  return /^[a-zA-Z]:\//.test(path);
}

function pathComparisonKey(path: string): string {
  return isWindowsDrivePath(path) ? path.toLowerCase() : path;
}

/**
 * Returns a backend read path without eagerly turning workspace-relative paths
 * into an absolute Windows path. The backend resolves relative paths against
 * the trusted workspace root and performs the final boundary validation.
 */
export function pathForWorkspaceRead(
  rawPath: string,
  workspaceRoot: string,
): string {
  const path = normalizeFilePath(rawPath);
  const root = normalizeFilePath(workspaceRoot).replace(/\/+$/, "");
  if (!root) return path;

  const comparablePath = pathComparisonKey(path);
  const comparableRoot = pathComparisonKey(root);
  if (comparablePath === comparableRoot) return ".";
  if (comparablePath.startsWith(`${comparableRoot}/`)) {
    return path.slice(root.length + 1);
  }
  return path;
}
