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
- 停止中に `speed_ratio` 用の加工済み再生バッファを作成し、再生時はソース時間軸を維持したまま加工済みバッファを参照する構成へ変更済み
- この構成変更後、`cargo check` は通過している
- 停止中に毎フレーム `set_dsp_settings()` が走り加工済みバッファを再生成していたため、CPU 1 コア張り付き・メモリ増加・UI フリーズにつながる問題があった
- `app/mod.rs` で前回適用した DSP 設定を保持し、設定が変わったときだけ加工済みバッファを再構築するデバウンスを追加済み
- デバウンス追加後も `cargo check` は通過している
- 実機ではまだ UI フリーズ・メモリ増加の解消と、`0.50x / 1.50x` の音程維持・ノイズ量を確認していない
- 現在は安定基準線へ復帰し、フリーズとメモリ増加が止まったことを実機で確認済み
- OLA ベースのオフライン time stretch は CPU・メモリ増加を招いたため撤回した
- OLA 実装の CPU 暴走は、長尺音源全体に対する同期処理と Hann 窓の大量再計算が原因と特定した
- メモリ増加は巨大な `output` / `weights` バッファを段階的に書き込む挙動による可能性が高く、継続的なリークとは限らないことを確認した
- 最新実装では設定変更時のみ再生成するデバウンスが既に存在していたため、今回の主因は毎フレーム再生成ではなく、設定変更1回の処理自体が過大だった
- OLA の単体テストは小規模入力では通過しているが、長尺音源に対する処理時間・ピークメモリ・UI 応答性は未検証だった
- 再実装する場合は、UI スレッドで音源全体を同期変換せず、長さ比例の巨大な加工済みバッファを常時保持しない方式が必要
- 再実装候補として、固定長リングバッファと worker を使うストリーミング方式を検討する
- 音程維持付き speed change は、worker と固定長リングバッファによるストリーミング方式で実装する方針を確定した
- 実用性を優先し、speed change では音高を維持して統一する方針とする
- UI、worker、音声コールバック、Transport の責務を分離し、UI スレッドでは DSP 計算をせず、音声コールバックではメモリ確保・Mutex・DSP 計算を行わない方針とする
- 加工済み音源全体を保持せず、DSP 追加メモリは音源長に依存しない固定長とする
- 音声コールバックは固定長 SPSC リングバッファから読み出すだけとし、残量不足時は無音を出して underrun を記録する
- worker の処理結果には `generation_id` を持たせ、シーク・停止・設定変更後に古い生成結果を破棄できるようにする
- UI、シーク、ループは元音源フレームを基準に維持し、加工バッファの逆数時間軸参照は廃止する
- `TimeStretchBackend` を再生制御から分離し、ストリーミング対応の実績あるライブラリを第一候補とする。実用性を優先して Rubber Band のリアルタイムモードを第一候補とし、ライセンス・Linux 配布方法を確認して採否を決める
- Rubber Band を採用する方針とし、GPL 公開に合わせて `LICENSE`、依存ライセンス表記、ビルド手順、Linux の動的ライブラリ依存など、公開ソースとして分かりやすいプロジェクト構成へ整理する
- Rubber Band は GPLv2以降または商用ライセンスのデュアルライセンスであるため、今回の GPL 公開方針では GPLv2以降での公開・配布条件への適合性を確認する必要がある
- 商用利用は想定しておらず、アプリ公開は行う想定であるため、Rubber Band を採用する場合は GPLv2以降での公開・配布条件への適合性を確認する必要がある
- Rubber Band のリアルタイム backend 接続に必要な Linux システムライブラリ `librubberband-dev` は、開発環境への導入がまだ完了していない
- 独自 WSOLA を採用する場合は、固定サイズの作業領域を再利用し、`process()` 中に `Vec` を生成せず、音源長に比例した配列を作らない。窓係数は事前計算して再利用する
- `Start / Pause / Stop / Seek / LoopJump` に応じて、先読み、バッファ維持・破棄、DSP 再同期を行う
- 再生準備は `Idle → Preparing → Ready → Playing` を基本とし、先読み完了後に再生を許可する構成を検討する
- 計測対象として worker 処理時間、リングバッファ残量、underrun 回数、DSP 確保メモリ、prepare 完了時間を追加する
- `0.50x / 0.75x / 1.25x / 1.50x` で音程差 ±10 cent 以内、先読み完了後の underrun 0 回、音声コールバック内のヒープ確保 0 回などを合格条件とする
- 全体加工バッファと逆数時間軸参照を廃止する方針を確定した
- パススルー worker と固定長 SPSC リングバッファを最初に実装し、UI、音声コールバック、シーク、停止の安定性を確認してから time stretch backend を接続する
- パススルー worker、固定長 SPSC リングバッファ、generation 管理、underrun・残量・確保量の基本計測を実装済み
- `player.rs` の音声コールバックはリングから読むだけの構成とし、バッファ不足時は無音を出して再生位置を進めない
- Seek / Stop / LoopJump 後は古い世代の音声を破棄する構成を実装済み
- `dsp_engine.rs` から全体加工バッファ方式を外した
- `cargo test`、`cargo check`、差分チェックを通過している
- 長尺音源での UI 応答性・メモリ使用量・underrun の実機確認は未実施
- worker は現時点ではパススルーであり、実際の音程維持アルゴリズムは未接続
- 基本 Transport 操作、通常再生、停止／再生／一時停止、シーク、ループ、5分超の長尺音源再生は実機で正常動作を確認済み
- ループで開始位置へ戻る際に必ず無音が挟まるため、UX 上の問題として解消が必要
- 速度変更時のメモリ使用量は問題ないが、再生中の worker が CPU をかなり専有している
- スペクトラム画面の表示について、デバッグ表示 `stream ready / … fr / … underrun` は `PlayerSnapshot.debug_summary` に入るだけで UI 側では参照されておらず、スペクトラムへの重なりの原因ではない
- スペクトラム画面の日本語文字化けについては、eframe の日本語フォント未設定が原因候補だが、現時点では確定していない
- ループ無音は `LoopJump` ごとに世代更新と旧リング内容の破棄を行い、ループ先頭を worker が作り直す現在の構成により発生している
- worker はリング残量に応じてほぼ連続生成しているため、パススルー段階でも CPU を継続的に専有しており、低水位／高水位とブロック単位の生成・待機が必要
- worker は再生開始後に起動するのではなく、音源読み込み時点で起動し、停止中・一時停止中からもリングバッファへの先読みを行う
- `Play` は worker の起動ではなく、先読み済みリングバッファの読み出し開始として動作している
- `Pause` では読み出しを停止し、worker はリングが満杯になるまで先読みした後に待機する
- `Stop / Seek / LoopJump` では世代を切り替え、該当位置から先読みし直す
- ループ時は到達後に `LoopJump` してから先頭を生成し直すため、先頭音が事前にリングへ用意されておらず、無音が発生している
- 通常のクリックによる `Seek` でも、世代切替と旧リング内容の破棄後にクリック先から worker が生成し直すため、リングが一時的に空になり無音が発生する
- Seek は「古い位置の音を混ぜない」ことを優先しているため、現状は切替直後の無音を許容する設計になっている
- クリックによる Seek では、generation の更新と新しい位置からの生成指示自体はクリック直後に行われているが、停止中に旧世代データでリングが満杯のまま残っているため、worker は空きができるまで新世代データを書けない。再生開始後に旧世代データを読み出して世代不一致を無音化するため、無音が発生している
- Seek 後の無音は「生成開始が遅い」のではなく「旧世代リングを即時に無効化・クリアせず、新世代の先読みが再生開始に間に合っていない」ことが直接の原因である
- クリックによる Seek は generation を切り替える境界として扱い、generation 更新・旧 generation のリング内容破棄・新位置からの先読み・`Preparing → Ready` を一体で行う必要がある
- Seek は短い先読みが完了してから再生を再開する `Preparing → Ready → Playing` 型の扱いが必要であり、ループでは先頭を事前先読みしてリングを空にしない方式が必要
- Seek 後の旧 generation のリング内容は停止中にも即時に破棄するよう修正し、worker がクリック先の新 generation を先読みできる状態にした
- Seek 修正後、`cargo test`（3件）と `cargo check` は通過している
- 最新の実機確認では、Seek 後に無音なしで良好に再生できた
- ループも実機上はシームレスにつながることを確認した。ただし、ループ先頭を専用に事前先読みする実装はまだなく、現状のバッファ構成で無音が知覚されない状態であるため、実装上の保証はまだない
- Rubber Band 3.3 のリアルタイム backend を worker に接続し、`time_ratio = 1 / speed`、`pitch_scale = 1.0` として音程維持付き speed change の経路を実装済み
- Rubber Band の backend 接続後も音声コールバックはリング読み出しのみとし、DSP 処理は worker 側で行う構成を維持している
- GPL 公開用の `LICENSE`、`README.md`、`docs/architecture.md`、`docs/dependencies.md` を追加し、公開ソース向けのライセンス・設計・依存関係・ビルド情報を整理済み
- Rubber Band backend 接続後も `cargo test`（4件）と `cargo check` は通過している
- 実機では `1.00x` および `0.75x / 0.50x / 1.25x / 1.50x` の音程維持、開始時の無音、ノイズ、CPU使用率、underrun は未確認
- Rubber Band のリアルタイム R3 エンジンを使用しているため、パススルー時より CPU 使用量が増える可能性があり、実機結果を基に品質設定と worker の水位制御を調整する
- Rubber Band のリアルタイム backend は高速化のため R3 から R2 engine へ変更済みで、音程維持の基本設定は維持している
- worker は低水位（約200ms）まで待機し、高水位（約400ms）までまとめて生成する方式へ変更済みで、連続的な小刻み処理と過剰な起床を減らす構成になっている
- R2 engine・worker 水位制御変更後の `cargo test`（4件）と `cargo check` は通過している
- R2 engine・worker 水位制御変更後、CPU 使用率は若干低下し、音質にも問題がないことを実機で確認した
- R2 engine・worker 水位制御を現行構成として採用する方針を確定した
- Rubber Band backend へ独立した ±1 オクターブシフトを接続済みで、速度変更とは独立して動作する
- `+1 oct` は `pitch_scale = 2.0`、`-1 oct` は `pitch_scale = 0.5` として扱い、速度変更との組み合わせも可能な構成になっている
- オクターブシフト追加後も `cargo test`（4件）と `cargo check` は通過している
- オクターブシフト単独および速度変更との組み合わせはまだ実機確認していない

## MVP ゴール

- Transport イベントに応じて DSP の状態を適切に初期化できる
- `BypassTimeStretchProcessor` を実際の `TimeStretchProcessor` に置き換える
- time stretch の内部状態と処理入口を実装する
- `speed_ratio` を解釈した速度変更の実処理を time stretch 側へ組み込む
- 実用性を優先し、speed change では音高を維持する
- speed change とは独立して pitch shift を適用できる

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
- [x] 停止中に `speed_ratio` 用の加工済み再生バッファを作成し、再生時はソース時間軸を維持したまま加工済みバッファを参照する構成へ変更する
- [x] 復旧状態で `cargo check` が通過することを確認する
- [x] 停止中の毎フレーム DSP 再構築を止め、DSP 設定が変わったときだけ加工済みバッファを再構築するデバウンスを追加する
- [x] 実機で安定基準線へ復帰し、UI フリーズとメモリ増加が解消したことを確認する
- [x] 現在の復旧状態で、速度変更に連動して音高も変化する通常再生状態を確認する
- [x] OLA ベースのオフライン time stretch を実装して原因を確認し、CPU・メモリ増加を再現したうえで撤回する
- [x] OLA 実装の CPU 暴走が、長尺音源全体への同期処理と Hann 窓の大量再計算による処理量過大であることを特定する
- [x] OLA 実装のメモリ増加が、巨大な `output` / `weights` バッファを段階的に保持・書き込む構成によるものと切り分ける
- [x] 最新実装では設定変更1回の処理自体が過大であり、毎フレーム再生成が今回の主因ではないことを確認する
- [x] 音程維持を worker・固定長リングバッファによるストリーミング方式で実装する方針を確定する
- [x] 固定長リングバッファ残量、underrun 回数、生成フレーム数、DSP 確保メモリの計測値を追加する
- [ ] worker 処理時間と prepare 完了時間の計測値を追加する
- [x] パススルー worker と固定長 SPSC リングバッファを実装する
- [x] 基本 Transport 操作、通常再生、停止／再生／一時停止、シーク、ループ、5分超の長尺音源再生を実機確認する
- [x] Seek 時に generation を切り替えると同時に停止中でも旧世代リングを即時に破棄し、新位置の先読みを可能にする
- [x] Seek 後に新位置の先読みが可能な状態で、実機で無音なしに再生を再開できることを確認する
- [ ] ループ先頭を事前に先読みし、LoopJump 後の無音を実装上も保証する
- [ ] Seek 先を短時間先読みして `Preparing → Ready → Playing` で再生を再開し、クリックによる Seek 後の無音を実装上も保証する
- [x] worker を低水位／高水位・ブロック生成へ変更し、連続的な小刻み処理と過剰な起床を減らす
- [ ] 長尺音源で UI 応答性・メモリ上限・underrun を実機確認する
- [x] ストリーミング対応の `TimeStretchBackend` に Rubber Band のリアルタイムモードを接続し、公開ソース向けのLICENSE・依存関係・ビルド手順を整理する
- [x] `librubberband-dev` を開発環境へ導入する
- [x] Rubber Band 3.3 のリアルタイム backend を worker に接続し、`time_ratio = 1 / speed`、`pitch_scale = 1.0` で音程維持付き speed change の処理経路を実装する
- [x] GPL公開用の `LICENSE`、`README.md`、`docs/architecture.md`、`docs/dependencies.md` を追加する
- [x] `1.00x` の Rubber Band backendを実機確認する
- [x] `0.75x / 0.50x / 1.25x / 1.50x` の音程維持付き speed changeを実機確認する
- [ ] generation 管理、バッファ破棄、再先読みを実装し、Seek / LoopJump / Stop 後の古い音声混入がないことを確認する
- [x] 音程維持付き speed change を CPU・メモリ使用量の上限を確認しながら実装する
- [ ] オフライン time stretch の実機動作と UI 安定性を確認する
- [x] 独立した ±1 オクターブの pitch shift を Rubber Band backend に接続する
- [ ] 独立した pitch shift を速度変更と組み合わせて実機確認する
- [ ] Transport イベントによる reset が実処理でも成立することを確認する
- [ ] 古い全体加工バッファ方式と `playback_samples` 全体保持を削除する
- [x] Rubber Band のリアルタイム backend を高速化のため R3 から R2 engine へ変更する
- [x] worker を低水位（約200ms）まで待機し、高水位（約400ms）までまとめて生成する制御へ変更する
- [x] R2 engine・worker 水位制御変更後、CPU 使用率が若干低下し、音質に問題がないことを実機確認する
- [ ] R2 engine・worker 水位制御変更後の underrun を実機確認する
- [ ] ±1 オクターブの単独動作と `0.50x / 1.50x` との組み合わせを実機確認する

## 未解決論点

- 現在の `timestretch.rs` は実質パススルー状態であり、time stretch 実処理はまだ成立していない
- 現在は安定基準線へ復帰しており、フリーズとメモリ増加は解消している
- 現在の再生状態は速度変更に連動して音高も変化する通常の「テープ速度変更」であり、音程維持付き speed change は未実装
- `player.rs` の `speed_ratio` は再生進行へ反映され、加工済みバッファはその逆数の時間軸で参照される
- 停止中に加工済み再生バッファを生成する構成はフリーズ・メモリ増加の切り分けに有効だったが、今後の音程維持処理はこの安定基盤を崩さない別設計へ切り替える
- OLA ベースのオフライン time stretch は、長尺音源全体を同期処理し巨大な `output` / `weights` を保持するため、現状の構成では採用しない
- 音程維持付き speed change は、固定長リングバッファと worker を使うストリーミング方式で再設計する方針を確定した
- 実用性を優先し、speed change では音程を維持する方針を確定した
- パススルー worker、固定長 SPSC リングバッファ、generation 管理、underrun・残量・確保量の基本計測は実装済み
- 音声コールバックではメモリ確保・Mutex・DSP 計算を行わず、リングバッファから読み出すだけとする構成を実装済み
- worker の処理結果に `generation_id` を持たせ、シーク・停止・設定変更後の古い結果を破棄する仕組みを実装済み
- UI、シーク、ループは元音源フレームを基準に維持し、加工バッファの逆数時間軸参照は廃止する必要がある
- `TimeStretchBackend` は Rubber Band のリアルタイムモードを採用する方針となり、開発環境への `librubberband-dev` 導入と backend 接続まで完了した
- Rubber Band 3.3 のリアルタイム backend を worker に接続し、`time_ratio = 1 / speed`、`pitch_scale = 1.0` による音程維持付き speed change の処理経路が実装済み
- Rubber Band は GPLv2以降または商用ライセンスのデュアルライセンスであり、非商用・公開配布を前提に GPLv2以降での公開・配布条件へ適合させる方針
- 公開ソースとして分かりやすい構成にするため、`LICENSE`、`README.md`、`docs/architecture.md`、`docs/dependencies.md` を追加済み
- `cargo test`（4件）と `cargo check` は Rubber Band backend 接続後も通過している
- 長尺音源に対する処理時間・ピークメモリ・UI 応答性・underrun の実機試験は未実施
- ただし通常再生、停止／再生／一時停止、シーク、ループ、5分超の長尺音源再生自体は実機で正常動作を確認済み
- ループ先頭への遷移は実機ではシームレスに聞こえることを確認済みだが、ループ先頭の専用先読みは未実装であり、実装上のシームレス性は保証されていない
- 通常のクリックによる Seek は実機で無音なしに再生できることを確認済みだが、`Preparing → Ready → Playing` の明示的な準備状態は未実装
- worker はパススルー段階でも再生中に CPU をかなり専有しており、ブロック生成・水位制御が必要
- `0.50x / 0.75x / 1.25x / 1.50x` の音程維持は全速度で実機確認済み
- Rubber Band の R3 engine は CPU 使用量が高かったため R2 engine へ変更し、worker も低水位／高水位制御へ最適化済み
- R2 engine・worker 水位制御変更後は CPU 使用率が若干低下し、音質に問題がないことを実機確認済み
- R2 engine・worker 水位制御変更後の underrun は未確認
- Rubber Band のリアルタイム R2 engine での CPU 低減効果を維持しつつ、underrun が発生しないか実機で確認する必要がある
- 独立した ±1 オクターブの pitch shift は Rubber Band backend へ接続済みで、速度変更とは独立して適用できる
- ±1 オクターブの単独動作および速度変更との組み合わせはまだ実機確認していない
- proper な pitch shift の backend 接続自体は完了しているが、実機での音質・ノイズ・CPU の確認が未完了
- 実処理へ再移行した後の Transport イベントによる reset の挙動確認が未実施
- worker 処理時間と prepare 完了時間の計測は未実装
- スペクトラム画面の日本語文字化けは未解決で、eframe の日本語フォント設定が原因候補
- `stream ready / … fr / … underrun` は `PlayerSnapshot.debug_summary` に保持されるだけで UI 表示の原因ではなく、スペクトラム画面への重なりも確認されていない
- `cargo test`、`cargo check`、差分チェックは通過している
- worker は音源読み込み時点で起動し、停止中・一時停止中もリングバッファへの先読みを行う。`Play` は worker 起動ではなくリング読み出し開始である
- `LoopJump` では到達後に世代を切り替えて先頭から作り直すため、ループ先頭の事前先読みがなくても現状の実機では無音が知覚されていないが、専用の先読み制御は未実装
- worker は低水位／高水位のブロック単位生成へ変更済みで、CPU の過剰な連続処理を抑える構成になっている
- Seek でも `LoopJump` と同様に世代切替後のリング再充填が必要だが、旧世代リングの即時破棄により実機では無音なしで再開できることを確認済み
- Seek 時は停止中にも旧 generation のリング内容を即時に破棄するよう修正済みで、worker がクリック先の新 generation を先読みできる状態になった
- Seek 修正後の `cargo test`（3件）と `cargo check` は通過している
- Seek 後に無音なしで再生を開始できることは実機で確認済み
- ループ無音は実機上では解消しているが、現状の方式は専用のループ先頭事前先読みではなく、既存の generation 切替・リング再生成で無音が知覚されない状態である
- Rubber Band backend による音程維持付き速度変更の実装自体は完了しており、R2 engine と worker 水位制御変更後の音質は問題なく、CPU 使用率も若干低下したことを実機確認済み
- Rubber Band backend による独立した ±1 オクターブシフトの実装は完了しているが、単独動作・速度変更との組み合わせの実機確認が未完了

## 次の一手

1. ±1 オクターブを 1.00x で単独確認し、続けて `0.50x / 1.50x` と組み合わせて音程・ノイズ・CPU を実機確認する
2. R2 engine・低水位／高水位 worker 変更後の underrun と長尺音源での UI 応答性・メモリ上限を実機確認する
3. 実機結果を基に音質・CPU・UI 表記を調整し、必要に応じて worker の水位制御を調整する