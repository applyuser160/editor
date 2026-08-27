import { describe, expect, it } from "vitest";

import { normalizeFilePath, pathForWorkspaceRead } from "./path-utils";

describe("search-result file paths", () => {
  it("keeps a relative Windows search result relative for backend reads", () => {
    expect(
      pathForWorkspaceRead(
        "src-tauri\\src\\settings_store.rs",
        "D:\\Desktop\\editor",
      ),
    ).toBe("src-tauri/src/settings_store.rs");
  });

  it("converts an in-workspace Windows absolute path to a relative path", () => {
    expect(
      pathForWorkspaceRead(
        "D:\\Desktop\\editor\\src-tauri\\src\\settings_store.rs",
        "D:/Desktop/editor",
      ),
    ).toBe("src-tauri/src/settings_store.rs");
  });

  it("removes the Windows extended-length namespace before resolving paths", () => {
    expect(
      pathForWorkspaceRead(
        "\\\\?\\D:\\Desktop\\editor\\src-tauri\\src\\settings_store.rs",
        "\\\\?\\D:\\Desktop\\editor",
      ),
    ).toBe("src-tauri/src/settings_store.rs");
  });

  it("preserves paths outside the workspace for backend boundary validation", () => {
    expect(pathForWorkspaceRead("D:/other/file.rs", "D:/Desktop/editor")).toBe(
      "D:/other/file.rs",
    );
  });

  it("normalizes separators for Monaco model paths", () => {
    expect(normalizeFilePath("src-tauri\\src\\settings_store.rs")).toBe(
      "src-tauri/src/settings_store.rs",
    );
  });
});
