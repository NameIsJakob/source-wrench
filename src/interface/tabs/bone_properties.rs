use crate::{
    input::DefineBone,
    interface::{fix_naming_conflicts, lists::ListPanel},
};

use super::TabViewer;
use eframe::egui::{self, TextEdit};

impl<'a> TabViewer<'a> {
    pub fn render_bone_properties(&mut self, ui: &mut egui::Ui) {
        let mut selected_bone_property = None;
        egui::SidePanel::right("Bone Properties Right Panel")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                selected_bone_property = ListPanel::new("Bone Properties").show("Bone Property", &mut self.input_data.define_bones, ui, Default::default);
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Bone Properties");
            ui.separator();

            if let Some(active_bone_property_index) = selected_bone_property {
                self.render_properties(ui, active_bone_property_index);
            } else {
                ui.label("No Bone Properties");
            }
        });
    }

    fn render_properties(&mut self, ui: &mut egui::Ui, active_bone_property_index: usize) {
        ui.horizontal(|ui| {
            let name_label = ui.label("Name: ");
            if ui
                .text_edit_singleline(&mut self.input_data.define_bones[active_bone_property_index].name)
                .labelled_by(name_label.id)
                .lost_focus()
            {
                fix_naming_conflicts(&mut self.input_data.define_bones, active_bone_property_index);
            }
        });

        let active_bone_property = &mut self.input_data.define_bones[active_bone_property_index];
        render_hierarchy_options(ui, active_bone_property);
        render_transform_options(ui, active_bone_property);
    }
}

fn render_hierarchy_options(ui: &mut egui::Ui, active_bone_property: &mut DefineBone) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut active_bone_property.define_parent, "");

        let parent_label = ui.label("Parent: ");
        ui.add(TextEdit::singleline(&mut active_bone_property.parent).interactive(active_bone_property.define_parent))
            .labelled_by(parent_label.id);
    });
}

fn render_transform_options(ui: &mut egui::Ui, active_bone_property: &mut DefineBone) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut active_bone_property.define_location, "");
        ui.label("Location: ");
        if active_bone_property.define_location {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut active_bone_property.location.x));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut active_bone_property.location.y));
            ui.label("Z:");
            ui.add(egui::DragValue::new(&mut active_bone_property.location.z));
        } else {
            ui.label("Unlocked");
        }
    });

    ui.horizontal(|ui| {
        ui.checkbox(&mut active_bone_property.define_rotation, "");
        ui.label("Rotation: ");
        if active_bone_property.define_rotation {
            ui.label("X:");
            ui.add(egui::DragValue::new(&mut active_bone_property.rotation.x));
            ui.label("Y:");
            ui.add(egui::DragValue::new(&mut active_bone_property.rotation.y));
            ui.label("Z:");
            ui.add(egui::DragValue::new(&mut active_bone_property.rotation.z));
        } else {
            ui.label("Unlocked");
        }
    });
}
