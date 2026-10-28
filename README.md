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
# ond-unit.toml
version = 1
case_files = ["cases/ond-unit.toml"]

[project]
path = "path/to/checks"
role = "Test"

[vm]
ram_size = 0x0001_0000
stack_size = 0x0000_1000
```

package ごとのケースを別ファイルにまとめる例です。\
root と子ファイルの設定項目は[規範仕様 §11](specs/ond-unit.md#11-実行設定の-schema)を参照してください。

```toml
# cases/ond-unit.toml
package = "checks/cases"

[[cases]]
name = "example"
function = "Run"
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
