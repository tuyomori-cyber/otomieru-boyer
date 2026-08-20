# Current State

Updated: 1787186594

Now:
- Done: ページ基準の表示を秒ベースのビューポート基準へ変更
- Done: 停止中の下部バー操作を「再生シーク」から「表示範囲移動」へ変更
- Done: 再生中は右端到達時のみ1画面ぶん表示更新する追従を実装
- Done: `cargo check` でビルド成立を確認

Next:
- 実機で表示範囲移動と再生位置非連動の挙動を確認
- 必要なら下部バーのドラッグ操作や追従条件を微調整

Constraints:
- Keep session.md short and optimized for Codex resumption.
- Treat turns.jsonl as the append-only detailed log.

Relevant:
- context: otomieru-boyer
- repo: otomieru-boyer
- cwd: /home/tuyomori/project/otomieru-boyer
- file: .context/session.md
- file: .context/turns.jsonl
- file: mod.rs

