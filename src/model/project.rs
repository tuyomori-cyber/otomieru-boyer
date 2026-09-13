use crate::model::EQ_BAND_COUNT;
use serde::{Deserialize, Serialize};

pub const DEFAULT_LAYER_VOLUME: f32 = 1.0;
pub const DEFAULT_LAYER_OPACITY: f32 = 1.0;
pub const DEFAULT_MEMO_DURATION_SECONDS: f64 = 0.5;
pub const FIXED_LAYER_COUNT: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayerId(u64);

impl LayerId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoId(u64);

impl MemoId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalePreset {
    Major,
    Minor,
    Pentatonic,
    Chromatic,
}

impl ScalePreset {
    pub const ALL: [Self; 4] = [Self::Major, Self::Minor, Self::Pentatonic, Self::Chromatic];

    pub fn label(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Pentatonic => "Pentatonic",
            Self::Chromatic => "Chromatic",
        }
    }

    fn intervals(self) -> &'static [usize] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Self::Pentatonic => &[0, 2, 4, 7, 9],
            Self::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundamentalAnalysisSettings {
    pub emphasis: f32,
    pub scale_root: usize,
    pub scale_preset: ScalePreset,
    pub emphasized_pitch_classes: [bool; 12],
    pub unemphasized_pitch_attenuation: f32,
}

impl FundamentalAnalysisSettings {
    pub fn apply_scale_preset(&mut self) {
        self.emphasized_pitch_classes = [false; 12];
        for interval in self.scale_preset.intervals() {
            self.emphasized_pitch_classes[(self.scale_root + interval) % 12] = true;
        }
    }
}

impl Default for FundamentalAnalysisSettings {
    fn default() -> Self {
        Self {
            emphasis: 0.0,
            scale_root: 0,
            scale_preset: ScalePreset::Major,
            emphasized_pitch_classes: [
                true, false, true, false, true, true, false, true, false, true, false, true,
            ],
            unemphasized_pitch_attenuation: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettings {
    pub preview_reference_a4_hz: f32,
    pub equalizer_gains_db: [f32; EQ_BAND_COUNT],
    pub fundamental_analysis: FundamentalAnalysisSettings,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            preview_reference_a4_hz: 440.0,
            equalizer_gains_db: [0.0; EQ_BAND_COUNT],
            fundamental_analysis: FundamentalAnalysisSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PitchMemo {
    pub id: MemoId,
    pub start_sec: f64,
    pub duration_sec: f64,
    pub pitch_midi: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PitchMemoLayer {
    pub id: LayerId,
    pub name: String,
    #[serde(skip_serializing, default = "default_layer_visible")]
    pub visible: bool,
    #[serde(skip_serializing, default)]
    pub muted: bool,
    #[serde(skip_serializing, default = "default_layer_volume")]
    pub volume: f32,
    #[serde(skip_serializing, default = "default_layer_opacity")]
    pub opacity: f32,
    pub memos: Vec<PitchMemo>,
}

fn default_layer_visible() -> bool {
    true
}

fn default_layer_volume() -> f32 {
    DEFAULT_LAYER_VOLUME
}

fn default_layer_opacity() -> f32 {
    DEFAULT_LAYER_OPACITY
}

impl PitchMemoLayer {
    fn new(id: LayerId, name: String) -> Self {
        Self {
            id,
            name,
            visible: true,
            muted: false,
            volume: DEFAULT_LAYER_VOLUME,
            opacity: DEFAULT_LAYER_OPACITY,
            memos: Vec::new(),
        }
    }
}

/// sidecar JSONへ永続化する、曲単位のプロジェクトデータ。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectData {
    pub project_settings: ProjectSettings,
    pub layers: Vec<PitchMemoLayer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedMemo {
    pub layer_id: LayerId,
    pub memo_id: MemoId,
}

/// 選択状態や採番位置など、sidecar JSONへは保存しない実行中の編集状態。
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectEditingState {
    pub selected_layer_id: Option<LayerId>,
    pub selected_memo: Option<SelectedMemo>,
    pub dirty: bool,
    next_layer_id: u64,
    next_memo_id: u64,
    undo_stack: Vec<ProjectEdit>,
    saved_data: ProjectData,
}

#[derive(Debug, Clone, PartialEq)]
enum ProjectEdit {
    Added(SelectedMemo),
    Deleted {
        selected: SelectedMemo,
        index: usize,
        memo: PitchMemo,
    },
    Updated {
        selected: SelectedMemo,
        before: PitchMemo,
    },
}

impl ProjectEditingState {
    fn for_data(data: &ProjectData) -> Self {
        let next_layer_id = next_id(data.layers.iter().map(|layer| layer.id.get()));
        let next_memo_id = next_id(
            data.layers
                .iter()
                .flat_map(|layer| layer.memos.iter().map(|memo| memo.id.get())),
        );

        Self {
            selected_layer_id: data.layers.first().map(|layer| layer.id),
            selected_memo: None,
            dirty: false,
            next_layer_id,
            next_memo_id,
            undo_stack: Vec::new(),
            saved_data: data.clone(),
        }
    }
}

/// 永続データと一時的な編集状態をまとめ、IDの一意な採番を担う。
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectState {
    pub data: ProjectData,
    pub editing: ProjectEditingState,
}

impl ProjectState {
    pub fn new() -> Self {
        let mut state = Self::from_data(ProjectData::default());
        state.editing.selected_layer_id = state.data.layers.first().map(|layer| layer.id);
        state.mark_saved();
        state
    }

    pub fn from_data(mut data: ProjectData) -> Self {
        ensure_fixed_layers_in_data(&mut data);
        reset_runtime_layer_settings(&mut data);
        let editing = ProjectEditingState::for_data(&data);
        Self { data, editing }
    }

    pub fn add_layer(&mut self, name: impl Into<String>) -> Option<LayerId> {
        if self.data.layers.len() >= FIXED_LAYER_COUNT {
            return None;
        }
        let id = LayerId::new(self.editing.next_layer_id)?;
        self.editing.next_layer_id = self.editing.next_layer_id.checked_add(1)?;
        self.data.layers.push(PitchMemoLayer::new(id, name.into()));
        self.editing.selected_layer_id = Some(id);
        self.editing.selected_memo = None;
        self.refresh_dirty();
        Some(id)
    }

    pub fn set_all_layers_visible(&mut self, visible: bool) -> bool {
        let mut changed = false;
        for layer in &mut self.data.layers {
            if layer.visible != visible {
                layer.visible = visible;
                changed = true;
            }
        }
        changed
    }

    pub fn set_all_layers_muted(&mut self, muted: bool) -> bool {
        let mut changed = false;
        for layer in &mut self.data.layers {
            if layer.muted != muted {
                layer.muted = muted;
                changed = true;
            }
        }
        changed
    }

    pub fn set_all_layers_volume(&mut self, volume: f32) -> bool {
        let volume = volume.clamp(0.0, 1.0);
        let mut changed = false;
        for layer in &mut self.data.layers {
            if layer.volume != volume {
                layer.volume = volume;
                changed = true;
            }
        }
        changed
    }

    pub fn set_all_layers_opacity(&mut self, opacity: f32) -> bool {
        let opacity = opacity.clamp(0.0, 1.0);
        let mut changed = false;
        for layer in &mut self.data.layers {
            if layer.opacity != opacity {
                layer.opacity = opacity;
                changed = true;
            }
        }
        changed
    }

    pub fn add_memo(
        &mut self,
        layer_id: LayerId,
        start_sec: f64,
        duration_sec: f64,
        pitch_midi: i32,
    ) -> Option<MemoId> {
        let layer = self
            .data
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)?;
        let id = MemoId::new(self.editing.next_memo_id)?;
        self.editing.next_memo_id = self.editing.next_memo_id.checked_add(1)?;
        layer.memos.push(PitchMemo {
            id,
            start_sec,
            duration_sec,
            pitch_midi,
        });
        self.editing.selected_memo = Some(SelectedMemo {
            layer_id,
            memo_id: id,
        });
        self.editing
            .undo_stack
            .push(ProjectEdit::Added(SelectedMemo {
                layer_id,
                memo_id: id,
            }));
        self.refresh_dirty();
        Some(id)
    }

    pub fn memo(&self, selected: SelectedMemo) -> Option<&PitchMemo> {
        self.data
            .layers
            .iter()
            .find(|layer| layer.id == selected.layer_id)?
            .memos
            .iter()
            .find(|memo| memo.id == selected.memo_id)
    }

    pub fn delete_memo(&mut self, selected: SelectedMemo) -> bool {
        let Some(layer) = self
            .data
            .layers
            .iter_mut()
            .find(|layer| layer.id == selected.layer_id)
        else {
            return false;
        };
        let Some(index) = layer
            .memos
            .iter()
            .position(|memo| memo.id == selected.memo_id)
        else {
            return false;
        };
        let memo = layer.memos.remove(index);
        self.editing.undo_stack.push(ProjectEdit::Deleted {
            selected,
            index,
            memo,
        });
        if self.editing.selected_memo == Some(selected) {
            self.editing.selected_memo = None;
        }
        self.refresh_dirty();
        true
    }

    pub fn update_memo(
        &mut self,
        selected: SelectedMemo,
        start_sec: f64,
        duration_sec: f64,
        pitch_midi: i32,
    ) -> bool {
        let Some(layer) = self
            .data
            .layers
            .iter_mut()
            .find(|layer| layer.id == selected.layer_id)
        else {
            return false;
        };
        let Some(memo) = layer
            .memos
            .iter_mut()
            .find(|memo| memo.id == selected.memo_id)
        else {
            return false;
        };
        if memo.start_sec == start_sec
            && memo.duration_sec == duration_sec
            && memo.pitch_midi == pitch_midi
        {
            return false;
        }
        let before = memo.clone();
        memo.start_sec = start_sec;
        memo.duration_sec = duration_sec;
        memo.pitch_midi = pitch_midi;
        self.editing
            .undo_stack
            .push(ProjectEdit::Updated { selected, before });
        self.editing.selected_layer_id = Some(selected.layer_id);
        self.editing.selected_memo = Some(selected);
        self.refresh_dirty();
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.editing.undo_stack.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.editing.undo_stack.pop() else {
            return false;
        };
        match edit {
            ProjectEdit::Added(selected) => {
                if let Some(layer) = self
                    .data
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == selected.layer_id)
                    && let Some(index) = layer
                        .memos
                        .iter()
                        .position(|memo| memo.id == selected.memo_id)
                {
                    layer.memos.remove(index);
                }
                self.editing.selected_layer_id = Some(selected.layer_id);
                self.editing.selected_memo = None;
            }
            ProjectEdit::Deleted {
                selected,
                index,
                memo,
            } => {
                if let Some(layer) = self
                    .data
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == selected.layer_id)
                {
                    layer.memos.insert(index.min(layer.memos.len()), memo);
                    self.editing.selected_layer_id = Some(selected.layer_id);
                    self.editing.selected_memo = Some(selected);
                }
            }
            ProjectEdit::Updated { selected, before } => {
                if let Some(layer) = self
                    .data
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == selected.layer_id)
                    && let Some(memo) = layer
                        .memos
                        .iter_mut()
                        .find(|memo| memo.id == selected.memo_id)
                {
                    *memo = before;
                    self.editing.selected_layer_id = Some(selected.layer_id);
                    self.editing.selected_memo = Some(selected);
                }
            }
        }
        self.refresh_dirty();
        true
    }

    pub fn mark_saved(&mut self) {
        self.editing.saved_data = self.data.clone();
        self.refresh_dirty();
    }

    fn refresh_dirty(&mut self) {
        self.editing.dirty = project_data_without_runtime_settings(&self.data)
            != project_data_without_runtime_settings(&self.editing.saved_data);
    }
}

impl Default for ProjectState {
    fn default() -> Self {
        Self::new()
    }
}

fn next_id(ids: impl Iterator<Item = u64>) -> u64 {
    match ids.max() {
        Some(id) => id.checked_add(1).unwrap_or(0),
        None => 1,
    }
}

fn ensure_fixed_layers_in_data(data: &mut ProjectData) {
    for layer_number in 1..=FIXED_LAYER_COUNT {
        if data.layers.len() >= FIXED_LAYER_COUNT {
            break;
        }
        let id_value = layer_number as u64;
        if data.layers.iter().any(|layer| layer.id.get() == id_value) {
            continue;
        }
        let id = LayerId::new(layer_number as u64)
            .expect("fixed layer indices must be positive and fit in u64");
        data.layers
            .push(PitchMemoLayer::new(id, format!("Layer {layer_number}")));
    }
    data.layers.sort_by_key(|layer| layer.id);
}

fn reset_runtime_layer_settings(data: &mut ProjectData) {
    for layer in &mut data.layers {
        layer.visible = true;
        layer.muted = false;
        layer.volume = DEFAULT_LAYER_VOLUME;
        layer.opacity = DEFAULT_LAYER_OPACITY;
    }
}

fn project_data_without_runtime_settings(data: &ProjectData) -> ProjectData {
    let mut persistent_data = data.clone();
    reset_runtime_layer_settings(&mut persistent_data);
    persistent_data
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_LAYER_OPACITY, DEFAULT_LAYER_VOLUME, FIXED_LAYER_COUNT, LayerId, MemoId, PitchMemo,
        PitchMemoLayer, ProjectData, ProjectState, ScalePreset, SelectedMemo,
    };

    #[test]
    fn a_new_project_has_six_clean_named_layers() {
        let project = ProjectState::new();

        assert_eq!(project.data.layers.len(), FIXED_LAYER_COUNT);
        for (index, layer) in project.data.layers.iter().enumerate() {
            assert_eq!(layer.id.get(), index as u64 + 1);
            assert_eq!(layer.name, format!("Layer {}", index + 1));
        }
        assert_eq!(project.data.layers[0].volume, DEFAULT_LAYER_VOLUME);
        assert_eq!(project.data.layers[0].opacity, DEFAULT_LAYER_OPACITY);
        assert_eq!(
            project.editing.selected_layer_id,
            Some(project.data.layers[0].id)
        );
        assert!(!project.editing.dirty);
    }

    #[test]
    fn loaded_projects_are_completed_to_six_layers_without_reusing_memo_ids() {
        let layer_id = LayerId::new(4).unwrap();
        let memo_id = MemoId::new(9).unwrap();
        let data = ProjectData {
            layers: vec![PitchMemoLayer {
                id: layer_id,
                name: "Bass".to_owned(),
                visible: true,
                muted: false,
                volume: 0.7,
                opacity: 0.6,
                memos: vec![PitchMemo {
                    id: memo_id,
                    start_sec: 1.0,
                    duration_sec: 0.5,
                    pitch_midi: 40,
                }],
            }],
            ..ProjectData::default()
        };
        let mut project = ProjectState::from_data(data);

        let new_memo_id = project.add_memo(layer_id, 2.0, 0.5, 52).unwrap();

        assert_eq!(project.data.layers.len(), FIXED_LAYER_COUNT);
        assert_eq!(project.data.layers[0].id, LayerId::new(1).unwrap());
        assert_eq!(project.data.layers[3].id, layer_id);
        assert_eq!(new_memo_id.get(), 10);
        assert!(project.editing.dirty);
    }

    #[test]
    fn a_fixed_project_cannot_add_a_seventh_layer() {
        let mut project = ProjectState::new();

        assert_eq!(project.add_layer("Layer 7"), None);
    }

    #[test]
    fn bulk_layer_settings_apply_the_same_value_to_every_layer() {
        let mut project = ProjectState::new();
        project.data.layers[1].visible = false;
        project.data.layers[2].muted = true;
        project.data.layers[3].volume = 0.2;

        assert!(project.set_all_layers_visible(true));
        assert!(project.set_all_layers_muted(false));
        assert!(project.set_all_layers_volume(0.35));
        assert!(project.set_all_layers_opacity(0.45));
        assert!(project.data.layers.iter().all(|layer| layer.visible));
        assert!(project.data.layers.iter().all(|layer| !layer.muted));
        assert!(project.data.layers.iter().all(|layer| layer.volume == 0.35));
        assert!(
            project
                .data
                .layers
                .iter()
                .all(|layer| layer.opacity == 0.45)
        );
        assert!(!project.editing.dirty);
    }

    #[test]
    fn loading_a_project_resets_runtime_layer_settings() {
        let mut data = ProjectState::new().data;
        let layer = &mut data.layers[0];
        layer.visible = false;
        layer.muted = true;
        layer.volume = 0.2;
        layer.opacity = 0.3;

        let project = ProjectState::from_data(data);
        let layer = &project.data.layers[0];

        assert!(layer.visible);
        assert!(!layer.muted);
        assert_eq!(layer.volume, DEFAULT_LAYER_VOLUME);
        assert_eq!(layer.opacity, DEFAULT_LAYER_OPACITY);
    }

    #[test]
    fn memos_can_be_placed_in_each_fixed_layer() {
        let mut project = ProjectState::new();
        let layer_ids: Vec<_> = project.data.layers.iter().map(|layer| layer.id).collect();

        for (index, layer_id) in layer_ids.iter().copied().enumerate() {
            project.editing.selected_layer_id = Some(layer_id);
            let memo_id = project
                .add_memo(layer_id, index as f64, 0.5, 60 + index as i32)
                .unwrap();
            assert_eq!(project.editing.selected_layer_id, Some(layer_id));
            assert_eq!(
                project
                    .memo(SelectedMemo { layer_id, memo_id })
                    .unwrap()
                    .pitch_midi,
                60 + index as i32
            );
        }

        assert!(
            project
                .data
                .layers
                .iter()
                .all(|layer| layer.memos.len() == 1)
        );
    }

    #[test]
    fn adding_a_memo_to_an_unknown_layer_does_not_consume_an_id() {
        let mut project = ProjectState::new();
        let missing_layer = LayerId::new(99).unwrap();

        assert_eq!(project.add_memo(missing_layer, 0.0, 0.5, 60), None);
        let layer_id = project.editing.selected_layer_id.unwrap();
        assert_eq!(project.add_memo(layer_id, 0.0, 0.5, 60).unwrap().get(), 1);
    }

    #[test]
    fn scale_preset_updates_the_emphasized_pitch_classes() {
        let mut settings = ProjectData::default().project_settings.fundamental_analysis;
        settings.scale_root = 2;
        settings.scale_preset = ScalePreset::Major;

        settings.apply_scale_preset();

        assert_eq!(
            settings.emphasized_pitch_classes,
            [
                false, true, true, false, true, false, true, true, false, true, false, true
            ]
        );
    }

    #[test]
    fn memo_create_delete_and_update_are_undoable() {
        let mut project = ProjectState::new();
        let layer_id = project.editing.selected_layer_id.unwrap();
        let memo_id = project.add_memo(layer_id, 1.0, 0.5, 60).unwrap();
        let selected = SelectedMemo { layer_id, memo_id };

        assert!(project.update_memo(selected, 1.25, 0.75, 61));
        assert!(project.undo());
        assert_eq!(project.memo(selected).unwrap().start_sec, 1.0);
        assert_eq!(project.memo(selected).unwrap().duration_sec, 0.5);
        assert_eq!(project.memo(selected).unwrap().pitch_midi, 60);

        assert!(project.delete_memo(selected));
        assert!(project.memo(selected).is_none());
        assert!(project.undo());
        assert!(project.memo(selected).is_some());

        assert!(project.undo());
        assert!(project.memo(selected).is_none());
        assert!(!project.can_undo());
        assert!(!project.editing.dirty);
    }

    #[test]
    fn undo_after_saving_marks_the_project_dirty_again() {
        let mut project = ProjectState::new();
        let layer_id = project.editing.selected_layer_id.unwrap();
        project.add_memo(layer_id, 1.0, 0.5, 60).unwrap();
        project.mark_saved();

        assert!(project.undo());

        assert!(project.editing.dirty);
    }
}
