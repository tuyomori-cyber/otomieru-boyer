# 外部依存関係

## Rubber Band Library

音程維持付き速度変更には [Rubber Band Library](https://breakfastquay.com/rubberband/) のリアルタイムAPIを使用する。

- Linux 開発時: `librubberband-dev` を導入する。
- 実行時: `librubberband.so` を動的リンクする。配布パッケージでは対応するライブラリを依存関係として宣言する。
- ライセンス: GPL-2.0-or-later。アプリケーション本体も GPL-2.0-or-later で公開する。

Rubber Band のソースとライセンスは公式配布元から取得できる。再配布時は、そのライセンス条件に従うこと。
