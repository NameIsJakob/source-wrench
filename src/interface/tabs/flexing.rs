use crate::{
    import::FileStatus,
    input::{self, Model},
    interface::{
        fix_naming_conflicts,
        icons::{IconType, icon},
        lists::ListPanel,
    },
};

use super::TabViewer;
use eframe::egui;

impl<'a> TabViewer<'a> {
    pub fn render_flexing(&mut self, ui: &mut egui::Ui) {
        let mut selected_flex_key = None;
        let mut selected_flex_controller = None;

        egui::SidePanel::right("Flexing Right Panel")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                egui::TopBottomPanel::top("Flexing Right Top Panel")
                    .height_range(egui::Rangef::new(ui.available_height() * 0.2, ui.available_height() * 0.5))
                    .default_height(ui.available_height() * 0.5)
                    .resizable(true)
                    .show_inside(ui, |ui| {
                        ui.heading("Flex Controllers");
                        ui.spacing();
                        selected_flex_controller =
                            ListPanel::new("Flex Controllers").show("Flex Controller", &mut self.input_data.flex_controllers, ui, || {
                                let new_flex_controller = input::FlexController {
                                    identifier: self.input_data.flex_controller_identifier_generator,
                                    ..Default::default()
                                };
                                self.input_data.flex_controller_identifier_generator += 1;
                                new_flex_controller
                            });
                    });

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.heading("Flex Keys");
                    ui.spacing();
                    selected_flex_key = ListPanel::new("Flex Keys").show("Flex Controller", &mut self.input_data.flex_keys, ui, || {
                        let new_flex_key = input::FlexKey {
                            identifier: self.input_data.flex_key_identifier_generator,
                            ..Default::default()
                        };
                        self.input_data.flex_key_identifier_generator += 1;
                        new_flex_key
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Flexing");
            ui.separator();

            let selection_state_id = ui.make_persistent_id("Flexing Select State");
            let mut selection_state = FlexSelectState::load(ui.ctx(), selection_state_id).unwrap_or_default();
            egui::SidePanel::left("Flexing Left Panel")
                .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
                .show_inside(ui, |ui| {
                    if self.input_data.model_groups.is_empty() {
                        ui.label("No Model Groups!");
                        return;
                    }

                    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                        for (model_group_index, model_group) in self.input_data.model_groups.iter().enumerate() {
                            egui::CollapsingHeader::new(model_group.name.clone()).show(ui, |ui| {
                                if model_group.models.is_empty() {
                                    ui.label("No Models!");
                                    return;
                                }

                                for (model_index, model) in model_group.models.iter().enumerate() {
                                    egui::CollapsingHeader::new(model.name.clone()).show(ui, |ui| {
                                        if let Some(source_file_path) = &model.source_file_path
                                            && let Some(file_status) = self.loaded_files.get_file_status(source_file_path)
                                        {
                                            render_file_status(ui, file_status, &mut selection_state, model, model_index, model_group_index);
                                        } else {
                                            ui.label("No File Source!");
                                        }
                                    });
                                }
                            });
                        }
                    });
                });

            egui::Frame::new().inner_margin(5.0).show(ui, |ui| {
                ui.heading("Flex Controller");
                ui.separator();
                if let Some(active_flex_controller_index) = selected_flex_controller {
                    let active_flex_controller = &mut self.input_data.flex_controllers[active_flex_controller_index];
                    let name_label = ui.label("Flex Controller Name: ");
                    if ui
                        .text_edit_singleline(&mut active_flex_controller.name)
                        .labelled_by(name_label.id)
                        .lost_focus()
                    {
                        fix_naming_conflicts(&mut self.input_data.flex_controllers, active_flex_controller_index);
                    }

                    self.render_flex_controller_options(ui, active_flex_controller_index);
                } else {
                    ui.label("No Controllers!");
                }

                ui.heading("Flex Key");
                ui.separator();
                if let Some(active_flex_key_index) = selected_flex_key {
                    let name_label = ui.label("Flex Key Name: ");
                    if ui
                        .text_edit_singleline(&mut self.input_data.flex_keys[active_flex_key_index].name)
                        .labelled_by(name_label.id)
                        .lost_focus()
                    {
                        fix_naming_conflicts(&mut self.input_data.flex_keys, active_flex_key_index);
                    }

                    self.render_flex_key_options(ui, active_flex_key_index);
                } else {
                    ui.label("No Flex Keys!");
                }

                ui.heading("Flex");
                ui.separator();

                if self.input_data.flex_keys.is_empty() {
                    ui.colored_label(egui::Color32::RED, "No Flex Keys");
                    return;
                }

                if let Some(active_model_group) = self.input_data.model_groups.get_mut(selection_state.active_model_group_index)
                    && let Some(active_model) = active_model_group.models.get_mut(selection_state.active_model_index)
                    && let Some(source_file_path) = &active_model.source_file_path
                    && let Some(FileStatus::Loaded(file_data)) = self.loaded_files.get_file_status(source_file_path)
                    && let Some((active_part_name, active_part)) = file_data.parts.get_index(selection_state.active_part_index)
                    && let Some((active_flex_name, _)) = active_part.flexes.get_index(selection_state.active_flex_index)
                {
                    ui.label(format!("Selected Flex: {active_flex_name}"));
                    let model_flexes = active_model.flexes.entry(active_part_name.clone()).or_default();
                    let model_flex = model_flexes.entry(active_flex_name.clone()).or_default();

                    let selection_text = if let Some(assigned_key) = model_flex.assigned_flex_key {
                        let assigned_flex_key = self.input_data.flex_keys.iter().position(|key| assigned_key == key.identifier);
                        egui::RichText::new(&self.input_data.flex_keys[assigned_flex_key.unwrap_or_default()].name)
                    } else {
                        egui::RichText::new("Not Assigned").color(egui::Color32::RED)
                    };

                    egui::ComboBox::from_label("Assigned Flex Key").selected_text(selection_text).show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut model_flex.assigned_flex_key,
                            None,
                            egui::RichText::new("Not Assigned").color(egui::Color32::RED),
                        );
                        for flex_key in &self.input_data.flex_keys {
                            ui.selectable_value(&mut model_flex.assigned_flex_key, Some(flex_key.identifier), &flex_key.name);
                        }
                    });
                } else {
                    ui.label("No Selected Flex!");
                }
            });

            selection_state.store(ui.ctx(), selection_state_id);
        });
    }

    fn render_flex_controller_options(&mut self, _ui: &mut egui::Ui, _active_flex_controller_index: usize) {}

    fn render_flex_key_options(&mut self, ui: &mut egui::Ui, active_flex_key_index: usize) {
        let active_flex_key = &mut self.input_data.flex_keys[active_flex_key_index];
        if self.input_data.flex_controllers.is_empty() {
            ui.colored_label(egui::Color32::RED, "No Controllers Created");
        } else {
            let mapped_controller = self
                .input_data
                .flex_controllers
                .iter()
                .position(|controller| active_flex_key.assigned_controller == controller.identifier);

            let selection_text = if let Some(assigned_controller) = mapped_controller {
                egui::RichText::new(&self.input_data.flex_controllers[assigned_controller].name)
            } else {
                egui::RichText::new("Not Assigned").color(egui::Color32::RED)
            };

            egui::ComboBox::from_label("Flex Key Assigned Controller")
                .selected_text(selection_text)
                .show_ui(ui, |ui| {
                    for flex_controller in &self.input_data.flex_controllers {
                        ui.selectable_value(&mut active_flex_key.assigned_controller, flex_controller.identifier, &flex_controller.name);
                    }
                });
        }
    }
}

#[derive(Clone, Default)]
struct FlexSelectState {
    active_model_group_index: usize,
    active_model_index: usize,
    active_part_index: usize,
    active_flex_index: usize,
}

impl FlexSelectState {
    fn load(ctx: &egui::Context, id: egui::Id) -> Option<Self> {
        ctx.data_mut(|data| data.get_persisted(id))
    }

    fn store(self, ctx: &egui::Context, id: egui::Id) {
        ctx.data_mut(|data| data.insert_persisted(id, self));
    }
}

fn render_file_status(
    ui: &mut egui::Ui,
    file_status: FileStatus,
    selection_state: &mut FlexSelectState,
    model: &Model,
    model_index: usize,
    model_group_index: usize,
) {
    match file_status {
        FileStatus::Loading => {
            ui.spinner();
        }
        FileStatus::Loaded(file_data) => {
            for (part_index, (part_name, part)) in file_data.parts.iter().enumerate() {
                if model.disabled_parts.contains(part_name) {
                    continue;
                }

                if part.flexes.is_empty() {
                    continue;
                }

                egui::CollapsingHeader::new(part_name.clone()).show(ui, |ui| {
                    for (flex_index, flex_name) in part.flexes.keys().enumerate() {
                        if ui.button(flex_name).clicked() {
                            selection_state.active_model_group_index = model_group_index;
                            selection_state.active_model_index = model_index;
                            selection_state.active_part_index = part_index;
                            selection_state.active_flex_index = flex_index;
                        }
                    }
                });
            }
        }
        FileStatus::Failed => {
            ui.add(icon(IconType::X));
        }
    }
}
