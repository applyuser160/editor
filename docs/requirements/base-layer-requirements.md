# VS Code Base レイヤー 機能要件定義書 (ISO 29148 準拠)

> 本ドキュメントは、Microsoft VS Code の共通基盤層 (`src/vs/base/`) を Rust ネイティブ環境へ移植するための機能要件定義書です。

---

## 1. 概要とスコープ

`src/vs/base/` は、UI やプラットフォームに依存しない純粋なコアアルゴリズム、非同期処理、ライフサイクル管理、イベントディスパッチ、データ構造、プロセス間通信 (IPC) を提供する最下位レイヤーです。

---

## 2. 機能要件一覧 (Functional Requirements)

### REQ-BASE-001: ライフサイクル & リソース管理 (Lifecycle & Disposable)
- **説明:** すべての購読、タイマー、OSハンドル、バッファを決定論的に解放する Disposable パターンを実装する。
- **詳細要件:**
  - `IDisposable` トレイトを定義し、Rust の `Drop` トレイトと協調動作すること。
  - `DisposableStore` により、複数の Disposable オブジェクトを一括登録・一括破棄できること。
  - 破棄済みオブジェクトへの二重解放を防止し、メモリリークを根絶すること。

### REQ-BASE-002: イベントディスパッチ (Event & Emitter)
- **説明:** 強型付けされたゼロオーバーヘッドの非同期/同期イベント駆動システムを提供する。
- **詳細要件:**
  - `Event<T>` および `Emitter<T>` による Publish/Subscribe パターンを実装すること。
  - イベントのフィルタリング (`Event::filter`)、マップ変換 (`Event::map`)、合成 (`Event::any`) を提供すること。
  - リスナー登録時に `IDisposable` を返却し、登録解除を容易にすること。

### REQ-BASE-003: 非同期制御・キャンセレーション (Async & Cancellation)
- **説明:** 高負荷な言語サーバー解析や検索処理を安全に中断・制御する機構を提供する。
- **詳細要件:**
  - `CancellationToken` および `CancellationTokenSource` による協調的キャンセルを提供すること。
  - `Throttler` (連続イベントの間引き実行)、`Delayer` (デバウンス実行)、`Limiter` (同時実行数制御キュー) を実装すること。

### REQ-BASE-004: 高度データ構造 (Advanced Data Structures)
- **説明:** エディタ・ファイルツリー・高速検索に必要な特化型データ構造を提供する。
- **詳細要件:**
  - `TernarySearchTree` (三項探索木): URI/パスのプレフィックス検索および最短/最長一致判定を O(K) で実現。
  - `LRUCache`: 一定容量を超えた古いキャッシュ要素を O(1) で自動パージ。
  - `ResourceMap`: ファイルパスの大文字小文字無視 (Windows/macOS) を考慮した正規化 Map。

### REQ-BASE-005: 差分比較アルゴリズム (Diff & LCS)
- **説明:** Git 差分やエディタ内編集履歴を算出するための高速差分エンジン。
- **詳細要件:**
  - Myers 差分アルゴリズムおよび最長共通部分列 (LCS) による行単位・文字単位の差分算出。
  - 巨大ファイルに対するタイムアウト制御とフォールバックアルゴリズムの実装。

### REQ-BASE-006: プロセス間通信プロトコル (IPC & RPC Protocol)
- **説明:** メインプロセス、Extension Host、レンダラ間の型安全かつバイナリレベルで高速な RPC プロトコル。
- **詳細要件:**
  - `IChannel`, `IChannelClient`, `IChannelServer` による双方向通信。
  - JSON-RPC 2.0 および MessagePack / バイナリストリーム対応。
