use std::sync::atomic::{AtomicUsize, Ordering};

use crate::model::{
    ComparisonPhase, DEFAULT_COMPARISON_SEQUENCE, MAX_COMPARISON_SEQUENCE_LEN,
    is_valid_comparison_sequence,
};

const DISABLED_STATE: usize = 0;
const STATE_MASK: usize = 0b1111;
const SEQUENCE_SHIFT: usize = 4;
const PHASE_BITS: usize = 2;

/// 原曲と音高メモの別々の音声コールバックから参照する比較ループ状態。
///
/// 0を無効、1以上をシーケンス位置+1として単一のAtomic値へ格納し、
/// 有効状態と位置が食い違う中間状態を作らない。
pub struct ComparisonAudioControl {
    encoded_state: AtomicUsize,
}

impl ComparisonAudioControl {
    pub fn enable_from_start(&self) {
        let _ = self
            .encoded_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |encoded| {
                Some((encoded & !STATE_MASK) | 1)
            });
    }

    pub fn disable(&self) {
        let _ = self
            .encoded_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |encoded| {
                Some(encoded & !STATE_MASK)
            });
    }

    pub fn reset_to_start_if_enabled(&self) {
        let _ = self
            .encoded_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |encoded| {
                (encoded & STATE_MASK != DISABLED_STATE).then_some((encoded & !STATE_MASK) | 1)
            });
    }

    /// パターン本体と現在位置を一つのAtomic値として更新する。
    ///
    /// 比較中なら変更直後から新パターンの先頭へ戻り、無効中なら新パターンだけを
    /// 保持する。音声コールバックは途中まで更新されたパターンを観測しない。
    pub fn set_sequence(&self, sequence: &[ComparisonPhase]) {
        assert!(is_valid_comparison_sequence(sequence));
        let sequence_bits = encode_sequence(sequence);
        self.encoded_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |encoded| {
                let enabled = encoded & STATE_MASK != DISABLED_STATE;
                Some(sequence_bits | usize::from(enabled))
            })
            .expect("comparison sequence update always succeeds");
    }

    pub fn advance(&self) {
        let _ = self
            .encoded_state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |encoded| {
                if encoded & STATE_MASK == DISABLED_STATE {
                    return None;
                }
                let sequence_len = sequence_len(encoded);
                debug_assert!(sequence_len > 0);
                let current_index = encoded - 1;
                let next_index = current_index & STATE_MASK;
                Some((encoded & !STATE_MASK) | ((next_index + 1) % sequence_len + 1))
            });
    }

    pub fn snapshot(&self) -> ComparisonAudioSnapshot {
        let encoded = self.encoded_state.load(Ordering::Acquire);
        if encoded & STATE_MASK == DISABLED_STATE {
            ComparisonAudioSnapshot::default()
        } else {
            let sequence_index = (encoded & STATE_MASK) - 1;
            ComparisonAudioSnapshot {
                sequence_index: Some(sequence_index),
                phase: phase_at_encoded_sequence(encoded, sequence_index),
            }
        }
    }
}

impl Default for ComparisonAudioControl {
    fn default() -> Self {
        Self {
            encoded_state: AtomicUsize::new(encode_sequence(DEFAULT_COMPARISON_SEQUENCE)),
        }
    }
}

fn encode_sequence(sequence: &[ComparisonPhase]) -> usize {
    debug_assert!(is_valid_comparison_sequence(sequence));
    sequence
        .iter()
        .enumerate()
        .fold(0, |encoded, (index, phase)| {
            encoded | (encode_phase(*phase) << (SEQUENCE_SHIFT + index * PHASE_BITS))
        })
}

fn encode_phase(phase: ComparisonPhase) -> usize {
    match phase {
        ComparisonPhase::Original => 1,
        ComparisonPhase::Notes => 2,
        ComparisonPhase::Mix => 3,
    }
}

fn decode_phase(encoded: usize, index: usize) -> Option<ComparisonPhase> {
    match (encoded >> (SEQUENCE_SHIFT + index * PHASE_BITS)) & 0b11 {
        1 => Some(ComparisonPhase::Original),
        2 => Some(ComparisonPhase::Notes),
        3 => Some(ComparisonPhase::Mix),
        _ => None,
    }
}

fn sequence_len(encoded: usize) -> usize {
    (0..MAX_COMPARISON_SEQUENCE_LEN)
        .take_while(|index| decode_phase(encoded, *index).is_some())
        .count()
}

fn phase_at_encoded_sequence(encoded: usize, index: usize) -> Option<ComparisonPhase> {
    decode_phase(encoded, index % sequence_len(encoded))
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
    fn changing_the_sequence_restarts_an_enabled_control() {
        use crate::model::ComparisonPhase::{Mix, Notes};

        let control = ComparisonAudioControl::default();
        control.enable_from_start();
        control.advance();
        control.set_sequence(&[Mix, Mix, Notes]);

        assert_eq!(control.snapshot().sequence_index, Some(0));
        assert_eq!(control.snapshot().phase, Some(Mix));
        control.advance();
        assert_eq!(control.snapshot().phase, Some(Mix));
        control.advance();
        assert_eq!(control.snapshot().phase, Some(Notes));
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
