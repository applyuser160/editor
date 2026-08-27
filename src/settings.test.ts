import { beforeEach, describe, expect, it } from "vitest";

import {
  DEFAULT_KEYBINDINGS,
  DEFAULT_SETTINGS,
  commandForEvent,
  findKeybindingConflicts,
  getKeybindings,
  getScopedSettings,
  migrateLegacySettings,
  resolveSettings,
  saveKeybindings,
  saveScopedSettings,
} from "./settings";

describe("settings persistence", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("migrates valid legacy settings only once without overwriting saved user settings", () => {
    localStorage.setItem("oxide_theme", "vs");
    localStorage.setItem("oxide_fontSize", "18");
    localStorage.setItem("oxide_tabSize", "2");
    localStorage.setItem("oxide_minimap", "false");

    migrateLegacySettings();

    expect(getScopedSettings("user", "/workspace", "typescript")).toEqual({
      theme: "vs",
      fontSize: 18,
      tabSize: 2,
      minimap: false,
    });

    localStorage.setItem("oxide_theme", "hc-black");
    migrateLegacySettings();

    expect(getScopedSettings("user", "/workspace", "typescript")).toEqual({
      theme: "vs",
      fontSize: 18,
      tabSize: 2,
      minimap: false,
    });
  });

  it("does not overwrite a saved user configuration during legacy migration", () => {
    saveScopedSettings("user", "", "", { fontSize: 16 });
    localStorage.setItem("oxide_fontSize", "22");

    migrateLegacySettings();

    expect(getScopedSettings("user", "", "")).toEqual({ fontSize: 16 });
  });

  it("resolves default, user, workspace, and language settings in increasing precedence", () => {
    saveScopedSettings("user", "/workspace", "typescript", {
      theme: "vs",
      fontSize: 16,
      tabSize: 2,
    });
    saveScopedSettings("workspace", "/workspace", "typescript", {
      fontSize: 18,
      minimap: false,
    });
    saveScopedSettings("language", "/workspace", "typescript", {
      tabSize: 8,
    });

    expect(resolveSettings("/workspace", "typescript")).toEqual({
      ...DEFAULT_SETTINGS,
      theme: "vs",
      fontSize: 18,
      tabSize: 8,
      minimap: false,
    });
  });

  it("rejects unsupported setting keys and invalid setting values", () => {
    expect(() =>
      saveScopedSettings("user", "", "", { unsupported: true }),
    ).toThrow("未対応の設定項目です: unsupported");
    expect(() => saveScopedSettings("user", "", "", { fontSize: 29 })).toThrow(
      "fontSizeには10から28までの数値を指定してください",
    );
  });
});

describe("keybindings", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("returns independent default keybindings when no custom bindings are saved", () => {
    const keybindings = getKeybindings();

    expect(keybindings).toEqual(DEFAULT_KEYBINDINGS);
    expect(keybindings).not.toBe(DEFAULT_KEYBINDINGS);
  });

  it("resolves Ctrl+S to the save command", () => {
    expect(
      commandForEvent(
        new KeyboardEvent("keydown", { key: "s", ctrlKey: true }),
      ),
    ).toBe("save");
  });

  it("normalizes key aliases, rejects duplicate commands, and reports key conflicts", () => {
    expect(saveKeybindings([{ command: "save", key: "control + s" }])).toEqual([
      { command: "save", key: "Ctrl+S" },
    ]);

    expect(() =>
      saveKeybindings([
        { command: "save", key: "Ctrl+S" },
        { command: "save", key: "Ctrl+Shift+S" },
      ]),
    ).toThrow("コマンドが重複しています: save");

    expect(
      findKeybindingConflicts([
        { command: "save", key: "Ctrl+S" },
        { command: "new_file", key: "control + s" },
      ]),
    ).toEqual([{ key: "Ctrl+S", commands: ["save", "new_file"] }]);
  });
});
