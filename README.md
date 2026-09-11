# Otomieru Boyer

音声のスペクトログラムを見ながら、区間ループ、音程維持付き速度変更、オクターブシフト、5バンドEQを操作できるLinux向けデスクトップアプリケーションです。

![Otomieru Boyer screenshot](assets/screenshot-main.png)

## 主な機能

- 音声ファイルの読み込みと再生（WAV / MP3 / FLAC / OGG / M4A / AAC）
- スペクトログラムとピアノロールによる音高表示
- ループ範囲の作成・再生、停止・一時停止・シーク
- 音程を維持する速度変更：0.50x / 0.75x / 1.00x / 1.25x / 1.50x
- 独立したオクターブシフト：-1 oct / 0 st / +1 oct
- 5バンドEQ：100 Hz / 250 Hz / 1 kHz / 4 kHz / 10 kHz、各±12 dB
- 時間軸ズーム（ホイール）と音高方向ズーム（Ctrl＋ホイール）
- スケール・任意の12音指定を補助情報にした、表示専用の基音強調
- ウィンドウ高に追従するスペクトログラム・ピアノロール表示

## 描画性能

縮小表示時にも再生線を滑らかに追従させるため、スペクトログラムは表示幅（物理ピクセル数）に合わせて時間方向の解析フレームを集約して描画します。各描画列には対応する時間範囲の表示強度の最大値を使い、短いピークを残しながら描画矩形数を抑えます。拡大時は解析フレームごとに描画します。スペクトログラム本体はテクスチャへキャッシュし、再生中はそのテクスチャと再生線だけを描画します。表示範囲、表示サイズ、音程範囲、ゲイン、基音強調、音階の強調・減衰、EQが変わったときだけキャッシュを作り直します。最大縮小時のFPS改善は実機確認が必要です。

## 操作

- スペクトログラム上でホイール：時間軸の拡大／縮小
- スペクトログラム上で Ctrl＋ホイール：音高方向の拡大／縮小
- 停止中にスペクトログラムをクリック：再生位置を移動
- スペクトログラムを押下：対応する音を試聴し、左のピアノ鍵をピンクで表示
- `調整・設定`：EQ、基音強調、音色を1つのダイアログで同時に操作する
- `試聴音量`：スペクトログラムを押している間に鳴る試聴音の音量を調整する
- タイムラインをドラッグ：ループ範囲を作成・調整

## 同梱音源

スペクトログラム押下時の試聴音には、FreePats の [Piano FB small](https://freepats.zenvoid.org/Piano/honky-tonk-piano.html) を使用しています。サンプル、SFZ対応表、ライセンス文は [`assets/piano-fb`](assets/piano-fb) に同梱しています。配布元のCC0 1.0パブリックドメイン・デディケーションに従い利用しています。

シンセストリングスには、同じくCC0の [FreePats Synth Strings #1](https://freepats.zenvoid.org/Synthesizer/synth-strings.html) を使用しています。サンプル、SFZ対応表、ライセンス文は [`assets/synth-strings-1`](assets/synth-strings-1) に同梱しています。

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
