# jev-sift

[English](README.md) | [日本語](README.ja.md)

ソースコードをキーワードではなく**意味**で探します。「エラーを握りつぶしている箇所」「リトライしている処理」のように、grep では書けない問いで引けます。ファイルを関数くらいの塊に切り、塊ごとに「探しているものか」の確率を [TypeSafe AI](https://typesafe.ai/) の System One モデル [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev) に聞きます（[polidog/jev](https://github.com/polidog/jev) 経由）。

## 向いているところ

Claude Code（Sonnet）と比べて測りました。

- **1 か所を探すなら、Claude Code に聞くほうが確かです。** [cli/cli](https://github.com/cli/cli)（Go 1,000 ファイル）で「〜しているのはどこ？」を 7 問聞くと、Claude Code は jev-sift なしで 7 問とも当てました。jev-sift を使わせても正答は変わらず、料金が 16% 増えました。jev-sift が役に立つのは、人が 1〜3 秒・数セントで当たりをつけたいときです。
- **たくさんの中から、当てはまるものを全部ふるい出すのは得意です。** [banking77](https://github.com/PolyAI-LDN/task-specific-datasets)（銀行への問い合わせ 10,003 件）で、3 つの問いに当てはまる問い合わせを全部挙げさせました。

  | | F1 | 料金（1 問） | 時間（1 問） |
  | --- | --- | --- | --- |
  | jev-sift（閾値 0.5） | 0.58〜0.67 | 約 $0.09 | 2〜4 秒 |
  | jev-sift（閾値を問いごとに調整） | 0.61〜0.80 | 約 $0.09 | 2〜4 秒 |
  | Claude Code | 0.71〜0.83 | $1.1〜3.9 | 3〜8 分 |

  Claude Code がいちばん良かった問いは、サブエージェント 5 つに全件を読ませて $3.9 かかっていました。件数が増えるほど差は開きます。全部を出すときは `-n` を大きくしてください。

## インストール

[Releases](https://github.com/polidog/jev-sift/releases/latest) から、Linux（x86_64 / aarch64、静的リンク）・macOS（Intel / Apple Silicon）・Windows 向けのバイナリを落として展開し、`jev-sift` を `PATH` の通った場所に置いてください。

Cargo でビルドすることもできます。

```bash
cargo install --git https://github.com/polidog/jev-sift
```

## 使い方

```bash
export TYPESAFE_API_KEY=...
jev-sift "HTTP リクエストを送っている関数" ~/src/jev/src -n 3
# ファイルとディレクトリを混ぜて渡せる
jev-sift "HTTP リクエストを送っている関数" src/http.rs src/provider
```

```
0.97  /home/you/src/jev/src/http.rs:9  pub fn post<T: DeserializeOwned>(
0.92  /home/you/src/jev/src/provider/vercel.rs:8  pub struct Vercel;
0.91  /home/you/src/jev/src/provider/typesafe.rs:6  pub struct TypeSafe;
```

各行は、確率・`path:行番号`・塊の 1 行目です。閾値に届くものが無ければ、grep と同じく終了コード 1 で終わります。

ファイルの一覧をパイプで渡すと、候補を先に絞れます（`.gitignore` も効きます）。

```bash
rg -l "fn " | jev-sift "HTTP リクエストを送っている関数"
```

送る前に料金の目安を出せます。

```bash
jev-sift "エラーを握りつぶしている箇所" src --dry-run
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

Claude Code や Codex に、`jev-sift` をいつ・どう使うかを教えるスキルを書き出せます。

```bash
jev-sift --install-skill claude   # ~/.claude/skills/jev-sift/SKILL.md
jev-sift --install-skill codex    # ~/.codex/skills/jev-sift/SKILL.md
```

`CLAUDE_CONFIG_DIR` / `CODEX_HOME` を設定していればそちらに書きます。もう一度実行すると、バイナリに入っている版で上書きします。

上の cli/cli の測定では、スキルを入れても Claude Code は自分から jev-sift を呼びませんでした（7 問中 0 回）。rg で足りる問いだったためです。

## しくみ

- `PATH` のディレクトリを辿ります。隠しファイルと `target` / `node_modules` / `vendor` は飛ばし、バイナリと UTF-8 でないファイルは無視します。`.gitignore` は読みません。
- 1 MiB を超えるファイルは読みません。
- 空行のあとに行頭から始まる行で切ります（だいたいトップレベルの定義ごと）。3 行に満たない塊は次とつなげ、40 行を超えた塊は次の空行で切ります。`impl` / `class` の中のメソッドは 1 つずつには分かれません。
- 塊ごとに「この行範囲のコードは探しているものか」を yes/no の問い（`noul`）で聞き、確率を受け取ります。
- リクエストの state には行番号付きのファイル全体を入れ、問いは行範囲で塊を指します。モデルは周りのコードも見られ、塊がいくつあってもファイルは 1 回送るだけで済みます。別のファイルの塊を同じリクエストに混ぜることはしません。64 KB を超えるファイルは複数のリクエストに分けます。サーバーに大きすぎると断られたリクエスト（`max_tokens_exceeded`）は、塊を半分に割って投げ直します。
- 最大 16 リクエストを同時に送ります。429 と 5xx は間を空けて投げ直します（1, 2, 4, 8, 16 秒）。
- リクエストの組み方は、[mizchi/jev-lint](https://github.com/mizchi/jev-lint) が Jev について測った知見に沿っています。`--dry-run` の料金もその実測値（入力 100 万トークンあたり約 $0.042）から出す目安で、請求額そのものではありません。

参考にしたリポジトリは [references.json](references.json) にまとめています。

## ライセンス

[MIT](LICENSE)
