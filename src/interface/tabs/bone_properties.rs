use crate::{
    input::BoneProperty,
    interface::{fix_naming_conflicts, lists::ListPanel},
};

use super::TabViewer;
use eframe::egui::{self, TextEdit};

impl<'a> TabViewer<'a> {
    pub fn render_bone_properties(&mut self, ui: &mut egui::Ui) {
        let mut selected_bone_property = None;
        egui::Panel::right("Bone Properties Right Panel")
            .size_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show(ui, |ui| {
                selected_bone_property = ListPanel::new("Bone Properties").show("Bone Property", &mut self.input_data.bone_properties, ui, Default::default);
            });

        egui::CentralPanel::default().show(ui, |ui| {
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
                .text_edit_singleline(&mut self.input_data.bone_properties[active_bone_property_index].name)
                .labelled_by(name_label.id)
                .lost_focus()
            {
                fix_naming_conflicts(&mut self.input_data.bone_properties, active_bone_property_index);
            }
        });

        let active_bone_property = &mut self.input_data.bone_properties[active_bone_property_index];
        render_hierarchy_options(ui, active_bone_property);
        render_transform_options(ui, active_bone_property);
        render_ik_chain_options(ui, active_bone_property);
    }
}

fn render_hierarchy_options(ui: &mut egui::Ui, active_bone_property: &mut BoneProperty) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut active_bone_property.define_parent, "");

        let parent_label = ui.label("Parent: ");
        ui.add(TextEdit::singleline(&mut active_bone_property.parent).interactive(active_bone_property.define_parent))
            .labelled_by(parent_label.id);
    });
}

fn render_transform_options(ui: &mut egui::Ui, active_bone_property: &mut BoneProperty) {
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

fn render_ik_chain_options(ui: &mut egui::Ui, active_bone_property: &mut BoneProperty) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut active_bone_property.ik_chain, "");
        ui.label("IK Chain: ");
        if !active_bone_property.ik_chain {
            ui.label("Not A Chain");
        }
    });

    egui::CollapsingHeader::new("Ik Chain Data")
        .enabled(active_bone_property.ik_chain)
        .open(if active_bone_property.ik_chain { None } else { Some(false) })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let name_label = ui.label("name: ");
                ui.add(TextEdit::singleline(&mut active_bone_property.ik_chain_name)).labelled_by(name_label.id);
            });

            ui.horizontal(|ui| {
                ui.label("Knee Direction: ");
                ui.label("X:");
                ui.add(egui::DragValue::new(&mut active_bone_property.ik_chain_knee.x).range(-1.0..=1.0).speed(0.01));
                ui.label("Y:");
                ui.add(egui::DragValue::new(&mut active_bone_property.ik_chain_knee.y).range(-1.0..=1.0).speed(0.01));
                ui.label("Z:");
                ui.add(egui::DragValue::new(&mut active_bone_property.ik_chain_knee.z).range(-1.0..=1.0).speed(0.01));
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut active_bone_property.ik_chain_auto_play, "");
                ui.label("Auto Play Lock: ");

                if active_bone_property.ik_chain_auto_play {
                    ui.label("Position Lock: ");
                    ui.add(
                        egui::DragValue::new(&mut active_bone_property.ik_chain_position_lock)
                            .range(-1.0..=1.0)
                            .speed(0.01),
                    );
                    ui.label("Rotation Lock: ");
                    ui.add(
                        egui::DragValue::new(&mut active_bone_property.ik_chain_rotation_lock)
                            .range(-1.0..=1.0)
                            .speed(0.01),
                    );
                } else {
                    ui.label("No Lock");
                }
            });
        });
}
