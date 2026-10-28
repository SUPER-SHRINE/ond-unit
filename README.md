# ond-unit

ond-unit の規範資料です。

[規範仕様](specs/ond-unit.md)

## 実行

通常の `ond` コマンドを `PATH` に配置し、設定ファイルを指定して実行します。\
設定形式と判定条件は上記の規範仕様を参照してください。

```text
cargo run --offline -- path/to/ond-unit.toml
cargo run --offline -- path/to/ond-unit.toml --case example --fail-fast
```

設定ファイルの省略時はカレントディレクトリの `ond-unit.toml` を使用します。\
生成プロジェクトと通常のコンパイルキャッシュは対象プロジェクトの `target/ond-unit/` に保存されます。

対象 project と role 選択の条件は[規範仕様 §3](specs/ond-unit.md#3-ケース登録と関数)を参照してください。\
対象の追加 role を選択するときは、設定の `[project]` に `role` を記述します。

```toml
[project]
path = "path/to/checks"
role = "Test"
```

## 開発時の検証

隣接する `ond` と `kagura` のチェックアウトを参照してビルドします。

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
```

実コンパイラを使う統合テストは、通常の `ond` コマンドを `PATH` に配置して明示的に実行してください。\
テスト用プロジェクトはこのリポジトリの `target/` 内に作成されます。

```text
cargo test --offline --test cli -- --ignored
```
