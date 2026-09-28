# file-search

## 参考にしたものは references.json に書く

ほかのリポジトリのコード・設計・ドキュメント・測定結果を参考にしたら、その変更と**同じコミットで** `references.json` に 1 件足す。リスペクトのために、何を借りたかを構造化して残すためのファイルなので、「参考にしたけど書かない」はしない。

```json
{
  "repository": "owner/name",
  "url": "https://github.com/owner/name",
  "license": "MIT",
  "used_for": ["借りたものを具体的に 1 つずつ"],
  "files": ["読んだファイル (任意)"]
}
```

- `license` は推測しない。`gh api repos/owner/name --jq .license.spdx_id` で確かめる。無ければ `null`
- `used_for` は「参考にした」ではなく、何をどう取り入れたかを書く
- 既にあるリポジトリから新しく借りたら、その項目の `used_for` に足す
- Cargo.toml に入れただけの普通のクレートは書かなくてよい。コードや設計を読んで取り入れたときだけ書く
- `cargo test` が形を確かめる (`src/main.rs` の `references_are_structured`)
