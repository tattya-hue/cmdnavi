# 補完と入力編集

シェル補完は一度有効にすると、Tabを押した位置に応じて候補を出します。
候補はその都度設定ファイルから読み取るため、カテゴリやコマンドを追加した後に補完ファイルを作り直す必要はありません。

## 候補が出る場所

| 入力例（`<Tab>`はキー操作） | 候補 |
| --- | --- |
| `cmdnavi a<Tab>` | `add`などの操作名 |
| `cmdnavi net<Tab>` | `network`などのカテゴリ |
| `cmdnavi edit net<Tab>` | カテゴリ。登録済みコマンドは出さない |
| `cmdnavi add network ip<Tab>` | `network`内の登録済みコマンド。別カテゴリのコマンドや番号は出さない |
| `cmdnavi edit network ip<Tab>` | `network`内の登録済みコマンド。番号でも指定可能 |
| `cmdnavi --copy network <Tab>` | `network`内の項目 |
| `cmdnavi --move network 1 <Tab>` | 移動先カテゴリ |
| `cmdnavi add network "ip addr" <Tab>` | 説明欄なので登録済みコマンドは出さない |
| `cmdnavi --search <Tab>` | 検索語の欄なので登録済みコマンドは出さない |

補完は`cmdnavi`に対して登録します。シェルで通常のコマンドを入力する場所に、cmdnaviの登録内容を混ぜることはありません。
空白や引用符がある候補は、各シェルで1つの引数になる形で挿入します。保存した本文を実行して候補を作る処理はありません。

## Bash

現在のシェルで有効にします（Bash 4.4以降）。

```bash
source <(cmdnavi --completions bash)
```

継続して使う場合は次のようにファイルへ保存し、`~/.bashrc`から読み込んでください。

```bash
mkdir -p ~/.config/cmdnavi
cmdnavi --completions bash > ~/.config/cmdnavi/completion.bash
# この行を ~/.bashrc に追加
source ~/.config/cmdnavi/completion.bash
```

## Zsh

```zsh
autoload -Uz compinit
compinit
source <(cmdnavi --completions zsh)
```

継続して使う場合は`cmdnavi --completions zsh > ~/.config/cmdnavi/completion.zsh`で保存し、
`~/.zshrc`の`compinit`より後で`source ~/.config/cmdnavi/completion.zsh`を実行してください。
このファイルは直接sourceする形式です。

## Fish

Fish 3.7以降で次の場所へ保存すると、新しいシェルで読み込まれます。

```fish
mkdir -p ~/.config/fish/completions
cmdnavi --completions fish > ~/.config/fish/completions/cmdnavi.fish
source ~/.config/fish/completions/cmdnavi.fish
```

## PowerShell

Windows PowerShell 5.1、PowerShell 7で使用できます。Windows側にインストールした`cmdnavi.exe`が`PATH`上にある状態で実行してください。

```powershell
$completionDir = Join-Path $env:APPDATA 'cmdnavi'
New-Item -ItemType Directory -Force $completionDir | Out-Null
cmdnavi --completions powershell | Set-Content -Encoding UTF8 (Join-Path $completionDir 'completion.ps1')
. (Join-Path $completionDir 'completion.ps1')
```

継続して使う場合は`$PROFILE`のファイルに次の行を追加してください。

```powershell
. (Join-Path $env:APPDATA 'cmdnavi/completion.ps1')
```

補完候補はTabで切り替えられます。PSReadLineのメニュー表示を使う場合は、利用者の好みに合わせて次を設定できます。

```powershell
Set-PSReadLineKeyHandler -Key Tab -Function MenuComplete
```

シェルのプロファイルや既存のキー設定はcmdnaviから自動変更しません。

## 登録・編集フォーム

`cmdnavi add`や対話画面のCtrl+Aで開くフォームでは、別の設定なしで入力編集と補完が使えます。

- カテゴリ欄: Tabでカテゴリ名を補完。新しい名前を入力して作成することも可能。
- コマンド欄: Tabでそのカテゴリの登録済みコマンドを補完。
- 項目選択欄: Tabでそのカテゴリの登録済み項目・番号を補完。
- 説明欄・確認欄: コマンド補完なし。
- ↑↓: その欄の候補を入力履歴として選ぶ。別の種類の欄の履歴は混ぜない。
- ←→、Home/End、Delete、Backspace: 入力内容を編集。
- Ctrl+Cまたは`/cancel`: 操作を中止。入力履歴をディスクへ別途保存することはない。

編集フォームは現在の値が入力済みの状態で開きます。Enterで確定し、説明を空にする場合は欄を消すか`/clear`を入力します。
ブラケット付き貼り付けに対応した端末では複数行も1つの欄に貼り付けられ、最後のEnterで確定します。
シェルや端末による複数行貼り付けの確認が出る場合は、その端末の案内に従ってください。

標準入力をパイプする場合は従来の行入力になります。複数行のコマンドは、次のように引数で渡すかYAMLで編集してください。

```bash
cmdnavi add example $'echo first\necho second' '2行の例'
```

対話画面の検索欄は通常のカーソル編集に対応しますが、コマンドを入力する欄ではないため、登録済みコマンドのTab補完は行いません。
