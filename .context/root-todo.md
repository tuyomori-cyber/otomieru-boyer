# Auto ToDo Root

## テーマ

- `otomieru-boyer` の DSP 再生処理について、Transport イベントと Processor の状態管理を整え、`BypassTimeStretchProcessor` を実際の `TimeStretchProcessor` に置き換える

## 目的

- `AudioPlayer` から DSP 処理までの Transport イベント連携を整理する
- `Seek` や `LoopJump` などの状態変化時に DSP 内部状態を安全に reset できる構成にする
- 最終的に time stretch の実処理を `Processor` として組み込む

## 前提認識

- `AudioPlayer -> DspEngine -> ProcessorGraph -> Processor` の責務分離で進めている
- `DspTransportEvent` に `Start / Stop / Seek / LoopJump` を定義済み
- `AudioPlayer` から `DspEngine`、`ProcessorGraph` へ Transport イベントを通知する経路を実装済み
- `ProcessorGraph` は Transport イベントを受けて Processor の状態を reset できる
- `BypassTimeStretchProcessor` は実処理の差し込み口となる `PreparedTimeStretchProcessor` に置き換わった経緯があるが、現在の `timestretch.rs` は原因切り分けのためにかなり単純化され、`PreparedTimeStretchProcessor` 自体は実質パススルー状態になっている
- `TimeStretchProcessor` / `PitchShiftProcessor` を実サンプル経路へ接続するためのインターフェース拡張は実施済み
- `DspEngine::render_sample()` を `ProcessorGraph` 経由にする変更は実施済み
- `player.rs` には `speed_ratio` を再生位置の進行量へ掛ける処理が残っている
- `app/mod.rs` では DSP 設定の反映が `!playing` のときだけ `player.set_dsp_settings(...)` される状態になっている
- 現状は「デフォルト音高まで壊れる」「速度も変わらない」という症状に対して、`timestretch` 本体だけでは説明しづらく、`DspEngine` / `player` の再生経路または設定反映経路をさらに単純化して切り分ける段階
- `PreparedTimeStretchProcessor` に内部状態、窓付き合成、`speed_ratio` 進行、簡易 WSOLA 寄りの重ね位置探索まで実装した段階はあったが、現在は原因切り分けのために簡素化されており、実処理として再構成する必要がある
- 原因切り分けのため、現在は再生経路を最小構成へ戻し、実サンプル取得を DSP グラフ経由から `DspEngine::render_source_sample()` による元音源の直接補間へ変更している
- 現在の復旧状態では `speed_ratio` のみを再生進行量へ直接反映し、time stretch / pitch shift の DSP 処理は再生経路から外している
- この最小構成への変更後、`cargo check` は通過している

## MVP ゴール

- Transport イベントに応じて DSP の状態を適切に初期化できる
- `BypassTimeStretchProcessor` を実際の `TimeStretchProcessor` に置き換える
- time stretch の内部状態と処理入口を実装する
- `speed_ratio` を解釈した速度変更の実処理を time stretch 側へ組み込む

## 非ゴール

- 現時点で未確定の高度な DSP アルゴリズムや最適化を先行して実装すること
- Transport イベント連携とは無関係な再生機能を同時に変更すること

## 段階的 ToDo

- [x] `DspTransportEvent` を追加し、`Start / Stop / Seek / LoopJump` のイベント境界を型で明示する
- [x] `AudioPlayer -> DspEngine -> ProcessorGraph` のイベント通知経路を実装する
- [x] `ProcessorGraph` が Transport イベントで reset できる構成を実装する
- [x] `cargo check` でビルド成立を確認する
- [x] `TimeStretchProcessor` / `PitchShiftProcessor` を実サンプル経路へ接続できるインターフェースに拡張する
- [x] `DspEngine::render_sample()` を `ProcessorGraph` 経由の本番形へ変更する
- [x] `PreparedTimeStretchProcessor` に `speed_ratio` を扱う内部状態と処理計画を追加する
- [x] `speed_ratio` と Transport イベントに応じて再同期できる time stretch の骨格を実装する
- [x] `SourceAudioView` を追加し、`TimeStretchProcessor` がソース音全体を参照できるようにする
- [x] `DspEngine::render_sample()` を `ProcessorGraph -> TimeStretchProcessor -> PitchShiftProcessor` の経路へ変更する
- [x] 現在の実サンプル経路で `1.00x / 0 st` の通常再生を確認し、既存の出音が壊れていないことを確認する
- [x] `PreparedTimeStretchProcessor` に内部状態と窓付き合成の骨格を実装する
- [x] 再生ヘッド進行へ `speed_ratio` を反映する
- [x] `PreparedTimeStretchProcessor` に簡易 WSOLA 寄りの重ね位置探索を追加し、非1.00時の音高ズレを減らす位置合わせを実装する
- [x] 再生経路を最小構成へ戻し、実サンプル取得を DSP グラフ経由から直接補間へ変更する
- [x] `speed_ratio` のみを再生進行量へ直接反映する復旧状態へ変更する
- [x] 復旧状態で `cargo check` が通過することを確認する
- [ ] 最小構成で `1.00x / 0.50x / 1.50x` の通常音高と速度変化を実機確認する
- [ ] 復旧確認後、proper な time stretch / pitch shift を最小構成から別経路で再実装する
- [ ] 実機で `1.00x / 0.75x / 1.25x` の音高一致とノイズ量を確認する
- [ ] 必要に応じて search radius / overlap / hop / 窓設計を調整する
- [ ] Transport イベントによる reset が実処理でも成立することを確認する

## 未解決論点

- 現在の `timestretch.rs` は実質パススルー状態であり、time stretch 実処理はまだ成立していない
- `player.rs` の `speed_ratio` は現在、最小構成で再生進行量へ直接反映する形に戻している
- `app/mod.rs` の DSP 設定反映が停止中限定のため、設定反映経路が速度変更未反映に関係しているか未確認
- 最小構成で通常再生の音高と `speed_ratio` による速度変更が正しく戻るか未確認
- 実機での `1.00x / 0.50x / 1.50x` の通常音高と速度変化は未確認
- proper な time stretch / pitch shift は、復旧確認後に別経路で再実装する必要がある
- 実機での `1.00x / 0.75x / 1.25x` の音高一致とノイズ量は未確認
- 簡易 WSOLA 寄りの重ね位置探索による改善は、現在の簡素化前の実装に対するものであり、現状コードでの有効性は未確認
- search radius / overlap / hop / 窓設計は実機確認後に調整が必要になる可能性がある
- 実処理へ再移行した後の Transport イベントによる reset の挙動確認が未実施

## 次の一手

1. 最小構成で `1.00x / 0.50x / 1.50x` の通常音高と速度変化を実機確認する
2. 復旧確認後、proper な time stretch / pitch shift を最小構成から別経路で再実装する
3. 実処理への再移行後、Transport イベントによる reset が成立することを確認する