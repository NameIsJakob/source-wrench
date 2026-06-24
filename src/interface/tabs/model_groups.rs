use crate::{
    import::{FileStatus, SUPPORTED_FILES},
    interface::{
        fix_naming_conflicts,
        icons::{IconType, icon},
        lists::ListPanel,
    },
};

use super::TabViewer;
use eframe::egui;

impl<'a> TabViewer<'a> {
    pub fn render_model_groups(&mut self, ui: &mut egui::Ui) {
        let mut selected_model_group = None;
        egui::SidePanel::right("Model Groups Right Panel")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                selected_model_group = ListPanel::new("Model Groups").show("Model Group", &mut self.input_data.model_groups, ui, Default::default);
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Model Groups");
            ui.separator();

            if let Some(active_model_group_index) = selected_model_group {
                self.render_model_panel(ui, active_model_group_index);
            } else {
                ui.label("No Model Groups");
            }
        });
    }

    fn render_model_panel(&mut self, ui: &mut egui::Ui, active_model_group_index: usize) {
        let mut selected_model = None;
        egui::SidePanel::left("Model Groups Models Panel")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                selected_model = ListPanel::new("Model Group Models").show(
                    "Model",
                    &mut self.input_data.model_groups[active_model_group_index].models,
                    ui,
                    Default::default,
                );
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name_label = ui.label("Model Group Name: ");
                    if ui
                        .text_edit_singleline(&mut self.input_data.model_groups[active_model_group_index].name)
                        .labelled_by(name_label.id)
                        .lost_focus()
                    {
                        fix_naming_conflicts(&mut self.input_data.model_groups, active_model_group_index);
                    }
                });

                ui.separator();

                if let Some(active_model_index) = selected_model {
                    self.render_model_options(ui, active_model_group_index, active_model_index);
                } else {
                    ui.label("No Models");
                }
            });
        });
    }

    fn render_model_options(&mut self, ui: &mut egui::Ui, active_model_group_index: usize, active_model_index: usize) {
        ui.horizontal(|ui| {
            let name_label = ui.label("Model Name: ");
            if ui
                .text_edit_singleline(&mut self.input_data.model_groups[active_model_group_index].models[active_model_index].name)
                .labelled_by(name_label.id)
                .lost_focus()
            {
                fix_naming_conflicts(&mut self.input_data.model_groups[active_model_group_index].models, active_model_index);
            }
        });

        let active_model = &mut self.input_data.model_groups[active_model_group_index].models[active_model_index];
        ui.checkbox(&mut active_model.blank, "Blank");
        if active_model.blank {
            return;
        }

        if ui.button("Select Model File…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_title("Select Model File")
                .add_filter("Supported Files", &SUPPORTED_FILES)
                .pick_file()
        {
            if let Some(last_path) = &active_model.source_file_path
                && last_path != &path
            {
                self.loaded_files.unload_file(last_path);
            };
            active_model.source_file_path = Some(path.clone());
            self.loaded_files.load_file(path);
        }

        if let Some(source_file_path) = &active_model.source_file_path
            && let Some(file_status) = self.loaded_files.get_file_status(source_file_path)
        {
            ui.horizontal(|ui| {
                ui.label("Model File:");
                ui.monospace(source_file_path.display().to_string());
                match file_status {
                    FileStatus::Loading => {
                        ui.spinner();
                    }
                    FileStatus::Loaded(_) => {
                        ui.add(icon(IconType::Check));
                    }
                    FileStatus::Failed => {
                        ui.add(icon(IconType::X));
                    }
                }
            });

            if let FileStatus::Loaded(file_data) = file_status {
                if file_data.parts.is_empty() {
                    ui.colored_label(egui::Color32::RED, "Model File Has No Mesh!");
                    return;
                }

                ui.heading("Enabled Parts");
                ui.separator();
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    for (part_name, _) in &file_data.parts {
                        let mut enabled = !active_model.disabled_parts.contains(part_name);
                        if ui.checkbox(&mut enabled, part_name).changed() {
                            if enabled {
                                active_model.disabled_parts.swap_remove(part_name);
                            } else {
                                active_model.disabled_parts.insert(part_name.clone());
                            }
                        }
                    }
                });
            }
        }
    }
}
