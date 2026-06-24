use eframe::egui;
use std::sync::{Arc, atomic::AtomicBool};

mod animations;
mod bone_properties;
mod flexing;
mod log;
mod model_groups;
mod overview;
mod sequences;

pub enum UniqueTabs {
    Overview,
    Log,
    ModelGroups,
    Flexing,
    BoneProperties,
    Animations,
    Sequences,
}

pub struct TabViewer<'a> {
    compiling: Arc<AtomicBool>,
    input_data: &'a mut crate::input::SourceInput,
    loaded_files: &'a mut crate::import::FileManager,
}

impl<'a> TabViewer<'a> {
    pub fn create(compiling: Arc<AtomicBool>, input_data: &'a mut crate::input::SourceInput, loaded_files: &'a mut crate::import::FileManager) -> Self {
        Self {
            compiling,
            input_data,
            loaded_files,
        }
    }
}

impl<'a> egui_dock::TabViewer for TabViewer<'a> {
    type Tab = UniqueTabs;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            UniqueTabs::Overview => String::from("Overview").into(),
            UniqueTabs::Log => String::from("Log").into(),
            UniqueTabs::ModelGroups => String::from("Model Groups").into(),
            UniqueTabs::Flexing => String::from("Flexing").into(),
            UniqueTabs::BoneProperties => String::from("Bone Properties").into(),
            UniqueTabs::Animations => String::from("Animations").into(),
            UniqueTabs::Sequences => String::from("Sequences").into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            UniqueTabs::Overview => self.render_overview(ui),
            UniqueTabs::Log => self.render_log(ui),
            UniqueTabs::ModelGroups => self.render_model_groups(ui),
            UniqueTabs::Flexing => self.render_flexing(ui),
            UniqueTabs::BoneProperties => self.render_bone_properties(ui),
            UniqueTabs::Animations => self.render_animation(ui),
            UniqueTabs::Sequences => self.render_sequences(ui),
        }
    }

    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
        [false, false]
    }
}
