import { invoke } from "@tauri-apps/api/core";

export interface EditorSettings {
  theme: "vscode-dark-plus" | "vs" | "hc-black";
  fontSize: number;
  tabSize: number;
  minimap: boolean;
  locale: "ja" | "en";
  highContrast: boolean;
  reducedMotion: boolean;
}

export type SettingScope = "user" | "workspace" | "language";

export interface Keybinding {
  command: string;
  key: string;
}

export interface KeybindingConflict {
  key: string;
  commands: string[];
}

export interface EditorProfile {
  version: 1;
  id: string;
  name: string;
  createdAt: string;
  settings: Partial<EditorSettings>;
  keybindings: Keybinding[];
  extensions: string[];
}

interface NativeSettingsSnapshot {
  userSettings: unknown;
  workspaceSettings: unknown;
  languageSettings: Record<string, unknown>;
  keybindings: unknown;
  profiles: unknown;
}

export const DEFAULT_SETTINGS: EditorSettings = {
  theme: "vscode-dark-plus",
  fontSize: 14,
  tabSize: 4,
  minimap: true,
  locale: "ja",
  highContrast: false,
  reducedMotion: false,
};

export const COMMAND_LABELS: Record<string, string> = {
  new_file: "新しいファイル",
  open_file_dialog: "ファイルを開く",
  close_tab: "アクティブなタブを閉じる",
  restore_closed_tab: "閉じたタブを復元",
  run: "実行",
  rename_file: "ファイル名を変更",
  go_to_line: "指定行へ移動",
  open_settings: "設定を開く",
  open_explorer: "エクスプローラーを開く",
  open_search: "検索を開く",
  open_scm: "ソース管理を開く",
  open_extensions: "拡張機能を開く",
  new_terminal: "新しいターミナル",
  save: "保存",
  quick_open: "クイックオープン",
  command_palette: "コマンドパレット",
  toggle_sidebar: "サイドバー切替",
  toggle_terminal: "ターミナル切替",
};

export const DEFAULT_KEYBINDINGS: Keybinding[] = [
  { command: "new_file", key: "Ctrl+N" },
  { command: "open_file_dialog", key: "Ctrl+O" },
  { command: "close_tab", key: "Ctrl+W" },
  { command: "restore_closed_tab", key: "Ctrl+Shift+T" },
  { command: "run", key: "F5" },
  { command: "rename_file", key: "F2" },
  { command: "go_to_line", key: "Ctrl+G" },
  { command: "open_settings", key: "Ctrl+," },
  { command: "open_explorer", key: "Ctrl+Shift+E" },
  { command: "open_search", key: "Ctrl+Shift+F" },
  { command: "open_scm", key: "Ctrl+Shift+G" },
  { command: "open_extensions", key: "Ctrl+Shift+X" },
  { command: "new_terminal", key: "Ctrl+Shift+`" },
  { command: "save", key: "Ctrl+S" },
  { command: "quick_open", key: "Ctrl+P" },
  { command: "command_palette", key: "Ctrl+Shift+P" },
  { command: "toggle_sidebar", key: "Ctrl+B" },
  { command: "toggle_terminal", key: "Ctrl+J" },
];

const USER_SETTINGS_KEY = "oxide.settings.user";
const KEYBINDINGS_KEY = "oxide.keybindings.user";
const PROFILES_KEY = "oxide.profiles";
const LEGACY_MIGRATION_KEY = "oxide.settings.legacy-migrated";
const LANGUAGE_SETTINGS_PREFIX = "oxide.settings.language:";

const storedValues = new Map<string, unknown>();
let activeWorkspaceRoot = "";
let nativeStoreReady = false;
let legacyFallback = false;
let savePending = false;

function workspaceSettingsKey(workspaceRoot: string): string {
  return `oxide.settings.workspace:${encodeURIComponent(workspaceRoot || "default")}`;
}

function languageSettingsKey(language: string): string {
  return `${LANGUAGE_SETTINGS_PREFIX}${language || "plaintext"}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function readLegacyJson(key: string): unknown {
  const value = localStorage.getItem(key);
  if (!value) return null;
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

function readJson(key: string): unknown {
  if (storedValues.has(key)) return storedValues.get(key) ?? null;
  if (legacyFallback) return readLegacyJson(key);
  return null;
}

function writeJson(key: string, value: unknown): void {
  storedValues.set(key, cloneJson(value));
  scheduleNativeSave();
}

function removeJson(key: string): void {
  storedValues.delete(key);
  scheduleNativeSave();
}

function validateSettings(value: unknown): Partial<EditorSettings> {
  if (!isRecord(value)) {
    throw new Error("設定はJSONオブジェクトで指定してください");
  }

  const supportedKeys = new Set<keyof EditorSettings>([
    "theme",
    "fontSize",
    "tabSize",
    "minimap",
    "locale",
    "highContrast",
    "reducedMotion",
  ]);
  const unsupportedKey = Object.keys(value).find(
    (key) => !supportedKeys.has(key as keyof EditorSettings),
  );
  if (unsupportedKey) {
    throw new Error(`未対応の設定項目です: ${unsupportedKey}`);
  }

  const settings: Partial<EditorSettings> = {};
  if (value.theme !== undefined) {
    if (
      value.theme !== "vscode-dark-plus" &&
      value.theme !== "vs" &&
      value.theme !== "hc-black"
    ) {
      throw new Error(
        "themeにはvscode-dark-plus、vs、hc-blackのいずれかを指定してください",
      );
    }
    settings.theme = value.theme;
  }
  if (value.fontSize !== undefined) {
    if (
      typeof value.fontSize !== "number" ||
      value.fontSize < 10 ||
      value.fontSize > 28
    ) {
      throw new Error("fontSizeには10から28までの数値を指定してください");
    }
    settings.fontSize = value.fontSize;
  }
  if (value.tabSize !== undefined) {
    if (
      typeof value.tabSize !== "number" ||
      value.tabSize < 2 ||
      value.tabSize > 8
    ) {
      throw new Error("tabSizeには2から8までの数値を指定してください");
    }
    settings.tabSize = value.tabSize;
  }
  if (value.minimap !== undefined) {
    if (typeof value.minimap !== "boolean") {
      throw new Error("minimapには真偽値を指定してください");
    }
    settings.minimap = value.minimap;
  }
  if (value.locale !== undefined) {
    if (value.locale !== "ja" && value.locale !== "en") {
      throw new Error("localeにはjaまたはenを指定してください");
    }
    settings.locale = value.locale;
  }
  if (value.highContrast !== undefined) {
    if (typeof value.highContrast !== "boolean") throw new Error("highContrastには真偽値を指定してください");
    settings.highContrast = value.highContrast;
  }
  if (value.reducedMotion !== undefined) {
    if (typeof value.reducedMotion !== "boolean") throw new Error("reducedMotionには真偽値を指定してください");
    settings.reducedMotion = value.reducedMotion;
  }
  return settings;
}

function readSettings(key: string): Partial<EditorSettings> {
  const value = readJson(key);
  if (value === null) return {};
  try {
    return validateSettings(value);
  } catch {
    return {};
  }
}

function settingsKey(
  scope: SettingScope,
  workspaceRoot: string,
  language: string,
): string {
  if (scope === "workspace") return workspaceSettingsKey(workspaceRoot);
  if (scope === "language") return languageSettingsKey(language);
  return USER_SETTINGS_KEY;
}

function legacySettingsForCurrentWorkspace(
  workspaceRoot: string,
): NativeSettingsSnapshot {
  const languageSettings: Record<string, unknown> = {};
  for (let index = 0; index < localStorage.length; index += 1) {
    const key = localStorage.key(index);
    if (!key || !key.startsWith(LANGUAGE_SETTINGS_PREFIX)) continue;
    const language = key.slice(LANGUAGE_SETTINGS_PREFIX.length);
    const value = readLegacyJson(key);
    if (value !== null) languageSettings[language] = value;
  }

  const legacyUserSettings = readLegacyJson(USER_SETTINGS_KEY) ?? {};
  const oldSettings: Partial<EditorSettings> = {};
  const oldTheme = localStorage.getItem("oxide_theme");
  const oldFontSize = Number(localStorage.getItem("oxide_fontSize"));
  const oldTabSize = Number(localStorage.getItem("oxide_tabSize"));
  const oldMinimap = localStorage.getItem("oxide_minimap");
  if (
    oldTheme === "vscode-dark-plus" ||
    oldTheme === "vs" ||
    oldTheme === "hc-black"
  )
    oldSettings.theme = oldTheme;
  if (oldFontSize >= 10 && oldFontSize <= 28)
    oldSettings.fontSize = oldFontSize;
  if (oldTabSize >= 2 && oldTabSize <= 8) oldSettings.tabSize = oldTabSize;
  if (oldMinimap !== null) oldSettings.minimap = oldMinimap !== "false";

  return {
    userSettings: {
      ...oldSettings,
      ...(isRecord(legacyUserSettings) ? legacyUserSettings : {}),
    },
    workspaceSettings:
      readLegacyJson(workspaceSettingsKey(workspaceRoot)) ?? {},
    languageSettings,
    keybindings: readLegacyJson(KEYBINDINGS_KEY) ?? [],
    profiles: readLegacyJson(PROFILES_KEY) ?? [],
  };
}

function hydrate(
  snapshot: NativeSettingsSnapshot,
  workspaceRoot: string,
): void {
  activeWorkspaceRoot = workspaceRoot;
  storedValues.clear();
  storedValues.set(USER_SETTINGS_KEY, cloneJson(snapshot.userSettings));
  storedValues.set(
    workspaceSettingsKey(workspaceRoot),
    cloneJson(snapshot.workspaceSettings),
  );
  storedValues.set(KEYBINDINGS_KEY, cloneJson(snapshot.keybindings));
  storedValues.set(PROFILES_KEY, cloneJson(snapshot.profiles));
  Object.entries(snapshot.languageSettings || {}).forEach(
    ([language, value]) => {
      storedValues.set(languageSettingsKey(language), cloneJson(value));
    },
  );
}

function snapshotForPersistence(): NativeSettingsSnapshot {
  const languageSettings: Record<string, unknown> = {};
  storedValues.forEach((value, key) => {
    if (key.startsWith(LANGUAGE_SETTINGS_PREFIX)) {
      languageSettings[key.slice(LANGUAGE_SETTINGS_PREFIX.length)] =
        cloneJson(value);
    }
  });

  return {
    userSettings: readJson(USER_SETTINGS_KEY) ?? {},
    workspaceSettings:
      readJson(workspaceSettingsKey(activeWorkspaceRoot)) ?? {},
    languageSettings,
    keybindings: readJson(KEYBINDINGS_KEY) ?? [],
    profiles: readJson(PROFILES_KEY) ?? [],
  };
}

function clearMigratedLocalStorage(workspaceRoot: string): void {
  const keys = [
    USER_SETTINGS_KEY,
    KEYBINDINGS_KEY,
    PROFILES_KEY,
    workspaceSettingsKey(workspaceRoot),
    "oxide_theme",
    "oxide_fontSize",
    "oxide_tabSize",
    "oxide_minimap",
  ];
  for (let index = 0; index < localStorage.length; index += 1) {
    const key = localStorage.key(index);
    if (key?.startsWith(LANGUAGE_SETTINGS_PREFIX)) keys.push(key);
  }
  keys.forEach((key) => localStorage.removeItem(key));
  localStorage.setItem(LEGACY_MIGRATION_KEY, "true");
}

function scheduleNativeSave(): void {
  if (!nativeStoreReady || savePending) return;
  savePending = true;
  queueMicrotask(async () => {
    savePending = false;
    try {
      const snapshot = await invoke<NativeSettingsSnapshot>(
        "save_editor_configuration",
        {
          snapshot: snapshotForPersistence(),
        },
      );
      hydrate(snapshot, activeWorkspaceRoot);
    } catch (error) {
      console.error("Failed to persist editor configuration:", error);
    }
  });
}

export async function initializeSettingsPersistence(
  workspaceRoot: string,
): Promise<void> {
  activeWorkspaceRoot = workspaceRoot;
  legacyFallback = false;
  const legacySnapshot = legacySettingsForCurrentWorkspace(workspaceRoot);
  try {
    const snapshot = await invoke<NativeSettingsSnapshot>(
      "migrate_editor_configuration",
      {
        snapshot: legacySnapshot,
      },
    );
    hydrate(snapshot, workspaceRoot);
    nativeStoreReady = true;
    clearMigratedLocalStorage(workspaceRoot);
  } catch (error) {
    nativeStoreReady = false;
    legacyFallback = true;
    console.error("Failed to initialize native editor configuration:", error);
    throw error;
  }
}

export async function reloadSettingsPersistence(
  workspaceRoot = activeWorkspaceRoot,
): Promise<void> {
  if (!nativeStoreReady) return;
  const snapshot = await invoke<NativeSettingsSnapshot>(
    "load_editor_configuration",
  );
  hydrate(snapshot, workspaceRoot);
}

export function migrateLegacySettings(): void {
  // Native initialization performs the authoritative migration. This fallback preserves
  // the former synchronous API for callers that run before the native bridge is ready.
  if (nativeStoreReady || localStorage.getItem(LEGACY_MIGRATION_KEY)) return;
  if (storedValues.has(USER_SETTINGS_KEY)) {
    localStorage.setItem(LEGACY_MIGRATION_KEY, "true");
    return;
  }

  const savedUserSettings = readLegacyJson(USER_SETTINGS_KEY);
  if (isRecord(savedUserSettings)) {
    storedValues.set(USER_SETTINGS_KEY, cloneJson(savedUserSettings));
    localStorage.setItem(LEGACY_MIGRATION_KEY, "true");
    return;
  }

  const legacy: Partial<EditorSettings> = {};
  const theme = localStorage.getItem("oxide_theme");
  const fontSize = Number(localStorage.getItem("oxide_fontSize"));
  const tabSize = Number(localStorage.getItem("oxide_tabSize"));
  const minimap = localStorage.getItem("oxide_minimap");
  if (theme === "vscode-dark-plus" || theme === "vs" || theme === "hc-black")
    legacy.theme = theme;
  if (fontSize >= 10 && fontSize <= 28) legacy.fontSize = fontSize;
  if (tabSize >= 2 && tabSize <= 8) legacy.tabSize = tabSize;
  if (minimap !== null) legacy.minimap = minimap !== "false";
  if (Object.keys(legacy).length > 0) {
    storedValues.set(USER_SETTINGS_KEY, cloneJson(legacy));
    localStorage.setItem(USER_SETTINGS_KEY, JSON.stringify(legacy));
  }
  localStorage.setItem(LEGACY_MIGRATION_KEY, "true");
}

export function getScopedSettings(
  scope: SettingScope,
  workspaceRoot: string,
  language: string,
): Partial<EditorSettings> {
  return readSettings(settingsKey(scope, workspaceRoot, language));
}

export function saveScopedSettings(
  scope: SettingScope,
  workspaceRoot: string,
  language: string,
  value: unknown,
): Partial<EditorSettings> {
  const settings = validateSettings(value);
  writeJson(settingsKey(scope, workspaceRoot, language), settings);
  return settings;
}

export function resolveSettings(
  workspaceRoot: string,
  language: string,
): EditorSettings {
  return {
    ...DEFAULT_SETTINGS,
    ...getScopedSettings("user", workspaceRoot, language),
    ...getScopedSettings("workspace", workspaceRoot, language),
    ...getScopedSettings("language", workspaceRoot, language),
  };
}

export function normalizeKeybinding(key: string): string {
  const aliases: Record<string, string> = {
    control: "Ctrl",
    ctrl: "Ctrl",
    shift: "Shift",
    alt: "Alt",
    option: "Alt",
    meta: "Meta",
    cmd: "Meta",
    command: "Meta",
    " ": "Space",
  };
  const modifiers = ["Ctrl", "Shift", "Alt", "Meta"];
  const parts = key
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map(
      (part) =>
        aliases[part.toLowerCase()] ||
        (part.length === 1 ? part.toUpperCase() : part),
    );
  const uniqueModifiers = modifiers.filter((modifier) =>
    parts.includes(modifier),
  );
  const mainKey = parts.find((part) => !modifiers.includes(part));
  return [...uniqueModifiers, ...(mainKey ? [mainKey] : [])].join("+");
}

function validateKeybindings(value: unknown): Keybinding[] {
  if (!Array.isArray(value)) {
    throw new Error("キーバインドはJSON配列で指定してください");
  }

  const keybindings = value.map((item) => {
    if (
      !isRecord(item) ||
      typeof item.command !== "string" ||
      typeof item.key !== "string"
    ) {
      throw new Error("各キーバインドにはcommandとkeyが必要です");
    }
    if (!(item.command in COMMAND_LABELS)) {
      throw new Error(`未対応のコマンドです: ${item.command}`);
    }
    const key = normalizeKeybinding(item.key);
    if (!key) throw new Error(`${item.command}のキーが空です`);
    return { command: item.command, key };
  });

  const commands = new Set<string>();
  keybindings.forEach((binding) => {
    if (commands.has(binding.command))
      throw new Error(`コマンドが重複しています: ${binding.command}`);
    commands.add(binding.command);
  });
  return keybindings;
}

export function getKeybindings(): Keybinding[] {
  const stored = readJson(KEYBINDINGS_KEY);
  if (stored === null)
    return DEFAULT_KEYBINDINGS.map((binding) => ({ ...binding }));
  try {
    return validateKeybindings(stored);
  } catch {
    return DEFAULT_KEYBINDINGS.map((binding) => ({ ...binding }));
  }
}

export function saveKeybindings(value: unknown): Keybinding[] {
  const keybindings = validateKeybindings(value);
  writeJson(KEYBINDINGS_KEY, keybindings);
  return keybindings;
}

export function resetKeybindings(): Keybinding[] {
  removeJson(KEYBINDINGS_KEY);
  return getKeybindings();
}

export function findKeybindingConflicts(
  keybindings: Keybinding[],
): KeybindingConflict[] {
  const commandsByKey = new Map<string, string[]>();
  keybindings.forEach((binding) => {
    const key = normalizeKeybinding(binding.key);
    const commands = commandsByKey.get(key) || [];
    commands.push(binding.command);
    commandsByKey.set(key, commands);
  });
  return Array.from(commandsByKey.entries())
    .filter(([, commands]) => commands.length > 1)
    .map(([key, commands]) => ({ key, commands }));
}

export function keybindingFromEvent(event: KeyboardEvent): string {
  const modifiers: string[] = [];
  if (event.ctrlKey) modifiers.push("Ctrl");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.altKey) modifiers.push("Alt");
  if (event.metaKey) modifiers.push("Meta");

  const modifierKeys = new Set(["Control", "Shift", "Alt", "Meta"]);
  if (!modifierKeys.has(event.key)) {
    const key =
      event.key === " "
        ? "Space"
        : event.key.length === 1
          ? event.key.toUpperCase()
          : event.key;
    modifiers.push(key);
  }
  return normalizeKeybinding(modifiers.join("+"));
}

export function commandForEvent(event: KeyboardEvent): string | null {
  const pressed = keybindingFromEvent(event);
  return (
    getKeybindings().find(
      (binding) => normalizeKeybinding(binding.key) === pressed,
    )?.command || null
  );
}

function validateProfile(value: unknown): EditorProfile {
  if (
    !isRecord(value) ||
    value.version !== 1 ||
    typeof value.name !== "string"
  ) {
    throw new Error("対応していないプロファイル形式です");
  }
  const settings = validateSettings(value.settings);
  const keybindings = validateKeybindings(value.keybindings);
  const extensions = Array.isArray(value.extensions)
    ? value.extensions.map((extension) => {
        if (typeof extension !== "string")
          throw new Error("拡張機能IDには文字列を指定してください");
        return extension;
      })
    : [];
  return {
    version: 1,
    id: typeof value.id === "string" ? value.id : crypto.randomUUID(),
    name: value.name.trim() || "Imported Profile",
    createdAt:
      typeof value.createdAt === "string"
        ? value.createdAt
        : new Date().toISOString(),
    settings,
    keybindings,
    extensions: [...new Set(extensions)].sort(),
  };
}

export function getProfiles(): EditorProfile[] {
  const value = readJson(PROFILES_KEY);
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    try {
      return [validateProfile(item)];
    } catch {
      return [];
    }
  });
}

function storeProfiles(profiles: EditorProfile[]): void {
  writeJson(PROFILES_KEY, profiles);
}

export function createProfile(
  name: string,
  extensions: string[],
): EditorProfile {
  const profile: EditorProfile = {
    version: 1,
    id: crypto.randomUUID(),
    name: name.trim(),
    createdAt: new Date().toISOString(),
    settings: getScopedSettings("user", "", ""),
    keybindings: getKeybindings(),
    extensions: [...new Set(extensions)].sort(),
  };
  if (!profile.name) throw new Error("プロファイル名を入力してください");
  storeProfiles([...getProfiles(), profile]);
  return profile;
}

export function deleteProfile(id: string): void {
  storeProfiles(getProfiles().filter((profile) => profile.id !== id));
}

export function applyProfile(profile: EditorProfile): void {
  writeJson(USER_SETTINGS_KEY, profile.settings);
  writeJson(KEYBINDINGS_KEY, profile.keybindings);
}

export function exportProfile(profile: EditorProfile): string {
  return JSON.stringify(profile, null, 2);
}

export function importProfile(json: string): EditorProfile {
  const profile = validateProfile(JSON.parse(json) as unknown);
  const profiles = getProfiles().filter(
    (existing) => existing.id !== profile.id,
  );
  storeProfiles([...profiles, profile]);
  return profile;
}
