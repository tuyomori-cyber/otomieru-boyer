# Current State

Updated: 1787186278

Now:
- Done: 秒ベース表示開始位置への変更影響を調査
- Done: 変更中心は `AppState` と `spectrogram/timeline` UI 層で、再生エンジン影響は小さいと整理

Next:
- `page` 概念を `view_start_seconds` / `view_duration_seconds` へ置き換える設計案を具体化
- 全体バーを「ページ選択」から「表示開始位置指定」へ再定義

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

