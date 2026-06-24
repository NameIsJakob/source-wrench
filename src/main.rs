#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui_dock::DockState;
use std::sync::{Arc, atomic::AtomicBool};

mod import;
mod input;
mod interface;
mod process;
mod utilities;
mod write;

use import::FileManager;
use interface::tabs::{TabViewer, UniqueTabs};
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
    tab_tree: DockState<UniqueTabs>,
    compiling: Arc<AtomicBool>,
    input_data: input::SourceInput,
    loaded_files: FileManager,
}

impl Default for SourceWrenchApplication {
    fn default() -> Self {
        let mut tree = DockState::new(vec![UniqueTabs::Overview]);

        let [main_tab, logging_tab] = tree.main_surface_mut().split_right(egui_dock::NodeIndex::root(), 0.5, vec![UniqueTabs::Log]);

        let [_, _] = tree
            .main_surface_mut()
            .split_below(main_tab, 0.35, vec![UniqueTabs::ModelGroups, UniqueTabs::Flexing, UniqueTabs::BoneProperties]);

        let [_, _] = tree
            .main_surface_mut()
            .split_below(logging_tab, 0.35, vec![UniqueTabs::Animations, UniqueTabs::Sequences]);

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
                &mut TabViewer::create(Arc::clone(&self.compiling), &mut self.input_data, &mut self.loaded_files),
            );
    }
}
