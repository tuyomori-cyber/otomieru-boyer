# Otomieru Boyer

音声のスペクトログラムを見ながら、区間ループ、音程維持付き速度変更、オクターブシフト、5バンドEQを操作できるLinux向けデスクトップアプリケーションです。

![Otomieru Boyer screenshot](assets/screenshot.png)

## 主な機能

- 音声ファイルの読み込みと再生（WAV / MP3 / FLAC / OGG / M4A / AAC）
- スペクトログラムとピアノロールによる音高表示
- ループ範囲の作成・再生、停止・一時停止・シーク
- 音程を維持する速度変更：0.50x / 0.75x / 1.00x / 1.25x / 1.50x
- 独立したオクターブシフト：-1 oct / 0 st / +1 oct
- 5バンドEQ：100 Hz / 250 Hz / 1 kHz / 4 kHz / 10 kHz、各±12 dB
- 時間軸ズーム（ホイール）と音高方向ズーム（Ctrl＋ホイール）
- ウィンドウ高に追従するスペクトログラム・ピアノロール表示

## 操作

- スペクトログラム上でホイール：時間軸の拡大／縮小
- スペクトログラム上で Ctrl＋ホイール：音高方向の拡大／縮小
- 停止中にスペクトログラムをクリック：再生位置を移動
- タイムラインをドラッグ：ループ範囲を作成・調整
- `EQ`：5バンドEQの別ウィンドウを開く

速度、オクターブ、EQは再生停止中に変更できます。EQは再生音だけでなく、スペクトログラムの明るさにも反映されます。

EQの反映は次のように分かれています。

- 再生音：worker 内で実際にEQ処理
- スペクトラム表示：同じEQカーブを色強度へ反映
- 元の解析データ：保持したままなので軽量

つまり、再生で強調した帯域が画面でも明るくなる表示です。

## 動作要件

- Linux の音声出力環境
- Rust（edition 2024 対応の安定版）
- Rubber Band Library 3.x の開発パッケージ
- 日本語UI表示用の Noto Sans CJK または IPAex フォント

Ubuntu / Debian 系では次を導入します。

```
sudo apt-get install librubberband-dev pkg-config fonts-noto-cjk
```

## ビルドとテスト

```
cargo run
cargo test
```

詳細は [仕様書](仕様書.md)、[アーキテクチャ](docs/architecture.md)、[外部依存関係](docs/dependencies.md) を参照してください。

## ライセンス

本プロジェクトは [GNU General Public License v2.0 or later](LICENSE) で公開します。音程維持処理に利用する Rubber Band Library もGPL条件で利用します。
