# cmdnavi

「あのコマンド、どう書くんだっけ？」をすぐに引ける、自分用のコマンドメモです。
コマンドに説明を添えてカテゴリ別に保存し、必要なときに検索できます。Windows・Linux対応。

## インストール

[GitHub Releases](https://github.com/tattya-hue/cmdnavi/releases/latest)から、使っているOSのファイルをダウンロードして展開します。

| OS | ファイル |
| --- | --- |
| Windows x64 | `…-x86_64-pc-windows-msvc.zip` |
| Linux x86_64（Ubuntu 22.04以降など） | `…-x86_64-unknown-linux-gnu.tar.gz` |

展開したフォルダで、Windowsなら `.\cmdnavi.exe`、Linuxなら `./cmdnavi` で起動できます。
普段使うには、実行ファイルを置いたフォルダを `PATH` に追加してください。
以下は `cmdnavi` で起動できる状態での例です。

Cargoが使える場合は、次のコマンドでもインストールできます（Rust 1.89以降）。

```sh
cargo install --git https://github.com/tattya-hue/cmdnavi.git --tag v1.3.0 --locked
```

## 登録して、必要なときに引く

### まず1件、登録する

```sh
cmdnavi add
```

保存先のカテゴリを選び、コマンドと説明を入力します。新しいカテゴリは名前を入力して作れます。
説明は空欄でも構いません。

例えば、`network` に `ip addr` を「IPアドレスを確認する」と登録しておきます。
一行で登録することもできます。

```sh
cmdnavi add network "ip addr" "IPアドレスを確認する"
```

### 必要になったら、検索する

```sh
cmdnavi
```

画面が開いたら、`IP` など思い出せる言葉を入力します。カテゴリ名・コマンド・説明から絞り込めます。
**↑↓で選び、Enterでコマンドと説明の詳細を表示**します。

| やりたいこと | キー |
| --- | --- |
| コマンド自体をコピーする（詳細画面） | c |
| 詳細から一覧へ戻る／画面を閉じる | Esc |
| 画面内の操作を確認する | F1（詳細画面では ? も可） |

詳細画面では `e` で編集、`d` で削除、`m` で移動できます。使えるキーは画面下部に表示します。

cmdnaviは参照用のツールです。登録したコマンドを実行することはありません。

### メモを整理する

```sh
cmdnavi edit       # 内容を編集する
cmdnavi remove     # 確認して削除する
cmdnavi --move     # 別のカテゴリへ移す
```

いずれも画面の案内に沿って、カテゴリとコマンドを選びます。
コマンドは本文や説明で検索できるので、番号を調べる必要はありません。
カテゴリが分かっていれば、`cmdnavi edit network` のように指定すると選択を省略できます。

## もう少し便利に使うには

シェルのTab補完を有効にすると、操作名やカテゴリを入力しやすくなります。
設定方法は[補完と入力編集](docs/completion.md)を参照してください。

保存先を確認したいときは `cmdnavi --config-path`。
ほかの操作は `cmdnavi --help`、詳しいオプションは `cmdnavi --help-all` で確認できます。

開発・公開に参加する方は[開発とリリース](docs/development.md)を参照してください。
