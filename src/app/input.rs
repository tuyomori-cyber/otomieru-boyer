use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelModifier {
    None,
    Shift,
    Ctrl,
    Alt,
}

impl WheelModifier {
    #[cfg(target_os = "linux")]
    pub const ALL: [Self; 3] = [Self::None, Self::Shift, Self::Ctrl];

    #[cfg(not(target_os = "linux"))]
    pub const ALL: [Self; 4] = [Self::None, Self::Shift, Self::Ctrl, Self::Alt];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "なし",
            Self::Shift => "Shift",
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
        }
    }

    pub fn matches(self, modifiers: egui::Modifiers) -> bool {
        match self {
            Self::None => !modifiers.shift && !modifiers.ctrl && !modifiers.alt,
            Self::Shift => modifiers.shift && !modifiers.ctrl && !modifiers.alt,
            Self::Ctrl => !modifiers.shift && modifiers.ctrl && !modifiers.alt,
            Self::Alt => !modifiers.shift && !modifiers.ctrl && modifiers.alt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableMemoModifier {
    Shift,
    Ctrl,
    Alt,
}

impl VariableMemoModifier {
    #[cfg(target_os = "linux")]
    pub const ALL: [Self; 2] = [Self::Shift, Self::Ctrl];

    #[cfg(not(target_os = "linux"))]
    pub const ALL: [Self; 3] = [Self::Shift, Self::Ctrl, Self::Alt];

    pub fn label(self) -> &'static str {
        match self {
            Self::Shift => "Shift",
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
        }
    }

    pub fn matches(self, modifiers: egui::Modifiers) -> bool {
        match self {
            Self::Shift => WheelModifier::Shift.matches(modifiers),
            Self::Ctrl => WheelModifier::Ctrl.matches(modifiers),
            Self::Alt => WheelModifier::Alt.matches(modifiers),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelAction {
    TimeZoom,
    PitchZoom,
    TimePan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MousePreset {
    Default,
    TimeNavigation,
}

impl MousePreset {
    pub const ALL: [Self; 2] = [Self::Default, Self::TimeNavigation];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "既定",
            Self::TimeNavigation => "時間移動重視",
        }
    }
}

/// 曲ごとではなく、利用者ごとのマウス入力設定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MouseInputSettings {
    pub time_zoom_modifier: WheelModifier,
    pub pitch_zoom_modifier: WheelModifier,
    pub time_pan_modifier: WheelModifier,
    pub variable_memo_modifier: Option<VariableMemoModifier>,
}

impl Default for MouseInputSettings {
    fn default() -> Self {
        Self::for_preset(MousePreset::Default)
    }
}

impl MouseInputSettings {
    pub fn for_preset(preset: MousePreset) -> Self {
        match preset {
            MousePreset::Default => Self {
                time_zoom_modifier: WheelModifier::None,
                pitch_zoom_modifier: WheelModifier::Ctrl,
                time_pan_modifier: WheelModifier::Shift,
                variable_memo_modifier: Some(VariableMemoModifier::Shift),
            },
            MousePreset::TimeNavigation => Self {
                time_zoom_modifier: WheelModifier::Ctrl,
                pitch_zoom_modifier: WheelModifier::Shift,
                time_pan_modifier: WheelModifier::None,
                variable_memo_modifier: Some(VariableMemoModifier::Shift),
            },
        }
    }

    pub fn preset(&self) -> Option<MousePreset> {
        MousePreset::ALL
            .into_iter()
            .find(|preset| self == &Self::for_preset(*preset))
    }

    pub fn wheel_action(&self, modifiers: egui::Modifiers) -> Option<WheelAction> {
        // LinuxではAlt + ホイールがデスクトップ環境の拡大縮小へ予約されるため、
        // 保存済みの旧設定が残っていてもアプリ内操作には使わない。
        if cfg!(target_os = "linux") && modifiers.alt {
            return None;
        }
        if self.time_zoom_modifier.matches(modifiers) {
            Some(WheelAction::TimeZoom)
        } else if self.pitch_zoom_modifier.matches(modifiers) {
            Some(WheelAction::PitchZoom)
        } else if self.time_pan_modifier.matches(modifiers) {
            Some(WheelAction::TimePan)
        } else {
            None
        }
    }

    pub fn variable_memo_modifier_matches(&self, modifiers: egui::Modifiers) -> bool {
        self.variable_memo_modifier
            .is_some_and(|modifier| modifier.matches(modifiers))
    }

    pub fn set_wheel_modifier(&mut self, action: WheelAction, modifier: WheelModifier) -> bool {
        let current = self.wheel_modifier(action);
        if current == modifier {
            return false;
        }
        for other_action in [
            WheelAction::TimeZoom,
            WheelAction::PitchZoom,
            WheelAction::TimePan,
        ] {
            if other_action != action && self.wheel_modifier(other_action) == modifier {
                self.set_wheel_modifier_value(other_action, current);
                break;
            }
        }
        self.set_wheel_modifier_value(action, modifier);
        true
    }

    /// LinuxでOSに奪われるAlt割当を、現在の安全な既定値へ戻す。
    pub fn normalize_for_platform(&mut self) -> bool {
        if cfg!(target_os = "linux")
            && (self.time_zoom_modifier == WheelModifier::Alt
                || self.pitch_zoom_modifier == WheelModifier::Alt
                || self.time_pan_modifier == WheelModifier::Alt
                || self.variable_memo_modifier == Some(VariableMemoModifier::Alt))
        {
            *self = Self::default();
            return true;
        }
        false
    }

    fn wheel_modifier(&self, action: WheelAction) -> WheelModifier {
        match action {
            WheelAction::TimeZoom => self.time_zoom_modifier,
            WheelAction::PitchZoom => self.pitch_zoom_modifier,
            WheelAction::TimePan => self.time_pan_modifier,
        }
    }

    fn set_wheel_modifier_value(&mut self, action: WheelAction, modifier: WheelModifier) {
        match action {
            WheelAction::TimeZoom => self.time_zoom_modifier = modifier,
            WheelAction::PitchZoom => self.pitch_zoom_modifier = modifier,
            WheelAction::TimePan => self.time_pan_modifier = modifier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MouseInputSettings, MousePreset, VariableMemoModifier, WheelAction, WheelModifier,
    };
    use eframe::egui;

    #[test]
    fn default_preset_preserves_existing_wheel_and_memo_bindings() {
        let settings = MouseInputSettings::default();
        assert_eq!(
            settings.wheel_action(egui::Modifiers::NONE),
            Some(WheelAction::TimeZoom)
        );
        assert_eq!(
            settings.wheel_action(egui::Modifiers::CTRL),
            Some(WheelAction::PitchZoom)
        );
        assert_eq!(
            settings.wheel_action(egui::Modifiers::SHIFT),
            Some(WheelAction::TimePan)
        );
        assert_eq!(
            settings.variable_memo_modifier,
            Some(VariableMemoModifier::Shift)
        );
    }

    #[test]
    fn selecting_an_assigned_wheel_modifier_swaps_the_two_actions() {
        let mut settings = MouseInputSettings::default();
        assert!(settings.set_wheel_modifier(WheelAction::TimePan, WheelModifier::Ctrl));
        assert_eq!(settings.time_pan_modifier, WheelModifier::Ctrl);
        assert_eq!(settings.pitch_zoom_modifier, WheelModifier::Shift);
    }

    #[test]
    fn changing_a_binding_makes_the_settings_custom() {
        let mut settings = MouseInputSettings::for_preset(MousePreset::Default);
        assert!(settings.set_wheel_modifier(WheelAction::TimePan, WheelModifier::Ctrl));
        assert_eq!(settings.preset(), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn alt_scroll_is_never_an_application_action_on_linux() {
        let settings = MouseInputSettings {
            time_zoom_modifier: WheelModifier::Alt,
            ..MouseInputSettings::default()
        };

        assert_eq!(settings.wheel_action(egui::Modifiers::ALT), None);
    }
}
