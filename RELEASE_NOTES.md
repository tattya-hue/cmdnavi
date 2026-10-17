## cmdnavi v1.1.0

Windows・Linux向けの実行ファイルを配布します。Rustのインストールは不要です。

### ダウンロード

- **Windows（x64）**: `cmdnavi-v1.1.0-x86_64-pc-windows-msvc.zip`
- **Linux（x86_64、glibc 2.35以降）**: `cmdnavi-v1.1.0-x86_64-unknown-linux-gnu.tar.gz`
- 各アーカイブの `.sha256` ファイルはダウンロード内容の検証用です。

展開したフォルダで、Windowsは `./cmdnavi.exe`、Linuxは `./cmdnavi` をターミナルから実行してください。
ARM64・32ビット版およびAlpine Linux（musl）向けのバイナリは含みません。

### 改善点

- 文字を入力しながら絞り込める対話画面。日本語検索・長い説明の折り返し・スクロールに対応。
- カテゴリを選んで登録、説明の省略、一行での登録、重複確認。
- 選択した項目をCtrl+Eで編集、Enterでコピー。Ctrl+Aで追加、Escで終了。
- Windowsの設定保存先（`%APPDATA%\cmdnavi\config.yaml`）、メモ帳、クリップボードに対応。
- 従来のカテゴリ表示、横断検索、YAML編集にも対応。

Linuxのコピー機能には `wl-copy`、`xclip`、`xsel` のいずれかが必要です。
WindowsはWindows PowerShellを使用します。登録したコマンドを実行する機能はありません。

### 既存ユーザー向け

既存の設定ファイルは引き続き利用できます。説明（`description`）は省略できるようになりました。
ターミナルで引数なしに起動すると対話画面が開きます。パイプ出力時は通常のテキスト表示です。
WSLで保存した設定はLinux側にあり、Windows版の設定とは別です。
