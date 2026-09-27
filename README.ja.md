# file-search

[English](README.md) | [日本語](README.ja.md)

ソースコードをキーワードではなく**意味**で探します。「エラーを握りつぶしている箇所」「リトライしている処理」のように、grep では書けない問いで引けます。ファイルを関数くらいの塊に切り、塊ごとに「探しているものか」の確率を [TypeSafe AI](https://typesafe.ai/) の System One モデル [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev) に聞きます（[polidog/jev](https://github.com/polidog/jev) 経由）。

## インストール

[Releases](https://github.com/polidog/file-search/releases/latest) から、Linux（x86_64 / aarch64、静的リンク）・macOS（Intel / Apple Silicon）・Windows 向けのバイナリを落として展開し、`file-search` を `PATH` の通った場所に置いてください。

Cargo でビルドすることもできます。

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
0.97  /home/you/src/jev/src/http.rs:9  pub fn post<T: DeserializeOwned>(
0.92  /home/you/src/jev/src/provider/vercel.rs:8  pub struct Vercel;
0.91  /home/you/src/jev/src/provider/typesafe.rs:6  pub struct TypeSafe;
```

各行は、確率・`path:行番号`・塊の 1 行目です。閾値に届くものが無ければ、grep と同じく終了コード 1 で終わります。

ファイルの一覧をパイプで渡すと、候補を先に絞れます（`.gitignore` も効きます）。

```bash
rg -l "fn " | file-search "HTTP リクエストを送っている関数"
```

送る前に料金の目安を出せます。

```bash
file-search "エラーを握りつぶしている箇所" src --dry-run
# 39 ファイル / 1007 塊 / 40 リクエスト / 入力 約 502585 トークン / 約 $0.0211
```

| オプション | 既定値 | |
| --- | --- | --- |
| `PATH...` | 標準入力、なければ `.` | 探すファイルかディレクトリ（複数可）。省略して標準入力にパスを流してもよい |
| `-n`, `--num` | `10` | 表示件数 |
| `-t`, `--threshold` | `0.5` | これ未満の確率は出さない |
| `-m`, `--max` | `1000` | 採点する塊の数の上限（API 呼び出し量の歯止め） |
| `--dry-run` | | 送らずに、リクエスト数・入力トークン数・料金の目安だけ出す |
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
- 塊ごとに「この行範囲のコードは探しているものか」を yes/no の問い（`noul`）で聞き、確率を受け取ります。
- リクエストの state には行番号付きのファイル全体を入れ、問いは行範囲で塊を指します。モデルは周りのコードも見られ、塊がいくつあってもファイルは 1 回送るだけで済みます。別のファイルの塊を同じリクエストに混ぜることはしません。64 KB を超えるファイルは複数のリクエストに分けます。
- 最大 16 リクエストを同時に送ります。429 と 5xx は間を空けて投げ直します（1, 2, 4, 8, 16 秒）。
- リクエストの組み方は、[mizchi/jev-lint](https://github.com/mizchi/jev-lint) が Jev について測った知見に沿っています。`--dry-run` の料金もその実測値（入力 100 万トークンあたり約 $0.042）から出す目安で、請求額そのものではありません。

## ライセンス

[MIT](LICENSE)
