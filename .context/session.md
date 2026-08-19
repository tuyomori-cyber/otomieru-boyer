# Current State

Updated: 1787116249

Now:
- Done: `Symphonia` を依存追加
- Done: 音声ファイルを PCM `f32` にデコードする `audio::decoder::decode_file(...)` を実装
- Done: `DecodedAudio` から `Track` を作る変換を追加
- Done: `cargo check` でビルド成立を確認

Next:
- ファイルダイアログ導入
- `Open` で `Track` 読み込みと状態更新を実装

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

