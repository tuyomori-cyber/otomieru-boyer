# Otomieru Boyer

音声のスペクトログラム表示、区間ループ、音程維持付き速度変更を行うデスクトップアプリケーションです。

## 動作要件

- Rust（edition 2024 対応の安定版）
- Linux の音声出力環境
- Rubber Band Library 3.x の開発パッケージ

Ubuntu / Debian 系では、次で必要なネイティブ依存関係を導入できます。

```bash
sudo apt-get install librubberband-dev pkg-config
```

## ビルドとテスト

```bash
cargo run
cargo test
```

## 構成

- `src/audio/streaming.rs`: worker、固定長 SPSC リングバッファ、Rubber Band backend
- `src/audio/player.rs`: CPAL 出力と Transport 制御
- `src/audio/dsp_engine.rs`: 再生設定とストリーミング経路の接続
- `src/ui/`: egui による操作UIとスペクトログラム
- `docs/`: アーキテクチャと外部依存の方針

## ライセンス

本プロジェクトは [GNU General Public License v2.0 or later](LICENSE) で公開します。

速度変更の音程維持には Rubber Band Library を使用します。Rubber Band も GPLv2以降で利用します。詳細は [外部依存関係](docs/dependencies.md) を参照してください。
