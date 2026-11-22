use std::{fmt::Debug, hash::Hash};

use eframe::egui::{AsIdSalt, Button, Frame, IdSalt, Label, ScrollArea, Sense, Ui, vec2};

use crate::{input::NamedData, interface::enforce_unique_name};

pub struct ListSelection {
    list_id: IdSalt,
}

impl ListSelection {
    pub fn new(id: impl AsIdSalt) -> Self {
        Self { list_id: IdSalt::new(id) }
    }

    pub fn show<T: NamedData + Hash + Debug>(self, ui: &mut Ui, display: &str, entries: &mut Vec<T>, create_entry: impl FnOnce() -> T) -> Option<usize> {
        if ui
            .add_sized(vec2(ui.available_width(), ui.spacing().interact_size.y), Button::new(format!("Add {display}")))
            .clicked()
        {
            let check_index = entries.len();
            entries.push(create_entry());
            enforce_unique_name(entries, check_index);
        }

        let remove_active_entry = ui
            .add_sized(
                vec2(ui.available_width(), ui.spacing().interact_size.y),
                Button::new(format!("Remove {display}")),
            )
            .clicked();

        let dnd_id = ui.make_persistent_id(self.list_id);
        let active_index_id = dnd_id.with("ListSelection");
        let mut active_index = ui
            .ctx()
            .data_mut::<Option<usize>>(|data| data.get_persisted(active_index_id))
            .unwrap_or_default()
            .min(entries.len().saturating_sub(1));

        Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(ui.visuals().noninteractive().bg_stroke)
            .inner_margin(ui.spacing().menu_margin)
            .show(ui, |ui| {
                ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    let dnd_response =
                        egui_dnd::Dnd::new(ui, dnd_id)
                            .with_drag_axis(egui_dnd::DragAxis::Vertical)
                            .show_vec(entries, |ui, item, handle, state| {
                                if handle
                                    .sense(Sense::click())
                                    .ui(ui, |ui| {
                                        Frame::new()
                                            .fill(if state.dragged {
                                                ui.visuals().widgets.active.bg_fill
                                            } else {
                                                ui.visuals().disable(ui.visuals().widgets.inactive.bg_fill)
                                            })
                                            .stroke(if state.index == active_index {
                                                ui.visuals().widgets.active.bg_stroke
                                            } else {
                                                ui.visuals().widgets.inactive.bg_stroke
                                            })
                                            .show(ui, |ui| {
                                                ui.add_sized(
                                                    vec2(ui.available_width(), ui.spacing().interact_size.y),
                                                    Label::new(item.get_name()).selectable(false),
                                                );
                                            });
                                    })
                                    .clicked()
                                {
                                    active_index = state.index;
                                }
                            });

                    if let Some(updated) = dnd_response.update {
                        let destination = if updated.from < updated.to { updated.to - 1 } else { updated.to };
                        if active_index == updated.from {
                            active_index = destination;
                        } else if updated.from < active_index && active_index <= destination {
                            active_index -= 1;
                        } else if destination <= active_index && active_index < updated.from {
                            active_index += 1;
                        }
                    }
                });
            });

        if remove_active_entry && !entries.is_empty() {
            entries.remove(active_index);
            active_index = active_index.saturating_sub(1);
        }

        ui.ctx().data_mut(|data| data.insert_persisted(active_index_id, active_index));

        if entries.is_empty() {
            return None;
        }

        Some(active_index)
    }
}
