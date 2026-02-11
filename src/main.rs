#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{self, TextEdit};
use egui_dock::DockState;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

mod import;
mod input;
mod interface;
mod process;
mod utilities;
mod write;

use import::{FileManager, FileStatus, SUPPORTED_FILES};
use interface::{icon, toggle_ui_compact};
use utilities::logging;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_maximized(true).with_drag_and_drop(false),
        centered: true,
        ..Default::default()
    };
    eframe::run_native("Source Wrench", options, Box::new(|_| Ok(Box::<SourceWrenchApplication>::default())))
}

struct SourceWrenchApplication {
    tab_tree: DockState<SourceWrenchTabType>,
    compiling: Arc<AtomicBool>,
    input_data: input::SourceInput,
    loaded_files: FileManager,
}

impl Default for SourceWrenchApplication {
    fn default() -> Self {
        let mut tree = DockState::new(vec![SourceWrenchTabType::Main]);

        let [main_tab, logging_tab] = tree
            .main_surface_mut()
            .split_right(egui_dock::NodeIndex::root(), 0.5, vec![SourceWrenchTabType::Logging]);

        let [_, _] = tree
            .main_surface_mut()
            .split_below(main_tab, 0.35, vec![SourceWrenchTabType::ModelGroups, SourceWrenchTabType::DefineBones]);

        let [_, _] = tree
            .main_surface_mut()
            .split_below(logging_tab, 0.35, vec![SourceWrenchTabType::Animations, SourceWrenchTabType::Sequences]);

        let mut loaded_files = FileManager::default();

        if let Err(watch_error) = loaded_files.start_file_watch() {
            error!("Fail To Start File Watch: {watch_error}!");
        }

        Self {
            tab_tree: tree,
            compiling: Arc::new(AtomicBool::new(false)),
            input_data: Default::default(),
            loaded_files,
        }
    }
}

impl eframe::App for SourceWrenchApplication {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        logging::set_ui_context(ctx.clone());
        egui_dock::DockArea::new(&mut self.tab_tree)
            .style(egui_dock::Style::from_egui(ctx.style().as_ref()))
            .show_close_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show(
                ctx,
                &mut SourceWrenchTabManager {
                    compiling: Arc::clone(&self.compiling),
                    input_data: &mut self.input_data,
                    loaded_files: &mut self.loaded_files,
                },
            );
    }
}

enum SourceWrenchTabType {
    Main,
    Logging,
    ModelGroups,
    DefineBones,
    Animations,
    Sequences,
}

struct SourceWrenchTabManager<'a> {
    compiling: Arc<AtomicBool>,
    input_data: &'a mut input::SourceInput,
    loaded_files: &'a mut FileManager,
}

impl egui_dock::TabViewer for SourceWrenchTabManager<'_> {
    type Tab = SourceWrenchTabType;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match &tab {
            SourceWrenchTabType::Main => String::from("Main").into(),
            SourceWrenchTabType::Logging => String::from("Log").into(),
            SourceWrenchTabType::ModelGroups => String::from("Model Groups").into(),
            SourceWrenchTabType::DefineBones => String::from("Define Bones").into(),
            SourceWrenchTabType::Animations => String::from("Animations").into(),
            SourceWrenchTabType::Sequences => String::from("Sequences").into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            SourceWrenchTabType::Main => self.render_main(ui),
            SourceWrenchTabType::Logging => self.render_logging(ui),
            SourceWrenchTabType::ModelGroups => self.render_model_groups(ui),
            SourceWrenchTabType::DefineBones => self.render_define_bones(ui),
            SourceWrenchTabType::Animations => self.render_animations(ui),
            SourceWrenchTabType::Sequences => self.render_sequences(ui),
        }
    }

    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
        [false, false]
    }
}

macro_rules! check_name_conflict {
    ($elements:expr, $check_index:ident) => {
        while $elements
            .iter()
            .enumerate()
            .any(|(element_index, element)| element_index != $check_index && element.name == $elements[$check_index].name)
        {
            if let Some(numbered_index) = $elements[$check_index].name.rfind('#') {
                let (name, number) = $elements[$check_index].name.split_at(numbered_index);
                if let Ok(index) = number[1..].parse::<usize>() {
                    $elements[$check_index].name = format!("{}#{}", name, index + 1);
                    continue;
                }
            }
            $elements[$check_index].name = format!("{} #0", $elements[$check_index].name);
        }
    };
}

impl SourceWrenchTabManager<'_> {
    fn render_main(&mut self, ui: &mut egui::Ui) {
        ui.heading("Source Wrench");
        ui.label("A Source Engine Model Compiler");

        ui.separator();

        let name_label = ui.label("Model Out Directory: ");
        if ui
            .text_edit_singleline(
                &mut self
                    .input_data
                    .export_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| String::from("Select a directory...")),
            )
            .labelled_by(name_label.id)
            .clicked()
            && let Some(path) = rfd::FileDialog::new().set_title("Select Export Path").pick_folder()
        {
            self.input_data.export_path = Some(path);
        }

        if let Some(export_path) = &self.input_data.export_path {
            let name_label = ui.label("Model Name: ");
            ui.text_edit_singleline(&mut self.input_data.model_name).labelled_by(name_label.id);

            let is_compiling = self.compiling.load(std::sync::atomic::Ordering::Relaxed);

            let button_response = ui.add_enabled(!is_compiling, egui::Button::new("Compile Model"));
            if button_response.clicked() {
                // The best thing to do is just to clone the data.
                let input_data = self.input_data.clone();
                let loaded_files = self.loaded_files.clone();
                let export_path = export_path.to_string_lossy().to_string();
                let compiling = Arc::clone(&self.compiling);
                compiling.store(true, std::sync::atomic::Ordering::Relaxed);

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
    }

    fn render_logging(&mut self, ui: &mut egui::Ui) {
        let mut logger = logging::LOGGER.lock();
        ui.horizontal(|ui| {
            ui.horizontal(|ui| {
                ui.label("Verbose:");
                toggle_ui_compact(ui, &mut logger.allow_verbose);

                ui.label("Debug:");
                toggle_ui_compact(ui, &mut logger.allow_debug);
            });

            if ui.button("Clear Log").clicked() {
                logger.logs.clear();
            }
        });

        ui.separator();

        egui::ScrollArea::vertical().auto_shrink([false; 2]).stick_to_bottom(true).show(ui, |ui| {
            for (log, level) in &logger.logs {
                let log_color = match level {
                    logging::LogLevel::Info => egui::Color32::DARK_GREEN,
                    logging::LogLevel::Verbose => egui::Color32::MAGENTA,
                    logging::LogLevel::Debug => egui::Color32::CYAN,
                    logging::LogLevel::Warn => egui::Color32::YELLOW,
                    logging::LogLevel::Error => egui::Color32::RED,
                };

                if matches!(level, logging::LogLevel::Verbose) && !logger.allow_verbose {
                    continue;
                }

                if matches!(level, logging::LogLevel::Debug) && !logger.allow_debug {
                    continue;
                }

                ui.colored_label(log_color, log);
                ui.separator();
            }
        });
    }

    fn render_model_groups(&mut self, ui: &mut egui::Ui) {
        let mut remove_active_model_group = false;
        let mut selected_model_group = None;
        let mut updated_model_group_name = false;

        egui::SidePanel::right("Model Groups Right Panel")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                if ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Add Model Group"),
                    )
                    .clicked()
                {
                    let new_model_group_index = self.input_data.model_groups.len();
                    self.input_data.model_groups.push(Default::default());
                    check_name_conflict!(self.input_data.model_groups, new_model_group_index);
                }

                remove_active_model_group = ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Remove Model Group"),
                    )
                    .clicked();

                selected_model_group = interface::ListSelect::new("Model Groups").show(&mut self.input_data.model_groups, ui, |ui, entry| {
                    ui.add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Label::new(&entry.name).selectable(false),
                    );
                })
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Model Groups");
            ui.separator();

            if let Some(active_model_group_index) = selected_model_group {
                let active_model_group = &mut self.input_data.model_groups[active_model_group_index];

                let mut remove_active_model = false;
                let mut selected_model = None;
                let mut updated_model_name = false;

                egui::SidePanel::left("Model Groups Models Panel")
                    .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
                    .show_inside(ui, |ui| {
                        if ui
                            .add_sized(egui::vec2(ui.available_width(), ui.spacing().interact_size.y), egui::Button::new("Add Model"))
                            .clicked()
                        {
                            let new_model_index = active_model_group.models.len();
                            active_model_group.models.push(Default::default());
                            check_name_conflict!(active_model_group.models, new_model_index);
                        }

                        remove_active_model = ui
                            .add_sized(
                                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                                egui::Button::new("Remove Model"),
                            )
                            .clicked();

                        selected_model = interface::ListSelect::new("Model Groups Models").show(&mut active_model_group.models, ui, |ui, entry| {
                            ui.add_sized(
                                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                                egui::Label::new(if entry.blank {
                                    egui::RichText::new(&entry.name).strikethrough()
                                } else {
                                    egui::RichText::new(&entry.name)
                                })
                                .selectable(false),
                            );
                        })
                    });

                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    egui::Frame::new().inner_margin(5.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let name_label = ui.label("Model Group Name: ");
                            updated_model_group_name = ui.text_edit_singleline(&mut active_model_group.name).labelled_by(name_label.id).lost_focus();
                        });

                        ui.separator();

                        if let Some(active_model_index) = selected_model {
                            let active_model = &mut active_model_group.models[active_model_index];
                            ui.horizontal(|ui| {
                                let name_label = ui.label("Model Name: ");
                                updated_model_name = ui.text_edit_singleline(&mut active_model.name).labelled_by(name_label.id).lost_focus();
                            });

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
                                            ui.add(icon(interface::IconType::Check));
                                        }
                                        FileStatus::Failed => {
                                            ui.add(icon(interface::IconType::X));
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

                            return;
                        }

                        ui.label("No Models");
                    });

                    if updated_model_name && let Some(active_model) = selected_model {
                        check_name_conflict!(active_model_group.models, active_model);
                    }

                    if remove_active_model && let Some(active_model) = selected_model {
                        let removed = active_model_group.models.remove(active_model);
                        if let Some(removed_path) = removed.source_file_path {
                            self.loaded_files.unload_file(&removed_path);
                        }
                    }
                });
                return;
            }

            // TODO: Keep the models menu shown with buttons disabled.
            ui.label("No Model Groups");
        });

        if updated_model_group_name && let Some(active_model_group_index) = selected_model_group {
            check_name_conflict!(self.input_data.model_groups, active_model_group_index);
        }

        if remove_active_model_group && let Some(active_model_group) = selected_model_group {
            let removed = self.input_data.model_groups.remove(active_model_group);
            for removed_model in removed.models {
                if let Some(removed_path) = removed_model.source_file_path {
                    self.loaded_files.unload_file(&removed_path);
                }
            }
        }
    }

    fn render_define_bones(&mut self, ui: &mut egui::Ui) {
        let mut remove_active_define_bone = false;
        let mut selected_define_bone = None;
        let mut updated_define_bone_name = false;

        egui::SidePanel::right("Define Bone List")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                if ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Add Define Bone"),
                    )
                    .clicked()
                {
                    let new_define_bone_index = self.input_data.define_bones.len();
                    self.input_data.define_bones.push(Default::default());
                    check_name_conflict!(self.input_data.define_bones, new_define_bone_index);
                }

                remove_active_define_bone = ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Remove Define Bone"),
                    )
                    .clicked();

                selected_define_bone = interface::ListSelect::new("Define Bone").show(&mut self.input_data.define_bones, ui, |ui, entry| {
                    ui.add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Label::new(&entry.name).selectable(false),
                    );
                })
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Define Bones");
            ui.separator();

            if let Some(active_define_bone_index) = selected_define_bone {
                let active_define_bone = &mut self.input_data.define_bones[active_define_bone_index];
                ui.horizontal(|ui| {
                    let name_label = ui.label("Name: ");
                    updated_define_bone_name = ui.text_edit_singleline(&mut active_define_bone.name).labelled_by(name_label.id).lost_focus();
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut active_define_bone.define_parent, "");

                    let parent_label = ui.label("Parent: ");
                    ui.add(TextEdit::singleline(&mut active_define_bone.parent).interactive(active_define_bone.define_parent))
                        .labelled_by(parent_label.id);
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut active_define_bone.define_location, "");
                    ui.label("Location: ");
                    if active_define_bone.define_location {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut active_define_bone.location.x));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut active_define_bone.location.y));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut active_define_bone.location.z));
                    } else {
                        ui.label("Unlocked");
                    }
                });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut active_define_bone.define_rotation, "");
                    ui.label("Rotation: ");
                    if active_define_bone.define_rotation {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut active_define_bone.rotation.x));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut active_define_bone.rotation.y));
                        ui.label("Z:");
                        ui.add(egui::DragValue::new(&mut active_define_bone.rotation.z));
                    } else {
                        ui.label("Unlocked");
                    }
                });

                return;
            }
            ui.label("No Define Bones");
        });

        if updated_define_bone_name && let Some(active_define_bone) = selected_define_bone {
            check_name_conflict!(self.input_data.define_bones, active_define_bone);
        }

        if remove_active_define_bone && let Some(active_define_bone) = selected_define_bone {
            self.input_data.define_bones.remove(active_define_bone);
        }
    }

    fn render_animations(&mut self, ui: &mut egui::Ui) {
        let mut remove_active_animation = false;
        let mut selected_animation = None;
        let mut updated_animation_name = false;

        egui::SidePanel::right("Animations Right Panel")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                if ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Add Animation"),
                    )
                    .clicked()
                {
                    let new_animation_index = self.input_data.animations.len();
                    let new_animation = input::Animation {
                        animation_identifier: self.input_data.animation_identifier_generator,
                        ..Default::default()
                    };
                    self.input_data.animation_identifier_generator += 1;
                    self.input_data.animations.push(new_animation);
                    check_name_conflict!(self.input_data.animations, new_animation_index);
                }

                remove_active_animation = ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Remove Animation"),
                    )
                    .clicked();

                selected_animation = interface::ListSelect::new("Animations").show(&mut self.input_data.animations, ui, |ui, entry| {
                    ui.add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Label::new(&entry.name).selectable(false),
                    );
                })
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Animations");
            ui.separator();

            if let Some(active_animation) = selected_animation {
                let active_animation = &mut self.input_data.animations[active_animation];

                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    let name_label = ui.label("Animation Name: ");
                    updated_animation_name = ui.text_edit_singleline(&mut active_animation.name).labelled_by(name_label.id).lost_focus();

                    if ui.button("Select Model File…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Select Model File")
                            .add_filter("Supported Files", &SUPPORTED_FILES)
                            .pick_file()
                    {
                        if let Some(last_path) = &active_animation.source_file_path {
                            self.loaded_files.unload_file(last_path);
                        };
                        active_animation.source_file_path = Some(path.clone());
                        self.loaded_files.load_file(path);
                    }

                    if let Some(source_file_path) = &active_animation.source_file_path
                        && let Some(file_status) = self.loaded_files.get_file_status(source_file_path)
                    {
                        ui.horizontal(|ui| {
                            ui.label("Animation File:");
                            ui.monospace(source_file_path.display().to_string());
                            match file_status {
                                FileStatus::Loading => {
                                    ui.spinner();
                                    active_animation.source_animation = 0;
                                }
                                FileStatus::Loaded(_) => {
                                    ui.add(icon(interface::IconType::Check));
                                }
                                FileStatus::Failed => {
                                    ui.add(icon(interface::IconType::X));
                                    active_animation.source_animation = 0;
                                }
                            }
                        });

                        if let FileStatus::Loaded(file_data) = file_status {
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
                    }
                });
                return;
            }
            ui.label("No Animations");
        });

        if updated_animation_name && let Some(active_animation) = selected_animation {
            check_name_conflict!(self.input_data.animations, active_animation);
        }

        if remove_active_animation && let Some(active_animation) = selected_animation {
            let removed = self.input_data.animations.remove(active_animation);
            if let Some(removed_path) = removed.source_file_path {
                self.loaded_files.unload_file(&removed_path);
            }
        }
    }

    fn render_sequences(&mut self, ui: &mut egui::Ui) {
        let mut remove_active_sequence = false;
        let mut selected_sequence = None;
        let mut updated_sequence_name = false;

        egui::SidePanel::right("Sequences Right Panel")
            .width_range(egui::Rangef::new(ui.available_width() * 0.2, ui.available_width() * 0.5))
            .show_inside(ui, |ui| {
                if ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Add Sequence"),
                    )
                    .clicked()
                {
                    let new_sequence_index = self.input_data.sequences.len();
                    self.input_data.sequences.push(Default::default());
                    check_name_conflict!(self.input_data.sequences, new_sequence_index);
                }

                remove_active_sequence = ui
                    .add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Button::new("Remove Sequence"),
                    )
                    .clicked();

                selected_sequence = interface::ListSelect::new("Sequences").show(&mut self.input_data.sequences, ui, |ui, entry| {
                    ui.add_sized(
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                        egui::Label::new(&entry.name).selectable(false),
                    );
                })
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Sequences");
            ui.separator();

            if let Some(active_sequence) = selected_sequence {
                let active_sequence = &mut self.input_data.sequences[active_sequence];

                let name_label = ui.label("Sequence Name: ");
                updated_sequence_name = ui.text_edit_singleline(&mut active_sequence.name).labelled_by(name_label.id).lost_focus();

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

        if updated_sequence_name && let Some(active_sequence) = selected_sequence {
            check_name_conflict!(self.input_data.sequences, active_sequence);
        }

        if remove_active_sequence && let Some(active_sequence) = selected_sequence {
            self.input_data.sequences.remove(active_sequence);
        }
    }
}
