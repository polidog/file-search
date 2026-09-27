# file-search

[English](README.md) | [日本語](README.ja.md)

ソースコードをキーワードではなく**意味**で探します。「エラーを握りつぶしている箇所」「リトライしている処理」のように、grep では書けない問いで引けます。ファイルを関数くらいの塊に切り、塊ごとに [TypeSafe AI](https://typesafe.ai/) の System One モデル [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev) で採点します（[polidog/jev](https://github.com/polidog/jev) 経由）。

## インストール

```bash
cargo install --git https://github.com/polidog/file-search
```

## 使い方

```bash
export TYPESAFE_API_KEY=...
file-search "HTTP リクエストを送っている関数" ~/src/jev/src -n 3
# ファイルとディレクトリを混ぜて渡せる
file-search "HTTP リクエストを送っている関数" src/http.rs src/provider
```

```
★★★★★ 3.7  /home/you/src/jev/src/http.rs:9  pub fn post<T: DeserializeOwned>(
★★★★★ 3.7  /home/you/src/jev/src/provider/vercel.rs:8  pub struct Vercel;
★★★★☆ 3.3  /home/you/src/jev/src/provider/cloudflare.rs:17  impl Provider for Cloudflare {
```

ファイルの一覧をパイプで渡すと、候補を先に絞れます（`.gitignore` も効きます）。

```bash
rg -l "fn " | file-search "HTTP リクエストを送っている関数"
```

| オプション | 既定値 | |
| --- | --- | --- |
| `PATH...` | 標準入力、なければ `.` | 探すファイルかディレクトリ（複数可）。省略して標準入力にパスを流してもよい |
| `-n`, `--num` | `10` | 表示件数 |
| `-m`, `--max` | `200` | 採点する塊の数の上限（API 呼び出し量の歯止め） |
| `-p`, `--provider` | `typesafe` | `typesafe` / `cloudflare` / `vercel`（`JEV_PROVIDER` でも可） |

プロバイダごとの環境変数は [polidog/jev](https://github.com/polidog/jev#providers) を見てください。

## エージェント用スキル

Claude Code や Codex に、`file-search` をいつ・どう使うかを教えるスキルを書き出せます。

```bash
file-search --install-skill claude   # ~/.claude/skills/file-search/SKILL.md
file-search --install-skill codex    # ~/.codex/skills/file-search/SKILL.md
```

`CLAUDE_CONFIG_DIR` / `CODEX_HOME` を設定していればそちらに書きます。もう一度実行すると、バイナリに入っている版で上書きします。

## しくみ

- `PATH` のディレクトリを辿ります。隠しファイルと `target` / `node_modules` / `vendor` は飛ばし、バイナリと UTF-8 でないファイルは無視します。`.gitignore` は読みません。
- 1 MiB を超えるファイルは読みません。
- 空行のあとに行頭から始まる行で切ります（だいたいトップレベルの定義ごと）。3 行に満たない塊は次とつなげ、40 行を超えた塊は次の空行で切ります。`impl` / `class` の中のメソッドは 1 つずつには分かれません。
- 1 つの塊は 1000 文字までで切ります。
- 20 塊を 1 リクエストにまとめ、最大 8 リクエストを同時に送ります。

## ライセンス

[MIT](LICENSE)
