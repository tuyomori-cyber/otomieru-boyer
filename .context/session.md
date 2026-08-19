# Current State

Updated: 1787116455

Now:
- Done: ファイルダイアログを導入
- Done: `Open` から `Symphonia` デコードを実行し、`Track` を `AppState` に反映する処理を実装
- Done: 読み込み済みファイル情報と状態メッセージを UI に反映
- Done: `cargo check` でビルド成立を確認

Next:
- `CPAL` を追加して `audio::player` の再生基盤を実装
- Play / Stop を実際の音声出力へ接続

Constraints:
- Keep session.md short and optimized for Codex resumption.
- Treat turns.jsonl as the append-only detailed log.

Relevant:
- context: otomieru-boyer
- repo: otomieru-boyer
- cwd: /home/tuyomori/project/otomieru-boyer
- file: .context/session.md
- file: .context/turns.jsonl
- file: root-todo.md

