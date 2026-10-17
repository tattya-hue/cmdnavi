# 開発とリリース

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
git tag -a v1.3.0 -m "Release v1.3.0"
git push origin v1.3.0
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
