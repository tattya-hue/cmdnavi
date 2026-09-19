# cmdnavi

`cmdnavi`は、自分で登録したコマンド文字列と説明をカテゴリ別に保存・参照する
Linux向けCLIツールです。登録されたコマンドを実行する機能はありません。

## インストール

### GitHubからインストール（推奨）

本コマンドのインストールにはRust 1.89以降とCargoが必要です。これらが未導入の場合は
[rustup](https://rustup.rs/)からインストールしてください。

本コマンドはソースをGitHubから直接取得してインストールします。

```sh
cargo install --git https://github.com/tattya-hue/cmdnavi.git --tag v1.0.0 --locked
```

実行ファイルは通常`~/.cargo/bin/cmdnavi`へ配置されます。インストール後に
コマンドが見つからない場合は、`~/.cargo/bin`が`PATH`に含まれていることを
確認してください。

```sh
cmdnavi --version
```

更新する場合は、新しいタグを指定して`--force`を付けます。

```sh
cargo install --git https://github.com/tattya-hue/cmdnavi.git --tag v1.1.0 --locked --force
```

アンインストール：

```sh
cargo uninstall cmdnavi
```

### ソースからインストール（開発者向け）

開発やソースの確認を行う場合はリポジトリをクローンします。

```sh
git clone https://github.com/tattya-hue/cmdnavi.git
cd cmdnavi
cargo install --path .
```

現時点では、crates.ioの`cargo install cmdnavi`や、Rustを必要としないビルド済み
バイナリの配布には対応していません。これらを提供する場合は、この節を更新します。

## 使い方

```sh
cmdnavi add network       # 対話形式で項目を追加
cmdnavi network           # networkカテゴリを表示
cmdnavi list              # カテゴリ一覧を表示
cmdnavi remove network    # 対話形式で項目を削除
cmdnavi edit network      # networkカテゴリだけをYAMLで編集
cmdnavi edit              # 設定全体をYAMLで編集
cmdnavi --help
cmdnavi --version
```

追加例：

```text
$ cmdnavi add network
Command: ip addr
Description: IPアドレスを確認する
Category "network" does not exist.
Created category "network".
Added command to "network".
```

表示例：

```text
$ cmdnavi network
ip addr
  IPアドレスを確認する
```

カテゴリ名には英数字、`_`、`-`を使用できます。`add`、`edit`、`list`、
`remove`、`help`、`version`は予約語です。

## 設定ファイル

`XDG_CONFIG_HOME`が設定されている場合：

```text
$XDG_CONFIG_HOME/cmdnavi/config.yaml
```

設定されていない場合：

```text
~/.config/cmdnavi/config.yaml
```

データ形式：

```yaml
network:
  - command: ip addr
    description: IPアドレスを確認する
  - command: ss -tulpn
    description: LISTEN中のポートを確認する
```

書き込み処理では同じディレクトリの`config.yaml.lock`を使用します。このロック
ファイルは残り続けますが正常な動作です。複数プロセスから同時に追加しても、
一方の更新で他方の更新が失われないようになっています。

## エディタ

エディタは次の優先順位で選択されます。

1. `$VISUAL`
2. `$EDITOR`
3. `vi`

例：

```sh
export VISUAL="code --wait"
cmdnavi edit network
```

存在しないカテゴリを指定した場合もエディタが開き、次の記入例がコメントとして
表示されます。

```yaml
# Add a command by removing the leading '#' characters.
# Both command and description are required.
# - command: ip addr
#   description: IPアドレスを確認する
```

先頭の`#`を削除し、内容を書き換えて保存するとカテゴリとコマンドが作成されます。
コメントを残したまま閉じた場合や、ファイルを空にした場合は何も作成されません。

初めて`cmdnavi edit`を実行した場合は、カテゴリ名を含む次の例を表示します。

```yaml
# cmdnavi configuration
# Add a category and command by removing the leading '#' characters.
# Both command and description are required.
# network:
#   - command: ip addr
#     description: IPアドレスを確認する
```

`command`と`description`は両方とも必須です。`command`だけを記入した場合や、
`description`を空にした場合は設定を変更せず、復旧用の編集ファイルを残します。

編集内容に問題がある場合は、同じファイルをもう一度開くか確認されます。

```text
the changes were not saved because the edited YAML is invalid
Choose Y below to reopen the same file, then correct the YAML and save it.

Your edits are still here:
  /tmp/cmdnavi-edit-xxxx.yaml

YAML message: ...

Open the same file again? [Y/n]:
```

Enterまたは`Y`を入力すると、編集した内容を保持したまま同じファイルを再び開きます。
修正して保存すると再検証され、正しければ設定へ反映されます。`n`を入力した場合は
設定を変更せずに終了し、復旧用ファイルの場所と再開方法を表示します。

編集後のYAMLが不正な場合、元の設定は変更されません。編集した一時ファイルの
パスが表示されるため、内容を復旧できます。

エディタを開いている間に同じカテゴリが別のプロセスから変更された場合も、
上書きせずに終了します。カテゴリ単位の編集では、別カテゴリへの同時変更は
保持されます。

## 終了コード

- `0`: 正常終了（削除確認でキャンセルした場合を含む）
- `1`: 引数、入力、設定、エディタ、ファイル操作などのエラー

エラーは標準エラー出力へ、通常の結果と対話プロンプトは標準出力へ出力されます。
エラー表示では、最初に何ができなかったかを示し、その直後に次に試す操作を
自然な文章で表示します。対象ファイルや技術的なメッセージは、必要な場合だけ
空行の後へ補足として表示します。例えば、存在しないカテゴリでは
`cmdnavi list`または`cmdnavi add`、エディタを起動できない場合は`$VISUAL`と
`$EDITOR`、設定ファイルを読み書きできない場合は対象パスと権限の確認を案内します。

## 開発時の確認

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```
