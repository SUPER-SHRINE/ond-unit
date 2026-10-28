# ond-unit 規範仕様

## 1. 適用範囲

本書は ond-unit が TOML に登録されたケースを制御可能な Kagura 環境で実行し、その結果を判定する契約を定める。\
定義対象は、設定、ケース登録、生成、起動、観測、実行上限、および判定である。\
本書に記載のない事項は未定義であり、実装裁量として補完しない。

ond-unit は利用者が記述した通常の Ond プログラムを通常の Ond compile と link により実行する。\
本書の RAM 初期値設定は、§4 のロード契約を維持する入力を対象とし、ロード契約が定めるヘッダやセクションなどの内容を書き換えるケースは適用範囲に含めない。

## 2. 用語

**実行設定** は `ond-unit.toml` に記述する一つの実行環境とケース一覧を指す。

**ケース** は実行設定に登録する一つの実行単位を指す。

**ホスト** は、ケースの選択、Kagura VM の初期化と実行、および結果判定を行う ond-unit の実行制御部分を指す。

## 3. ケース登録と関数

実行対象のケース集合は §11 の `cases` で定める。\
登録順に 0 から始まる `u32` の index を各ケースに割り当て、ホストは登録済みの有効な index を選択する。

`project.path` の対象は、`ond.toml` の `[project].final` が省略または `false` である通常 Ond project とする。\
`final = true` の対象は、ケース関数の配置先が root package または subpackage のいずれであっても、生成専用プロジェクトを更新する前に ERROR とする。\
final の型と省略値、および依存 graph の成立条件は [Ond project role 仕様](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/build/roles.md) に従う。

ケース関数の型は引数なし、戻り値なしの公開トップレベル関数 `func()` とする。\
通常の import が可能な任意の package に配置でき、補助関数には通常の可視性規則を適用する。\
同一 package 内の非公開補助関数も使用できる。\
利用者の検証は通常 Ond の式、制御、trap、および利用者自身の補助関数で記述する。

設定に記載された関数が存在しない、可視でない、または指定された型に一致しない場合、VM 実行前に ERROR とする。\
関数の存在、可視性、および通常 Ond 関数型との一致は通常の Ond compile で検証する。

ond-unit は登録順と登録配置情報から、ケースの import 宣言、関数ポインタ配列、`main()`、および起動引数読出しを含む生成ソースを作る。\
ホストと生成ソースは同一のケース一覧および配置情報を使用する。

生成ソースは利用者ソースを編集せず、`project.path` の project 内の `target/ond-unit/` に生成専用プロジェクトとして置く。\
生成専用プロジェクトは `[project].final = true` とし、対象 project を直接の path 依存として登録する。\
`project.role` を指定した場合はその依存 edge の `role` に同じ値を設定し、省略時は edge の `role` も省略して対象の role 未指定構成を選択する。\
この選択を実行設定の全ケースと対象 project の全 package に適用する。\
対象の宣言済み role との一致、依存先への選択と project 境界、および複数依存経路の整合は上記の Ond project role 仕様に従い、違反は VM 実行前に ERROR とする。\
source の選択と可視性は [Ond role 構文と可視性](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/language/v1/roles.md) に従う。\
生成 `main()` は生成専用プロジェクトの通常 role 文脈に置き、選択された対象の公開 API を通常の project 間参照として利用する。

各実行時に現在の設定と登録先に合わせて生成専用プロジェクトを更新し、保存する。\
通常の compile cache は、入力が一致する場合に利用できる。

## 4. コンパイル成果物と外部規範

実行するプログラムは通常の Ond compile と link が生成する ONDX を使用する。\
compile 成果物の Contract は [Ond artifacts 仕様の版識別と互換条件](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/artifacts.md#版識別と互換条件) に従う。\
ONDX の形式、版識別、ロードおよび配置契約は [Ond artifacts 仕様](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/artifacts.md) に従う。

実行対象は Ond target `kagura-v1` とし、ABI は `ond-kagura-v1` とする。\
CPU と bus は [Kagura target README](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/targets/kagura-v1/README.md) が参照する Kagura 規範仕様 1.0.0 に従う。\
ABI 契約は [ond-kagura-v1 ABI 仕様](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/targets/kagura-v1/abi.md) に従う。\
fault の意味分類とメモリ契約は [Kagura machine 仕様](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/targets/kagura-v1/machine.md) に従う。\
debug metadata の形式と lookup は §9 の外部規範を定義元とする。

## 5. VM メモリと起動

物理 RAM はアドレス 0 から連続する `N = vm.ram_size` byte とする。\
実行に ROM を使用せず、ONDX 全体を RAM に配置して外部起動する。\
グローバル初期化と `main()` 呼出しは通常の実行時処理とし、nil の物理的な対応も通常の target 規則に従う。

起動引数領域は物理 RAM 末尾の半開区間 `[N - 8, N)` に置く、8-byte aligned の 8 byte とする。\
通常の link に渡す RAM は base 0、size `N - 8` とし、stack 予約量には `vm.stack_size` を渡す。\
プログラム、グローバル、ヒープ、およびスタックの配置は、この通常 link 対象範囲内で §4 の配置契約に従う。\
SP とエントリには、その link 結果の ONDX ヘッダ値を使用し、ホスト、link、および生成ソースは同じ起動引数領域を使用する。

| 起動引数領域内の offset | 内容 |
| --- | --- |
| `0..4` | 選択ケースの `u32` index の little-endian 表現 |
| `4..8` | すべて `0` |

ホストはケースごとの初期化で RAM のロードと 0 初期化を完了し、§7 の RAM 初期値を設定した後、上表の 8 byte を書き込む。\
その後 ONDX のエントリから実行を開始し、生成された `main()` が index に対応するケース関数を呼び出す。

## 6. 正常終了先

ケース関数の通常の return 後は、link の exit jump に相当する通常の終了経路を通る。

正常終了先は、32-bit アドレス空間内の半開区間 `[a, a + 4)` が物理 RAM およびすべての共通 MMIO 領域と重ならず、`a` が 4-byte aligned であるもののうち、`a` が最大のものとする。\
アドレス空間の範囲は `[0, 2^32)`、最大の候補は `0xFFFF_FFFC` であり、区間端と加算は数学的整数として扱う。\
条件を満たす正常終了先を選択できない場合、VM 実行前に ERROR とする。

ホストは PC が正常終了先と一致したことを次の命令取得より前に検出し、正常終了として扱う。\
正常終了先の 4 byte を命令取得せず、通常のテスト用途およびデバイス用途から分離する。

## 7. ケースごとの初期状態と観測

各ケースの開始時に、CPU、RAM、ヒープ、スタック、デバイス、モック、応答列の消費位置、観測列、および実行ステップ数を独立した初期状態にする。\
各ケースは同じ生成バイナリから実行できる。

ケースが fault で停止した後、ホストはプログラムの実行を再開しない。\
ホストは停止時の状態および停止までに完了した作用を観測し、後続ケースを新しい初期状態から実行する。

### 7.1. RAM の初期値と期待値

`cases.ram_initial` はエントリ実行前に設定する byte 列、`cases.ram_expected` はケース停止時に比較する byte 列を表す。\
両者の各領域は `address` と `bytes` で指定し、`bytes` の 1 要素を常に 1 byte とする。\
要素 `i` は `address + i` に対応し、配列は昇順アドレスへの直接指定である。\
整数の表記桁数、先頭の `0`、または `0x` の有無から幅を推測せず、配列全体をエンディアンに応じて並べ替えない。

TOML 整数の 16 進表記と `_` 区切りを使用できる。\
例えば `address = 0x0000_1000`、`bytes = [0x78, 0x56, 0x34, 0x12]` は、アドレス `0x1000`、`0x1001`、`0x1002`、`0x1003` にそれぞれ `0x78`、`0x56`、`0x34`、`0x12` を対応させる。\
この領域の 32-bit 読出し結果 `0x12345678` は §4 の Kagura little-endian 規則から定まる。

RAM 期待値は、指定された全 byte と停止時 RAM の対応 byte が一致した場合に成立する。

### 7.2. 共通 MMIO とケース固有の応答・期待

`mmio` は全ケースに共通するメモリマップを定義し、各 device は `[base, base + size)` を占める。\
`cases.mmio` は共通 device 名を参照する、ケース固有の stub または mock の設定とする。\
その読出し応答列 `read_responses` と期待 access 列 `expected_accesses` は独立した列である。

各 device は、読み出しのたびにそのケースの次の応答を 1 件消費し、指定幅の値を返す。\
応答 record の幅と実際の読出し幅が一致しない場合、モック不一致の FAIL として実行を停止する。\
書き込みは値を受理し、後続の読出し値は応答列から得る。\
幅 1、2、4 byte の access はそれぞれ Kagura の同じ幅の不可分な 1 回の bus 操作として扱い、複数の byte access に分割しない。\
multi-byte 値のバイト順と access の alignment、成立条件、および fault 時の作用は §4 の CPU と bus 規範に従う。

`expected_accesses` がある場合、その device に対して完了した読み書きの観測列と、操作、絶対アドレス、幅、値、順序、および件数がすべて一致したときにモック期待が成立する。\
順序の照合対象は device ごとの列とする。\
空列は当該 device に access がないことを期待し、省略は当該 device の access 列を照合しない stub とする。\
応答列の残件数は期待 access 件数を定義せず、回数の期待には `expected_accesses` を用いる。

読出し要求に対応する応答列が枯渇した場合、ホストは不足した device 名、access のアドレスと幅、および応答列内の位置を報告して実行を停止し、モック不一致の FAIL とする。\
本節の実行停止を伴う FAIL は、期待 fault の一致によって PASS にならない。

## 8. 終了期待と判定

`expect_fault` の省略は `Normal`、指定は `Fault` の終了期待を表す。

| 期待終了 | 実際の終了 | 終了条件 |
| --- | --- | --- |
| `Normal` | 正常終了 | 成立 |
| `Normal` | fault | 不成立 |
| `Fault` | fault が §9 の指定条件に一致 | 成立 |
| `Fault` | 正常終了または fault が指定条件に不一致 | 不成立 |

設定、ビルド、またはホスト実行環境の障害は ERROR とする。\
実行上限への到達は §10 の STEP_LIMIT、MMIO の実行停止を伴う不一致は §7.2 の FAIL に従う。\
これら以外の終了では、終了条件と指定された RAM およびモックの期待がすべて成立した場合に PASS とし、いずれかが不成立の場合に FAIL とする。

## 9. Fault の照合

期待 fault の設定項目と必須条件は §11.3 に定める。\
CPU の生の fault 種類 `cpu_kind` と、Ond の意味分類 `ond_kind` を区別する。\
CPU の生の `InvalidInstruction` のみから Ond の意味分類を推定しない。

| `cpu_kind` | Kagura CPU fault code |
| --- | --- |
| `InvalidInstruction` | `INVALID_INSTRUCTION` |
| `UnalignedAccess` | `UNALIGNED_ACCESS` |
| `UnalignedPc` | `UNALIGNED_PC` |
| `BusFault` | `BUS_FAULT` |

関数名と source path は debug metadata の文字列と完全一致で比較する。\
`source.path` は debug metadata の opaque な表示識別子として扱い、filesystem path として解決・正規化しない。

fault の照合には実際に fault した命令の `faulting_pc` と debug metadata を用い、照合元は次表に定める。\
FunctionDebug と DebugRange の選択、debug operation と CPU fault の対応は外部規範に従う。

Ond の意味分類は §11.3 の `ond_kind` の値を対象とし、§4 の target fault 対応表に従って、生の CPU fault code と `faulting_pc` が示す停止命令に対応する debug operation に裏付けられた場合に成立する。

| 条件 | 照合元 |
| --- | --- |
| `cpu_kind` | CPU が返した生の fault code |
| `ond_kind` | `faulting_pc` により選択した `DebugRange.operation` と CPU fault の対応 |
| `function` | `faulting_pc` の function lookup で得た `FunctionDebug.symbol` |
| `source.path`、`source.line`、`source.column` | `faulting_pc` により選択した `DebugRange.source` の対応 field |
| `reason` | 同じ `DebugRange.message` |
| `address`、`width`、`operation` | CPU fault result の address、byte 単位の access width、操作 |

すべての指定条件が一致した fault のみを期待 fault と一致させる。\
関数と source の両方を指定した場合も AND とし、準備処理または別の箇所の fault は同じ規則で判定する。\
利用者コードが発生させる trap も通常の fault として照合し、fault コードは本来の CPU fault コードのままとする。

debug metadata v1 は全ケースで生成および読み込みを行い、対象イメージとの識別子照合を必須とする。\
その形式と lookup の契約は [Debug metadata v1 仕様](https://github.com/SUPER-SHRINE/ond/blob/2b532ebef205d65d19024d5352c876293b090129/specs/ond/debug-metadata/v1.md) に従う。\
sidecar の欠落、不適合、またはイメージ識別子の不一致は ERROR とする。\
指定された条件の照合に必要な FunctionDebug、DebugRange、または source の情報を得られない場合も ERROR とし、使用しない任意情報の欠落はこの ERROR の条件に含めない。\
CPU fault result が指定の address、width、operation を持たない場合、および `message` が `null` で `reason` と一致しない場合は、対応条件の不一致とする。

## 10. 実行上限

既定ステップ上限は `1_000_000` とする。\
正の整数 `cases.max_steps` の指定は当該ケースの既定値を丸ごと置き換え、既定値より小さくても大きくてもよく、他ケースには影響しない。

1 命令の実行の試行を 1 step とし、fault した命令の試行も 1 step に数える。\
ONDX のエントリから計数を開始するため、グローバル初期化、生成 `main()`、およびケース関数の実行を含む。\
ホストによるロードと初期値設定は step に含めず、step の加算によって CPU fault code を変更しない。

上限を `L` とすると、`L` 回目の命令の試行まで実行できる。\
その試行が fault した場合は fault の終了判定を優先し、試行完了後の PC が §6 の正常終了先なら正常終了の判定を優先する。\
終了せず次の命令の試行が必要な場合、次の試行前に STEP_LIMIT とする。

STEP_LIMIT は PASS、FAIL、および ERROR と別の状態であり、診断には上限値、実行ステップ数、および停止位置を含める。

## 11. 実行設定の schema

実行設定は root の `version`、`project`、`vm`、`mmio`、`cases` と、以下の表に定める field からなる閉じた schema とする。\
key は lower_snake_case とし、未知 key、型不正、必須項目の欠落、および各表の成立条件に反する値は設定 ERROR である。\
表中の integer は TOML 整数を指し、範囲の計算は数学的整数で行う。\
`project.path` の相対 path は設定ファイル所在 directory を基準に解決する。

### 11.1. Root と共通環境

| Table | Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- | --- |
| root | `version` | integer、`1` | 必須 |
| root | `project` | table | 必須 |
| root | `vm` | table | 必須 |
| root | `mmio` | table の配列 | 省略時は空列 |
| root | `cases` | table の配列、index が §3 の型に収まる件数 | 必須 |
| `project` | `path` | 非空 string、§3 の対象条件を満たす Ond project の directory path | 必須 |
| `project` | `role` | 非空 string、§3 の選択 role | 省略時の構成は §3 |
| `vm` | `ram_size` | integer、物理 RAM の byte 容量、§5 の配置条件を満たす `8 < N <= 2^32`、8 の倍数 | 必須、既定値なし |
| `vm` | `stack_size` | integer、通常 link 対象 RAM の stack 予約 byte 数、§4 の配置契約に適合 | 必須、既定値なし |
| `mmio` の各要素 | `name` | 非空 string、共通 device の一意な名前 | 必須 |
| `mmio` の各要素 | `base` | integer、`0..0xFFFF_FFFF` の絶対 byte address | 必須 |
| `mmio` の各要素 | `size` | 正の integer、`base + size <= 2^32` | 必須 |

共通 MMIO 領域は物理 RAM および他の共通 MMIO 領域と重ならないものとする。\
通常 link 対象 RAM の容量不足など、§4 の link 成立条件を満たさない場合は VM 実行前に ERROR とする。

### 11.2. ケース

| `cases` の Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- |
| `name` | 非空 string、実行設定内で一意なケース名 | 必須 |
| `package` | 非空 string、通常 Ond の import 先 package path | 必須 |
| `function` | 非空 string、当該 package の通常 Ond の関数名 identifier | 必須、関数の条件は §3 |
| `max_steps` | 正の integer | 省略時は §10 の既定上限 |
| `expect_fault` | §11.3 の table | 省略時の終了期待は §8 |
| `ram_initial` | §11.4 の table の配列 | 省略時は初期値設定なし |
| `ram_expected` | §11.4 の table の配列 | 省略時は RAM 照合なし |
| `mmio` | §11.5 の table の配列 | 省略時は §11.5 の設定なし device の規則を適用 |

### 11.3. 期待 fault

| `cases.expect_fault` の Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- |
| `cpu_kind` | §9 の `cpu_kind` の string | `ond_kind` と少なくとも一方必須、省略時は生の種類を照合しない |
| `ond_kind` | string、`"BoundsCheck"`、`"DivisionByZero"`、`"SignedDivisionOverflow"`、`"InvalidConversion"`、`"StackOverflow"`、`"ExplicitTrap"` | `cpu_kind` と少なくとも一方必須、省略時は意味分類を照合しない |
| `function` | 非空 string、`FunctionDebug.symbol` と比較する関数名 | `source` と少なくとも一方必須、省略時は関数名を照合しない |
| `source` | 下表の table | `function` と少なくとも一方必須、省略時は source 位置を照合しない |
| `reason` | string、空 string も可 | 省略時は理由を照合しない |
| `address` | integer、`0..0xFFFF_FFFF` の絶対 byte address | 省略時はアドレスを照合しない |
| `width` | integer、byte 単位の `1`、`2`、`4` | 省略時は幅を照合しない |
| `operation` | string、`"fetch"`、`"load"`、`"store"` | 省略時は操作を照合しない |

| `cases.expect_fault.source` の Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- |
| `path` | 非空 string、§9 の source 識別子 | 必須 |
| `line` | integer、`1..0xFFFF_FFFF` の 1 行 | 必須 |
| `column` | integer、`1..0xFFFF_FFFF`、外部規範の UTF-8 byte 単位の列 | 省略時は列を照合しない |

### 11.4. RAM 領域

次表を `cases.ram_initial` と `cases.ram_expected` の各要素に共通して適用し、値の配置と比較は §7.1 に従う。

| Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- |
| `address` | integer、`0..0xFFFF_FFFF` の絶対 byte address | 必須 |
| `bytes` | integer の配列、各要素は `0..255` | 必須 |

領域 `[address, address + len(bytes))` は §5 の通常 link 対象 RAM 内に収める。\
同一ケースの RAM 初期値領域同士は重ならないものとし、初期値が対象とするロード契約の範囲は §1 に従う。

### 11.5. ケース固有の MMIO

| `cases.mmio` の Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- |
| `device` | string、共通 `mmio.name` と完全一致する参照 | 必須、同一ケースで同じ device の設定は 1 件 |
| `read_responses` | 下表の応答 record の配列 | 省略時は空列 |
| `expected_accesses` | 下表の期待 access record の配列 | 省略時は §7.2 の stub |

当該ケースの `cases.mmio` に現れない共通 device にも、空の応答列を用い、access 列の照合を行わない。

| 応答 record の Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- |
| `width` | integer、byte 単位の `1`、`2`、`4` | 必須 |
| `value` | integer、`0 <= value < 2^(8 * width)` | 必須 |

| 期待 access record の Key | 型・値 | 必須・省略時の動作 |
| --- | --- | --- |
| `operation` | string、`"read"` または `"write"` | 必須 |
| `address` | integer、絶対 byte address、指定 device 内に access 全体が収まる | 必須 |
| `width` | integer、byte 単位の `1`、`2`、`4` | 必須 |
| `value` | integer、`0 <= value < 2^(8 * width)` | 必須 |

### 11.6. 設定例

以下は schema に従う設定であり、package、関数、および source 識別子は対象 project の値を指定する。\
例の RAM 領域は、対象 project の配置が §1 と §11.4 の成立条件を満たす場合に適用する。

```toml
version = 1

[project]
path = "."

[vm]
ram_size = 0x0001_0000
stack_size = 0x0000_1000

[[mmio]]
name = "port"
base = 0x1000_0000
size = 4

[[cases]]
name = "port_roundtrip"
package = "checks/port"
function = "Roundtrip"
max_steps = 2_000_000

[[cases.ram_initial]]
address = 0x0000_1000
bytes = [0x78, 0x56, 0x34, 0x12]

[[cases.ram_expected]]
address = 0x0000_1000
bytes = [0x78, 0x56, 0x34, 0x12]

[[cases.mmio]]
device = "port"
read_responses = [{ width = 4, value = 0x1234_5678 }]
expected_accesses = [
  { operation = "read", address = 0x1000_0000, width = 4, value = 0x1234_5678 },
  { operation = "write", address = 0x1000_0000, width = 4, value = 0xABCD_0001 },
]

[[cases]]
name = "bounds_fault"
package = "checks/bounds"
function = "OutOfBounds"

[cases.expect_fault]
cpu_kind = "InvalidInstruction"
ond_kind = "BoundsCheck"

[cases.expect_fault.source]
path = "checks/bounds/main.ond"
line = 12
```

## 12. CLI

CLI の構文は次のとおりとする。

```text
ond-unit [CONFIG] [--case NAME]... [--fail-fast]
```

`CONFIG` は実行設定ファイルの path であり、省略時はカレント directory の `ond-unit.toml` を使用する。\
`--case NAME` があれば、その名前で選択された登録ケースの集合を実行し、省略時は全ケースを対象にする。\
存在しないケース名の指定は VM 実行前に ERROR とする。\
選択にかかわらず実行順は TOML の登録順とし、§3 の index を維持する。

CLI はケース名、失敗理由、および集計を人向けに出力する。\
既定では FAIL または STEP_LIMIT の後も後続ケースを実行し、`--fail-fast` 指定時はそのケースの後で実行を終了する。\
終了コードは、ERROR がある場合は `2`、ERROR がなく FAIL または STEP_LIMIT がある場合は `1`、対象全ケースが PASS の場合は `0` とする。
