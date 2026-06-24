use eframe::{
    egui::{Button, Context, DragAndDrop, Frame, Id, Label, LayerId, Order, Rect, ScrollArea, Sense, Ui, UiBuilder, Vec2, vec2},
    emath::TSTransform,
};

pub struct ListPanel {
    list_id: Id,
}

impl ListPanel {
    pub fn new(id: impl Into<Id>) -> Self {
        Self { list_id: id.into() }
    }

    pub fn show<T: crate::input::NamedData>(self, entry_name: &str, entries: &mut Vec<T>, ui: &mut Ui, create_entry: impl FnOnce() -> T) -> Option<usize> {
        if ui
            .add_sized(
                vec2(ui.available_width(), ui.spacing().interact_size.y),
                Button::new(format!("Add {entry_name}")),
            )
            .clicked()
        {
            let new_entry_index = entries.len();
            entries.push(create_entry());
            super::fix_naming_conflicts(entries, new_entry_index);
        }

        let remove_active_entry = ui
            .add_sized(
                vec2(ui.available_width(), ui.spacing().interact_size.y),
                Button::new(format!("Remove {entry_name}")),
            )
            .clicked();

        let selected_entry = ListSelect::new(self.list_id).show(entries, ui, |ui, entry| {
            ui.add_sized(
                vec2(ui.available_width(), ui.spacing().interact_size.y),
                Label::new(entry.get_name()).selectable(false),
            );
        });

        if remove_active_entry && let Some(active_entry) = selected_entry {
            entries.remove(active_entry);

            if entries.is_empty() {
                return None;
            }

            return Some(active_entry.saturating_sub(1));
        }

        selected_entry
    }
}

#[derive(Clone, Default)]
struct ListSelectState {
    selected_entry: usize,
}

impl ListSelectState {
    fn load(ctx: &Context, id: Id) -> Option<Self> {
        ctx.data_mut(|data| data.get_persisted(id))
    }

    fn store(self, ctx: &Context, id: Id) {
        ctx.data_mut(|data| data.insert_persisted(id, self));
    }
}

struct ListSelect {
    list_id: Id,
}

impl ListSelect {
    fn new(id: impl Into<Id>) -> Self {
        Self { list_id: id.into() }
    }

    fn show<T>(self, entries: &mut [T], ui: &mut Ui, entry_contents: impl Fn(&mut Ui, &T)) -> Option<usize> {
        let context = ui.ctx();
        let persistent_id = ui.make_persistent_id(self.list_id);
        context.check_for_id_clash(
            persistent_id,
            Rect::from_min_size(ui.available_rect_before_wrap().min, Vec2::ZERO),
            "DraggableSelectableList",
        );

        let mut state = ListSelectState::load(context, persistent_id).unwrap_or_default();
        state.selected_entry = state.selected_entry.min(entries.len().max(1) - 1);

        Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .inner_margin(2.5)
            .show(ui, |ui| {
                ScrollArea::vertical().auto_shrink([false, false]).scroll([false, true]).show(ui, |ui| {
                    Frame::new().inner_margin(ui.spacing().button_padding).show(ui, |ui| {
                        let mut replace = None;

                        // TODO: Animate the shifting of entries.
                        // TODO: Add filter inputs at the bottom.
                        for (entry_index, entry) in entries.iter().enumerate() {
                            let entry_id = self.list_id.with(entry_index);

                            let is_being_dragged = ui.ctx().is_being_dragged(entry_id);
                            if is_being_dragged {
                                DragAndDrop::set_payload(ui.ctx(), entry_index);

                                let layer_id = LayerId::new(Order::Tooltip, entry_id);
                                let response = ui
                                    .scope_builder(UiBuilder::new().layer_id(layer_id), |ui| {
                                        let mut entry_frame = Frame::new().begin(ui);
                                        entry_contents(&mut entry_frame.content_ui, entry);
                                        entry_frame.allocate_space(ui);

                                        let fill = if entry_index == state.selected_entry {
                                            ui.visuals().widgets.active.bg_fill
                                        } else {
                                            ui.visuals().disable(ui.visuals().widgets.inactive.bg_fill)
                                        };

                                        let stroke = ui.visuals().widgets.active.bg_stroke;

                                        entry_frame.frame.fill = fill;
                                        entry_frame.frame.stroke = stroke;

                                        entry_frame.paint(ui);
                                    })
                                    .response;

                                if !response.contains_pointer()
                                    && let Some(pointer_pos) = ui.ctx().pointer_interact_pos()
                                {
                                    let delta = pointer_pos - response.rect.center();
                                    ui.ctx().transform_layer_shapes(layer_id, TSTransform::from_translation(delta));
                                }
                                continue;
                            }

                            let mut entry_frame = Frame::new().begin(ui);
                            entry_contents(&mut entry_frame.content_ui, entry);
                            let entry_rect = entry_frame.allocate_space(ui).rect;
                            let interaction = ui.interact(entry_rect, entry_id, Sense::click_and_drag());

                            if interaction.clicked() {
                                state.selected_entry = entry_index;
                            }

                            if let Some(with) = interaction.dnd_release_payload::<usize>() {
                                replace = Some((entry_index, *with));
                            }

                            let fill = if entry_index == state.selected_entry {
                                ui.visuals().widgets.active.bg_fill
                            } else {
                                ui.visuals().disable(ui.visuals().widgets.inactive.bg_fill)
                            };

                            let dragged_id = ui.ctx().dragged_id();
                            let payload_id = DragAndDrop::payload::<usize>(ui.ctx()).map(|index| self.list_id.with(index));

                            let stroke = if payload_id == dragged_id && interaction.contains_pointer() {
                                ui.visuals().widgets.active.bg_stroke
                            } else {
                                ui.visuals().widgets.inactive.bg_stroke
                            };

                            entry_frame.frame.fill = fill;
                            entry_frame.frame.stroke = stroke;

                            entry_frame.paint(ui);
                        }

                        if let Some((swap, with)) = replace {
                            if swap > with {
                                entries[with..=swap].rotate_left(1);
                            } else {
                                entries[swap..=with].rotate_right(1);
                            }
                            state.selected_entry = swap;
                        }
                    });
                });
            });

        if entries.is_empty() {
            return None;
        }

        let active_index = state.selected_entry;
        state.store(ui.ctx(), persistent_id);
        Some(active_index)
    }
}
