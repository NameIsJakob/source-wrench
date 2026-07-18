use std::sync::Arc;

use crate::{
    import::{FileData, FileManager, FileStatus, SUPPORTED_FILES},
    input::Animation,
    interface::{
        fix_naming_conflicts,
        icons::{IconType, icon},
        lists::ListPanel,
    },
};

use super::TabViewer;
use eframe::egui;

impl<'a> TabViewer<'a> {
    pub fn render_animation(&mut self, ui: &mut egui::Ui) {
        let mut selected_animation = None;
        egui::Panel::right("Animations Right Panel")
            .size_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show(ui, |ui| {
                selected_animation = ListPanel::new("Animations").show("Animation", &mut self.input_data.animations, ui, || {
                    let new_animation = Animation {
                        animation_identifier: self.input_data.animation_identifier_generator,
                        ..Default::default()
                    };
                    self.input_data.animation_identifier_generator += 1;
                    new_animation
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Animations");
            ui.separator();

            if let Some(active_animation_index) = selected_animation {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    let name_label = ui.label("Animation Name: ");
                    if ui
                        .text_edit_singleline(&mut self.input_data.animations[active_animation_index].name)
                        .labelled_by(name_label.id)
                        .lost_focus()
                    {
                        fix_naming_conflicts(&mut self.input_data.animations, active_animation_index);
                    }

                    let active_animation = &mut self.input_data.animations[active_animation_index];
                    render_file_selection(ui, self.loaded_files, active_animation);

                    if let Some(source_file_path) = &active_animation.source_file_path
                        && let Some(file_status) = self.loaded_files.get_file_status(source_file_path)
                    {
                        render_file_status(ui, &file_status, active_animation);

                        if let FileStatus::Loaded(file_data) = file_status {
                            render_options(ui, file_data, active_animation);
                        }
                    }
                });
            } else {
                ui.label("No Animations");
            }
        });
    }
}

fn render_file_selection(ui: &mut egui::Ui, file_manager: &mut FileManager, active_animation: &mut Animation) {
    if ui.button("Select Model File…").clicked()
        && let Some(path) = rfd::FileDialog::new()
            .set_title("Select Model File")
            .add_filter("Supported Files", &SUPPORTED_FILES)
            .pick_file()
    {
        if let Some(last_path) = &active_animation.source_file_path {
            file_manager.unload_file(last_path);
        };
        active_animation.source_file_path = Some(path.clone());
        file_manager.load_file(path);
    }
}

fn render_file_status(ui: &mut egui::Ui, file_status: &FileStatus, active_animation: &mut Animation) {
    ui.horizontal(|ui| {
        ui.label("Animation File:");
        ui.monospace(active_animation.source_file_path.as_ref().unwrap().display().to_string());
        match file_status {
            FileStatus::Loading => {
                ui.spinner();
                active_animation.source_animation = 0;
            }
            FileStatus::Loaded(_) => {
                ui.add(icon(IconType::Check));
            }
            FileStatus::Failed => {
                ui.add(icon(IconType::X));
                active_animation.source_animation = 0;
            }
        }
    });
}

fn render_options(ui: &mut egui::Ui, file_data: Arc<FileData>, active_animation: &mut Animation) {
    if active_animation.source_animation > file_data.animations.len() {
        active_animation.source_animation = 0;
    }

    ui.separator();
    egui::ComboBox::from_label("Source Animation")
        .selected_text(file_data.animations.get_index(active_animation.source_animation).unwrap().0)
        .show_ui(ui, |ui| {
            for (source_animation_index, (source_animation_name, _)) in file_data.animations.iter().enumerate() {
                ui.selectable_value(&mut active_animation.source_animation, source_animation_index, source_animation_name);
            }
        });
}
