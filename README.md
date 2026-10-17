# cmdnavi

`cmdnavi`は、自分で登録したコマンド文字列と説明をカテゴリ別に保存・参照する
Windows・Linux向けCLIツールです。登録されたコマンドを実行する機能はありません。

## インストール

### 実行ファイルをダウンロード（推奨）

[GitHub Releases](https://github.com/tattya-hue/cmdnavi/releases/tag/v1.1.0)のAssetsから取得できます。
Rustのインストールは不要です。

| OS | ダウンロードするファイル |
| --- | --- |
| Windows x64 | `cmdnavi-v1.1.0-x86_64-pc-windows-msvc.zip` |
| Linux x86_64（glibc 2.35以降、Ubuntu 22.04以降など） | `cmdnavi-v1.1.0-x86_64-unknown-linux-gnu.tar.gz` |

WindowsではZIPを展開し、そのフォルダでPowerShellを開いて実行します。

```powershell
.\cmdnavi.exe --version
.\cmdnavi.exe
```

Linuxでは展開して実行します。

```sh
tar -xzf cmdnavi-v1.1.0-x86_64-unknown-linux-gnu.tar.gz
cd cmdnavi-v1.1.0-x86_64-unknown-linux-gnu
./cmdnavi --version
./cmdnavi
```

どこからでも使いたい場合は、実行ファイルを置いたフォルダを`PATH`に追加してください。
WSLではLinux版を使います。Windows版とWSL版の設定ファイルは別々です。
ARM64・32ビット版やAlpine Linux（musl）向けの実行ファイルは含みません。

各アーカイブの`.sha256`も配布します。Linuxでは同じフォルダにダウンロードし、
`sha256sum -c <アーカイブ名>.sha256`で検証できます。
Windowsでは`Get-FileHash <ZIPファイル名> -Algorithm SHA256`の値を`.sha256`の値と比較します。

### GitHubのソースからインストール

本コマンドのインストールにはRust 1.89以降とCargoが必要です。これらが未導入の場合は
[rustup](https://rustup.rs/)からインストールしてください。

本コマンドはソースをGitHubから直接取得してインストールします。

```sh
cargo install --git https://github.com/tattya-hue/cmdnavi.git --tag v1.1.0 --locked
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

crates.ioの`cargo install cmdnavi`には対応していません。

## 使い方

```sh
cmdnavi                   # 対話画面で検索・コピー・編集
cmdnavi add               # カテゴリ選択から対話形式で登録
cmdnavi add network "ip addr" "IPアドレス確認" # 一行で登録
cmdnavi add network "ip addr" # 説明を省略して登録
cmdnavi --search IP       # カテゴリ・コマンド・説明を横断検索
cmdnavi add network       # 対話形式で項目を追加
cmdnavi network           # networkカテゴリを表示
cmdnavi list              # カテゴリ一覧を表示
cmdnavi remove network    # 対話形式で項目を削除
cmdnavi edit network      # networkカテゴリだけをYAMLで編集
cmdnavi edit              # 設定全体をYAMLで編集
cmdnavi --help
cmdnavi --version
```

ターミナルで引数なし、または`cmdnavi --browse`を実行すると対話画面が開きます。

| 操作 | キー |
| --- | --- |
| カテゴリ・コマンド・説明から絞り込む | 文字を入力（日本語も可） |
| 検索語を途中編集する | ← / → / Home / End / Delete |
| 項目を選ぶ | ↑ / ↓ |
| 選んだコマンドをコピー | Enter |
| 登録 / 編集 / 削除 | Ctrl+A / Ctrl+E / Ctrl+D |
| 項目を移動 / カテゴリ名を変更 | Ctrl+G / Ctrl+N |
| 直前の変更を取り消す・やり直す | Ctrl+Z |
| 再読み込み（外部からの更新は自動反映） | Ctrl+R |
| 検索文字を消す | Ctrl+U |
| 長いコマンド・説明をスクロール | PageUp / PageDown |
| ヘルプ・エラー全文を読む | F1（↑↓でスクロール） |
| 終了する | Esc / Ctrl+C |

対話画面では空白で区切った複数の検索語すべてを含む項目を表示します。
コマンドと説明は詳細欄で折り返して表示し、日本語の文字幅も考慮します。
`NO_COLOR`を設定すると色を使いません。画面が小さい場合は拡大方法を案内します。
Windowsのコピー機能は標準のWindows PowerShellを使用します。
WSLではWindows PowerShellへのフォールバックも使います。コピー処理中も対話画面は操作可能です。
LinuxのコピーにはWaylandで`wl-copy`（wl-clipboard）、X11で`xclip`または`xsel`が必要です。
利用できない場合も画面内に理由を表示し、閲覧を続けられます。

出力をパイプやファイルへ送る場合や`TERM=dumb`の場合は、引数なしでも
カテゴリと件数の通常表示になります。`cmdnavi network`や`cmdnavi list`も従来どおり使えます。

`cmdnavi add`では既存カテゴリの`#番号`か新しい名前を入力します。
Enterだけなら`general`に保存します。コマンドが空欄ならその場で入力し直せます。
説明はEnterで省略でき、入力中は`/cancel`で中止できます。
同じカテゴリに同じコマンドがあれば重複追加しません。説明の省略時は既存の説明を保持し、
明示的に説明を変更した場合だけ更新を確認します。

対話画面のCtrl+E、または`cmdnavi edit network 1`で1件ずつ編集できます。
番号は対話画面のカテゴリ名の横に表示される番号です。
ターミナルでは現在値が入力済みで開き、左右キーなどで直接修正できます。
Enterで確定し、説明を消す場合は入力欄を空にするか`/clear`を入力します。
複数行の内容をまとめて編集する場合は`cmdnavi edit network`のYAML編集が使えます。

`--search`は大文字・小文字を区別せず、
日本語の説明も検索できます。空白を含む検索語は`cmdnavi --search "IP address"`のように
引用符で囲んでください。検索結果には所属カテゴリを表示します。

削除では、番号を選ぶ段階でEnterまたは`q`を入力するとキャンセルできます。
最後の確認で`y`または`yes`を入力した場合だけ削除します。
`cmdnavi add --help`など、各操作からもヘルプを確認できます。

追加例：

```text
$ cmdnavi add network
Add to [network] · Tab completes saved commands · Ctrl+C or /cancel cancels
Command: ip addr
Description (optional, Enter to skip): IPアドレスを確認する
Created category "network".
Saved command in [network].
```

表示例：

```text
$ cmdnavi network
[network] · 1 commands

[1]
ip addr
  IPアドレスを確認する
```

カテゴリ名には英数字、`_`、`-`を使用できます。`add`、`edit`、`list`、
`remove`、`help`、`version`は予約語です。

## 補完・入力編集

Bash・Zsh・Fish・PowerShell向けに、入力位置に応じた動的補完を提供します。
Bashで今のシェルへ読み込む場合:

```bash
source <(cmdnavi --completions bash)
```

操作名の位置には操作名・カテゴリ、カテゴリ引数にはカテゴリ、項目指定やコマンド入力欄には
そのカテゴリの登録済みコマンドを出します。説明欄・検索語にはコマンド候補を出しません。
登録・編集フォームのTab補完と左右キーによる編集は追加設定なしで利用できます。
各シェルへの保存方法と具体例は[補完・入力編集の説明](docs/completion.md)を参照してください。

## 項目・カテゴリの整理

```sh
cmdnavi --show network                 # 項目番号付きで表示
cmdnavi --show -ops                    # '-'で始まるカテゴリも表示可能
cmdnavi --print network "ip addr"       # コマンド本文だけを出力（末尾改行も追加しない）
cmdnavi --copy network 1               # 1件をコピー
cmdnavi edit network "ip addr"          # 番号だけでなく登録済み本文でも指定
cmdnavi remove network 1               # 確認して削除
cmdnavi --move network 1 tools         # 1件を別カテゴリへ移動
cmdnavi --rename-category tools ops    # カテゴリ名を変更
cmdnavi --remove-category ops          # 確認してカテゴリ全体を削除
cmdnavi --undo                         # 直前の保存を取り消す（再実行でやり直す）
cmdnavi --config-path                  # 実際の保存先を確認
```

項目には1から始まる番号、または保存したコマンド本文を指定します。空白がある本文は引用符で囲んでください。
番号は現在のカテゴリ内での並び順なので、項目の削除・移動で変わります。
同じ本文が重複している場合や数値だけのコマンドを指定する場合は、一覧の項目番号を使ってください。

保存時には同じフォルダの`config.yaml.undo`へ直前と保存後の状態を記録します。
取り消しは1段階で、対象はcmdnavi経由で行った直前の変更です。
外部でYAMLが変更されていた場合は上書きせずに停止します。
バックアップにも登録内容が含まれるため、データを移すときは必要に応じて一緒に扱ってください。

## 設定ファイル

`XDG_CONFIG_HOME`が設定されている場合：

```text
$XDG_CONFIG_HOME/cmdnavi/config.yaml
```

Linuxで設定されていない場合：

```text
~/.config/cmdnavi/config.yaml
```

Windowsでは`XDG_CONFIG_HOME`を設定していなければ、次に保存します。

```text
%APPDATA%\cmdnavi\config.yaml
```

`APPDATA`がない場合は`%USERPROFILE%\AppData\Roaming`を使います。

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
3. Windowsでは`notepad.exe`、Linuxでは`vi`

例：

```sh
export VISUAL="code --wait"
cmdnavi edit network
```

Windowsでエディタにフルパスを指定する場合、実行ファイルのパスだけならそのまま指定できます。
引数も付ける場合は、パスをシングルクォートで囲んでください。

```powershell
$env:VISUAL = "'C:\Program Files\Vim\vim91\gvim.exe' -f"
```

存在しないカテゴリを指定した場合もエディタが開き、次の記入例がコメントとして
表示されます。

```yaml
# Add a command by removing the leading '#' characters.
# Command is required; description is optional.
# - command: ip addr
#   description: IPアドレスを確認する
```

先頭の`#`を削除し、内容を書き換えて保存するとカテゴリとコマンドが作成されます。
コメントを残したまま閉じた場合や、ファイルを空にした場合は何も作成されません。

初めて`cmdnavi edit`を実行した場合は、カテゴリ名を含む次の例を表示します。

```yaml
# cmdnavi configuration
# Add a category and command by removing the leading '#' characters.
# Command is required; description is optional.
# network:
#   - command: ip addr
#     description: IPアドレスを確認する
```

`command`は必須です。`description`は省略または空文字列（`description: ""`）にできます。
`command`が空の場合やYAMLが不正な場合は設定を変更せず、復旧用の編集ファイルを残します。

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

通常のCLI操作では、エラーは標準エラー出力へ、結果と対話プロンプトは標準出力へ出力されます。
対話画面でのコピー失敗や入力エラーは画面内に表示します。
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
cargo build
python3 tests/terminal_smoke.py # Linuxの疑似端末で対話操作を検証
python3 tests/usability_smoke.py # 入力編集・貼り付け・Bash/Zsh/Fish補完を検証
```


## GitHub Releasesへの公開（メンテナ向け）

`.github/workflows/release.yml`はLinux・Windowsの両方で検査・テスト・ビルドし、
実行ファイルとREADME・LICENSE・リリースノートをアーカイブ化します。
通常のpush・PRではActionsのArtifactsに保存し、`v*`タグのpush時だけGitHub Releasesへ公開します。
両OSのジョブが成功してからアップロードし、アップロード完了後にドラフトを公開します。

1. `Cargo.toml`・`Cargo.lock`のバージョンと`RELEASE_NOTES.md`を更新します。
2. 変更をコミットしてGitHubにpushし、Actionsの成功を確認します。
3. そのコミットにタグを付けてpushします。タグはCargoのバージョンと一致させます。

```sh
git tag -a v1.1.0 -m "Release v1.1.0"
git push origin v1.1.0
```

リポジトリでGitHub Actionsを有効にしてください。公開ジョブは`contents: write`権限の
`GITHUB_TOKEN`を使用します。追加の個人アクセストークンは不要です。
公開に失敗した場合は原因を直して該当ワークフローを再実行できます。

ローカルで同じアーカイブを作る場合（各OSのネイティブ環境）：

```sh
# Linux
cargo build --release --locked --target x86_64-unknown-linux-gnu
python3 scripts/package.py --target x86_64-unknown-linux-gnu
```

```powershell
# Windows
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --locked --target x86_64-pc-windows-msvc
python scripts/package.py --target x86_64-pc-windows-msvc
```

パッケージ作成にはPython 3.11以降を使用します。成果物は`dist/`に保存されます。
配布用Linux版はUbuntu 22.04のActions環境でビルドし、glibcの必要バージョンを揃えます。
