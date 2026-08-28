export interface ExtensionIdentifier {
  namespace: string;
  name: string;
}

/** Return the Open VSX identifier for a namespace/name pair. */
export function extensionId(namespace: string, name: string): string {
  return `${namespace}.${name}`;
}

/**
 * Split a publisher.extension identifier without losing dots in either part.
 * Open VSX identifiers use the final dot as the namespace/name separator.
 */
export function splitExtensionId(id: string): ExtensionIdentifier {
  const separator = id.lastIndexOf(".");
  if (separator <= 0 || separator === id.length - 1) {
    return { namespace: id, name: id };
  }

  return {
    namespace: id.slice(0, separator),
    name: id.slice(separator + 1),
  };
}

export function formatDownloadCount(count: number | null): string {
  return count === null ? "" : `${count.toLocaleString()} DL`;
}

/** Escape API-provided strings before placing them in an HTML template. */
export function escapeHtml(value: string): string {
  return value.replace(
    /[&<>\"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '\"': "&quot;",
        "'": "&#39;",
      })[character] ?? character,
  );
}
