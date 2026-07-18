use crate::interface::{fix_naming_conflicts, lists::ListPanel};

use super::TabViewer;
use eframe::egui;

impl<'a> TabViewer<'a> {
    pub fn render_sequences(&mut self, ui: &mut egui::Ui) {
        let mut selected_sequence = None;
        egui::Panel::right("Sequences Right Panel")
            .size_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show(ui, |ui| {
                selected_sequence = ListPanel::new("Sequences").show("Sequence", &mut self.input_data.sequences, ui, Default::default);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Sequences");
            ui.separator();

            if let Some(active_sequence_index) = selected_sequence {
                let name_label = ui.label("Sequence Name: ");
                if ui
                    .text_edit_singleline(&mut self.input_data.sequences[active_sequence_index].name)
                    .labelled_by(name_label.id)
                    .lost_focus()
                {
                    fix_naming_conflicts(&mut self.input_data.sequences, active_sequence_index);
                }

                let active_sequence = &mut self.input_data.sequences[active_sequence_index];
                if self.input_data.animations.is_empty() {
                    ui.colored_label(egui::Color32::RED, "No Animations Created");

                    if !active_sequence.animations.is_empty() {
                        active_sequence.animations.clear();
                    }

                    return;
                }

                if active_sequence.animations.is_empty() {
                    active_sequence.animations = vec![vec![0]];
                }

                let sequence_animation = &mut active_sequence.animations[0][0];

                let active_animation = self
                    .input_data
                    .animations
                    .iter()
                    .position(|animation| *sequence_animation == animation.animation_identifier);

                if active_animation.is_none() {
                    *sequence_animation = self.input_data.animations[0].animation_identifier;
                }

                egui::ComboBox::from_label("Selected Animation")
                    .selected_text(&self.input_data.animations[active_animation.unwrap_or_default()].name)
                    .show_ui(ui, |ui| {
                        for animation in &self.input_data.animations {
                            ui.selectable_value(sequence_animation, animation.animation_identifier, &animation.name);
                        }
                    });
                return;
            }
            ui.label("No Sequences");
        });
    }
}
