use serde::{Deserialize, Serialize};

/// 常用ツールパレットに表示できる項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPaletteItem {
    Equalizer,
    Fundamental,
    Timbre,
    Tuning,
    Layers,
    #[serde(alias = "comparison_pattern")]
    LoopSequencer,
}

impl ToolPaletteItem {
    pub const ALL: [Self; 6] = [
        Self::Equalizer,
        Self::Fundamental,
        Self::Timbre,
        Self::Tuning,
        Self::Layers,
        Self::LoopSequencer,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Equalizer => "EQ",
            Self::Fundamental => "基音強調",
            Self::Timbre => "音色",
            Self::Tuning => "チューニング",
            Self::Layers => "レイヤー",
            Self::LoopSequencer => "比較パターン",
        }
    }
}

pub fn default_tool_palette_items() -> Vec<ToolPaletteItem> {
    ToolPaletteItem::ALL.to_vec()
}

/// マスターパレットの順番へ、表示対象だけを並べ直す。
pub fn ordered_visible_tools(
    order: &[ToolPaletteItem],
    visible_tools: &[ToolPaletteItem],
) -> Vec<ToolPaletteItem> {
    order
        .iter()
        .copied()
        .filter(|tool| visible_tools.contains(tool))
        .collect()
}

/// 不足・重複を補正し、全ツールを一度ずつ含むマスターパレット順を返す。
pub fn normalized_tool_palette_order(
    order: &[ToolPaletteItem],
    visible_tools: &[ToolPaletteItem],
) -> Vec<ToolPaletteItem> {
    let mut normalized = Vec::with_capacity(ToolPaletteItem::ALL.len());
    let source = if order.is_empty() {
        visible_tools
    } else {
        order
    };
    for tool in source.iter().chain(ToolPaletteItem::ALL.iter()) {
        if !normalized.contains(tool) {
            normalized.push(*tool);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{ToolPaletteItem, normalized_tool_palette_order, ordered_visible_tools};

    #[test]
    fn visible_tools_follow_the_master_palette_order() {
        use ToolPaletteItem::{Equalizer, LoopSequencer, Timbre};

        let order = [LoopSequencer, Equalizer, Timbre];
        let visible = [Equalizer, LoopSequencer];

        assert_eq!(
            ordered_visible_tools(&order, &visible),
            [LoopSequencer, Equalizer]
        );
    }

    #[test]
    fn an_old_visible_order_becomes_the_start_of_the_master_palette_order() {
        use ToolPaletteItem::{Equalizer, Timbre};

        let order = normalized_tool_palette_order(&[], &[Timbre, Equalizer]);

        assert_eq!(&order[..2], &[Timbre, Equalizer]);
        assert_eq!(order.len(), ToolPaletteItem::ALL.len());
    }
}
