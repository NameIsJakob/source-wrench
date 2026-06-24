use std::sync::{Arc, atomic::Ordering};

use crate::{error, info, process, write};

use super::TabViewer;
use eframe::egui;

impl<'a> TabViewer<'a> {
    pub fn render_overview(&mut self, ui: &mut egui::Ui) {
        self.render_header(ui);
        self.render_output_input(ui);

        if let Some(export_path) = &self.input_data.export_path {
            let name_label = ui.label("Model Name: ");
            ui.text_edit_singleline(&mut self.input_data.model_name).labelled_by(name_label.id);
            let is_compiling = self.compiling.load(std::sync::atomic::Ordering::Relaxed);
            let button_response = ui.add_enabled(!is_compiling, egui::Button::new("Compile Model"));
            if button_response.clicked() {
                self.compile_model(export_path.to_string_lossy().to_string());
            }
        } else {
            ui.label("Output Directory Needed To Compile Model!");
        }
    }

    fn render_header(&self, ui: &mut egui::Ui) {
        ui.heading("Source Wrench");
        ui.label("A Source Engine Model Compiler");
        ui.separator();
    }

    fn render_output_input(&mut self, ui: &mut egui::Ui) {
        let name_label = ui.label("Model Out Directory: ");
        let mut path_text = self
            .input_data
            .export_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| String::from("Select a directory..."));

        if ui.text_edit_singleline(&mut path_text).labelled_by(name_label.id).clicked()
            && let Some(path) = rfd::FileDialog::new().set_title("Select Export Path").pick_folder()
        {
            self.input_data.export_path = Some(path);
        }
    }

    fn compile_model(&mut self, export_path: String) {
        // The best thing to do is just to clone the data.
        let input_data = self.input_data.clone();
        let loaded_files = self.loaded_files.clone();
        let compiling = Arc::clone(&self.compiling);
        compiling.store(true, std::sync::atomic::Ordering::Relaxed);
        // TODO: Add a cancel button.
        std::thread::spawn(move || {
            if input_data.model_name.is_empty() {
                error!("Model name is empty!");
                compiling.store(false, Ordering::Relaxed);
                return;
            }

            let mut model_name = input_data.model_name.clone();
            if !model_name.ends_with(".mdl") {
                model_name.push_str(".mdl");
            }

            info!("Processing {}!", &model_name);

            let processed_data = match process::compile_data(&input_data, &loaded_files) {
                Ok(data) => data,
                Err(error) => {
                    error!("Fail To Compile Model: {error}!");
                    compiling.store(false, Ordering::Relaxed);
                    return;
                }
            };

            info!("Writing Files!");

            match write::write_files(input_data.model_name.clone(), model_name, processed_data, export_path) {
                Ok(_) => {}
                Err(error) => {
                    error!("Fail To Write Files: {error}!");
                    compiling.store(false, Ordering::Relaxed);
                    return;
                }
            }

            info!("Model compiled successfully!");
            compiling.store(false, Ordering::Relaxed);
        });
    }
}
