# Current State

Updated: 1787124665

Now:
- Done: スペクトログラム上の長押しで基準音が鳴り続けるよう実装
- Done: 押したまま上下移動で音程追従するよう実装
- Done: 再生中でもプレビュー音が鳴るようにした
- Done: `cargo check` でビルド成立を確認

Next:
- 長押しプレビューの実機挙動確認
- 問題なければループ範囲選択とループ再生へ進む

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

