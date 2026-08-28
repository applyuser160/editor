import { describe, expect, it } from "vitest";
import {
  escapeHtml,
  extensionId,
  formatDownloadCount,
  splitExtensionId,
} from "./extension-utils";

describe("extension utilities", () => {
  it("builds and splits Open VSX identifiers", () => {
    expect(extensionId("redhat", "java")).toBe("redhat.java");
    expect(splitExtensionId("redhat.java")).toEqual({
      namespace: "redhat",
      name: "java",
    });
    expect(splitExtensionId("publisher.team.extension")).toEqual({
      namespace: "publisher.team",
      name: "extension",
    });
  });

  it("handles malformed identifiers without throwing", () => {
    expect(splitExtensionId("extension")).toEqual({
      namespace: "extension",
      name: "extension",
    });
    expect(splitExtensionId("publisher.")).toEqual({
      namespace: "publisher.",
      name: "publisher.",
    });
  });

  it("formats download counts while preserving zero", () => {
    expect(formatDownloadCount(0)).toBe("0 DL");
    expect(formatDownloadCount(1234)).toContain("1");
    expect(formatDownloadCount(null)).toBe("");
  });

  it("escapes values rendered into HTML", () => {
    expect(escapeHtml(`<script>alert("x")</script>`)).toBe(
      "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;",
    );
    expect(escapeHtml("publisher's extension & docs")).toBe(
      "publisher&#39;s extension &amp; docs",
    );
  });
});
