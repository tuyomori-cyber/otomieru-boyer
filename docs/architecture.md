# 音声再生アーキテクチャ

再生経路は、UIスレッド、time-stretch worker、音声コールバックを分離する。

```text
UI / Transport
  └─ generation と再生位置を更新
       └─ worker: Rubber Band による time stretch
            └─ 固定長 SPSC リングバッファ
                 └─ CPAL audio callback: 読み出して出力
```

- UIスレッドはDSP計算を行わない。
- CPAL callback はメモリ確保、Mutex取得、DSP計算を行わない。
- Seek、Stop、LoopJump は generation を更新する。callback は旧 generation のフレームを破棄する。
- workerは固定量のワーク領域だけを持ち、音源長に比例する加工済みバッファを生成しない。

速度比 `speed` に対し、Rubber Band の time ratio には `1 / speed` を設定する。音程は `2^(semitones / 12)` を pitch scale として独立して設定するため、速度を変えても音程を保ちつつオクターブシフトできる。実用再生ではR2（高速）engineを使い、workerは低水位から高水位までまとめて生成する。
