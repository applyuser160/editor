# Oxide Editor のテスト探索

コマンドパレットの **Testing: Run Tests** は、アクティブワークスペースのプロジェクトファイルを検出し、対応するテストスイートを表示します。選択したスイートはアクティブルートで実行され、出力は Output パネルに表示されます。

| 検出ファイル                                              | テストスイート    | 実行コマンド       |
| --------------------------------------------------------- | ----------------- | ------------------ |
| `Cargo.toml`                                              | Rust: cargo test  | `cargo test`       |
| `package.json`                                            | Node.js: npm test | `npm test`         |
| `pyproject.toml`、`pytest.ini`、または `requirements.txt` | Python: pytest    | `python -m pytest` |

テストプロセスの出力に `error` または `warning` が含まれる場合は Problems として抽出します。`file:line:column: error: message` の形式で出力するツールは、ファイル位置も抽出できます。

現在の実装はテストスイートの検出と実行に焦点を当てています。個別テストのツリー表示、再実行・デバッグ操作、テスト失敗からのエディター遷移、watch mode、フレームワーク固有のテストアダプターは後続の実装対象です。テストの実行はワークスペースのコマンドを起動するため、信頼できるフォルダーでのみ実行してください。
