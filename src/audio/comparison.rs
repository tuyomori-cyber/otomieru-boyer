use std::sync::atomic::{AtomicUsize, Ordering};

use crate::model::{ComparisonPhase, DEFAULT_COMPARISON_SEQUENCE, comparison_phase_at};

const DISABLED_STATE: usize = 0;

/// 原曲と音高メモの別々の音声コールバックから参照する比較ループ状態。
///
/// 0を無効、1以上をシーケンス位置+1として単一のAtomic値へ格納し、
/// 有効状態と位置が食い違う中間状態を作らない。
pub struct ComparisonAudioControl {
    encoded_state: AtomicUsize,
}

impl ComparisonAudioControl {
    pub fn enable_from_start(&self) {
        self.encoded_state.store(1, Ordering::Release);
    }

    pub fn disable(&self) {
        self.encoded_state.store(DISABLED_STATE, Ordering::Release);
    }

    pub fn reset_to_start_if_enabled(&self) {
        let _ = self
            .encoded_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |encoded| {
                (encoded != DISABLED_STATE).then_some(1)
            });
    }

    pub fn advance(&self) {
        let sequence_len = DEFAULT_COMPARISON_SEQUENCE.len();
        debug_assert!(sequence_len > 0);
        let _ = self
            .encoded_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |encoded| {
                if encoded == DISABLED_STATE {
                    return None;
                }
                let current_index = encoded - 1;
                Some((current_index + 1) % sequence_len + 1)
            });
    }

    pub fn snapshot(&self) -> ComparisonAudioSnapshot {
        let encoded = self.encoded_state.load(Ordering::Acquire);
        if encoded == DISABLED_STATE {
            ComparisonAudioSnapshot::default()
        } else {
            let sequence_index = encoded - 1;
            ComparisonAudioSnapshot {
                sequence_index: Some(sequence_index),
                phase: comparison_phase_at(DEFAULT_COMPARISON_SEQUENCE, sequence_index),
            }
        }
    }
}

impl Default for ComparisonAudioControl {
    fn default() -> Self {
        Self {
            encoded_state: AtomicUsize::new(DISABLED_STATE),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComparisonAudioSnapshot {
    pub sequence_index: Option<usize>,
    pub phase: Option<ComparisonPhase>,
}

impl ComparisonAudioSnapshot {
    pub fn source_is_audible(self, user_muted: bool) -> bool {
        match self.phase {
            Some(ComparisonPhase::Original | ComparisonPhase::Mix) => true,
            Some(ComparisonPhase::Notes) => false,
            None => !user_muted,
        }
    }

    pub fn memos_are_audible(self) -> bool {
        !matches!(self.phase, Some(ComparisonPhase::Original))
    }
}

#[cfg(test)]
mod tests {
    use super::ComparisonAudioControl;
    use crate::model::ComparisonPhase;

    #[test]
    fn control_advances_through_the_configured_sequence() {
        let control = ComparisonAudioControl::default();
        assert_eq!(control.snapshot().phase, None);

        control.enable_from_start();
        assert_eq!(control.snapshot().phase, Some(ComparisonPhase::Original));
        control.advance();
        assert_eq!(control.snapshot().phase, Some(ComparisonPhase::Notes));
        control.advance();
        assert_eq!(control.snapshot().phase, Some(ComparisonPhase::Mix));
        control.advance();
        assert_eq!(control.snapshot().phase, Some(ComparisonPhase::Original));
    }

    #[test]
    fn reset_and_disable_preserve_the_expected_lifecycle() {
        let control = ComparisonAudioControl::default();
        control.enable_from_start();
        control.advance();
        control.reset_to_start_if_enabled();
        assert_eq!(control.snapshot().phase, Some(ComparisonPhase::Original));

        control.disable();
        control.reset_to_start_if_enabled();
        assert_eq!(control.snapshot().phase, None);
    }

    #[test]
    fn routing_uses_comparison_phase_without_changing_user_mutes() {
        let control = ComparisonAudioControl::default();
        assert!(!control.snapshot().source_is_audible(true));
        assert!(control.snapshot().memos_are_audible());

        control.enable_from_start();
        let original = control.snapshot();
        assert!(original.source_is_audible(true));
        assert!(!original.memos_are_audible());

        control.advance();
        let notes = control.snapshot();
        assert!(!notes.source_is_audible(false));
        assert!(notes.memos_are_audible());

        control.advance();
        let mix = control.snapshot();
        assert!(mix.source_is_audible(true));
        assert!(mix.memos_are_audible());
    }
}
