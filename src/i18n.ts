export type Locale = "ja" | "en";

type TranslationMap = Record<string, string>;

const dictionaries: Record<Locale, TranslationMap> = {
  ja: {
    "app.title": "Oxide Editor",
    "menu.file": "ファイル (File)",
    "menu.edit": "編集 (Edit)",
    "menu.selection": "選択 (Selection)",
    "menu.view": "表示 (View)",
    "menu.go": "移動 (Go)",
    "menu.run": "実行 (Run)",
    "menu.terminal": "ターミナル (Terminal)",
    "menu.help": "ヘルプ (Help)",
    "activity.explorer": "エクスプローラー",
    "activity.search": "検索",
    "activity.scm": "ソース管理",
    "activity.debug": "実行とデバッグ",
    "activity.extensions": "拡張機能",
    "activity.settings": "設定",
    "panel.terminal": "💻 ターミナル",
    "panel.output": "出力",
    "panel.problems": "問題",
    "quickopen.placeholder":
      "Oxide Editor - ファイルまたはコマンドを検索 (Ctrl+P)",
    "skip.main": "メイン編集領域へ移動",
  },
  en: {
    "app.title": "Oxide Editor",
    "menu.file": "File",
    "menu.edit": "Edit",
    "menu.selection": "Selection",
    "menu.view": "View",
    "menu.go": "Go",
    "menu.run": "Run",
    "menu.terminal": "Terminal",
    "menu.help": "Help",
    "activity.explorer": "Explorer",
    "activity.search": "Search",
    "activity.scm": "Source Control",
    "activity.debug": "Run and Debug",
    "activity.extensions": "Extensions",
    "activity.settings": "Settings",
    "panel.terminal": "💻 Terminal",
    "panel.output": "Output",
    "panel.problems": "Problems",
    "quickopen.placeholder": "Oxide Editor — Search files or commands (Ctrl+P)",
    "skip.main": "Skip to main editor",
  },
};

let activeLocale: Locale = "ja";

export function normalizeLocale(value: unknown): Locale {
  return value === "en" ? "en" : "ja";
}

export function getLocale(): Locale {
  return activeLocale;
}

export function translate(key: string, fallback?: string): string {
  return (
    dictionaries[activeLocale][key] || dictionaries.ja[key] || fallback || key
  );
}

export function applyLocale(value: unknown): Locale {
  activeLocale = normalizeLocale(value);
  document.documentElement.lang = activeLocale;
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((element) => {
    element.textContent = translate(
      element.dataset.i18n || "",
      element.textContent || "",
    );
  });
  document
    .querySelectorAll<HTMLElement>("[data-i18n-aria-label]")
    .forEach((element) => {
      element.setAttribute(
        "aria-label",
        translate(
          element.dataset.i18nAriaLabel || "",
          element.getAttribute("aria-label") || "",
        ),
      );
    });
  document
    .querySelectorAll<HTMLInputElement>("[data-i18n-placeholder]")
    .forEach((element) => {
      element.placeholder = translate(
        element.dataset.i18nPlaceholder || "",
        element.placeholder,
      );
    });
  document.dispatchEvent(
    new CustomEvent("oxide-locale-change", { detail: activeLocale }),
  );
  return activeLocale;
}
