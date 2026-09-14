#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonPhase {
    Original,
    Notes,
    Mix,
}

impl ComparisonPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Notes => "Notes",
            Self::Mix => "Mix",
        }
    }
}

/// v0.4ではUIから順序や周回数を変更しない。
///
/// 遷移を`ComparisonPhase`自身へ持たせず、ステップ列として定義することで、
/// 将来は同じフェーズを複数回並べるだけで複数周へ拡張できるようにする。
pub const DEFAULT_COMPARISON_SEQUENCE: &[ComparisonPhase] = &[
    ComparisonPhase::Original,
    ComparisonPhase::Notes,
    ComparisonPhase::Mix,
];

pub fn comparison_phase_at(
    sequence: &[ComparisonPhase],
    sequence_index: usize,
) -> Option<ComparisonPhase> {
    (!sequence.is_empty()).then(|| sequence[sequence_index % sequence.len()])
}

#[cfg(test)]
mod tests {
    use super::{ComparisonPhase, DEFAULT_COMPARISON_SEQUENCE, comparison_phase_at};

    #[test]
    fn default_sequence_repeats_original_notes_and_mix() {
        use ComparisonPhase::{Mix, Notes, Original};

        let phases = (0..7)
            .map(|index| comparison_phase_at(DEFAULT_COMPARISON_SEQUENCE, index).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            phases,
            [Original, Notes, Mix, Original, Notes, Mix, Original]
        );
    }

    #[test]
    fn repeated_steps_do_not_require_fixed_phase_transitions() {
        use ComparisonPhase::{Mix, Notes, Original};
        let sequence = [Original, Original, Notes, Notes, Mix, Mix];

        let phases = (0..sequence.len())
            .map(|index| comparison_phase_at(&sequence, index).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(phases, sequence);
    }

    #[test]
    fn empty_sequence_has_no_phase() {
        assert_eq!(comparison_phase_at(&[], 0), None);
    }
}
