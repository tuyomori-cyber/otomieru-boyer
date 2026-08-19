# Current State

Updated: 1787116548

Now:
- Done: `CPAL` を使う通常再生エンジンの最小実装を追加
- Done: `Play` / `Stop` を `audio::player` 経由へ接続
- Pending: Linux 音声バックエンドのシステム依存不足でビルド未完了

Next:
- `pkg-config` と `libasound2-dev` の導入後に `cargo check` を再実行
- 再生位置同期と Pause の実動作確認

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

