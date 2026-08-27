# ADR-0005: Tauri v2への段階的移行

* **ステータス:** 承認済み (Accepted) — 部分実装
* **日付:** 2026-08-22
* **意思決定者:** Syun
* **見直し日:** 2026-08-27
* **実装状況:** [Tauriシェルと限定的な開発機能は実装済み](../implementation-status.md)。VS Code UI・拡張機能との完全互換、および性能目標は未実装または未検証である。

---

## 文脈と課題 (Context and Problem Statement)

デスクトップシェルとして Tauri v2 を採用し、段階的に編集・ワークスペース・開発支援機能を追加するためのフレームワークを選定する必要がありました。メモリ消費量、起動時間、VS Code 互換性は、測定計画と E2E 検証を伴う将来の受け入れ基準です。

---

## 検討した選択肢 (Considered Options)

1. **現行 Electron を維持し、Node.js 側を部分的に Native Addon で最適化**
2. **ピュア Rust GUI（GPUI / Egui）でフロントエンドも含めゼロから再実装**
3. **Tauri v2（Rust Core + OS システム WebView）へ段階的に移行**

---

## 決定結果 (Decision Outcome)

**選択肢 3: 「Tauri v2 への段階的移行」** を採用しました。現行リリースは Tauri WebView と Monaco Editor を基盤とし、機能と互換性を段階的に評価・実装します。

### 決定理由 (Rationale):
- **段階的な実装:** Tauri の Rust バックエンドと WebView UI を用い、機能を小さな単位で実装・検証できる。
- **既存コンポーネントの活用:** Monaco Editor を編集 UI として利用できる。VS Code Workbench UI や拡張機能 API の直接再利用・完全互換は別途検証が必要である。
- **ネイティブ連携:** PTY、ファイル監視、ファイル I/O、LSP クライアントを Rust 側で実装できる。性能値は実測により確認する。

---

## プラスの影響 (Positive Consequences)
- Tauri v2 をデスクトップシェルとして採用し、Rust 側の PTY、ファイル監視、LSP、ワークスペース操作の基盤を実装できた。
- WebView と Monaco Editor を活用して、基本的な編集 UI を提供できる。
- メモリ消費量、起動時間、対応 OS、拡張機能互換性の達成度は未検証であり、数値目標としてのみ扱う。

## マイナスの影響 (Negative Consequences)
- Linux 環境において `webkit2gtk-4.1` の依存関係が必要。
- 大容量バイナリ転送時に Tauri IPC Channel を適切に設計する必要がある。
