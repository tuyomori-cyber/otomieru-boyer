# 外部依存関係

## Rubber Band Library

音程維持付き速度変更には [Rubber Band Library](https://breakfastquay.com/rubberband/) のリアルタイムAPIを使用する。

- Linux 開発時: `librubberband-dev` を導入する。
- 実行時: `librubberband.so` を動的リンクする。配布パッケージでは対応するライブラリを依存関係として宣言する。
- ライセンス: GPL-2.0-or-later。アプリケーション本体も GPL-2.0-or-later で公開する。

Rubber Band のソースとライセンスは公式配布元から取得できる。再配布時は、そのライセンス条件に従うこと。

## 同梱試聴音源

これらは実行時に別途導入する外部依存ではなく、試聴機能のためにアプリケーションへ同梱する第三者素材である。

| 音源 | 配布元 | ライセンス | 同梱先 |
| --- | --- | --- | --- |
| Piano FB small | [FreePats](https://freepats.zenvoid.org/Piano/honky-tonk-piano.html) | CC0 1.0 | `assets/piano-fb` |
| Synth Strings #1 | [FreePats](https://freepats.zenvoid.org/Synthesizer/synth-strings.html) | CC0 1.0 | `assets/synth-strings-1` |

各ディレクトリには、サンプルに加えて配布元のSFZ対応表とライセンス文を保持する。CC0の条件に従い、再配布を含めて利用する。
