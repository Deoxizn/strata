// SPDX-License-Identifier: GPL-3.0-or-later

use crate::model::{FileEntry, Location};
use crate::services::validate_basename;
use crate::ui::browser::ViewState;
use crate::ui::browser::paths::is_trash_location;
use crate::ui::browser_modes::BrowserMode;
use gtk::prelude::*;
use std::rc::Rc;

pub(super) struct ActiveRename {
    pub(super) entry: FileEntry,
    pub(super) field: gtk::Entry,
    pub(super) label: gtk::Label,
    pub(super) spacer: gtk::Box,
    pub(super) size: gtk::Label,
}

pub(super) struct PendingFolderRename {
    depth: usize,
    parent: Location,
}

pub(super) struct ActiveNewEntry {
    pub(super) location: Location,
    pub(super) is_directory: bool,
    pub(super) row: gtk::Box,
    pub(super) field: gtk::Entry,
}

/// Whether a name currently typed into a field should be visually flagged as
/// an error. An empty name is left unstyled: it's the normal starting state
/// (opening, cancelling, or succeeding a prompt all clear the field) rather
/// than a mistake the user made, even though it still can't be submitted.
/// Kept separate from `update_basename_validation` so it can be unit tested
/// without constructing a real GTK widget.
fn basename_field_error(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        None
    } else {
        validate_basename(name).err()
    }
}

/// Validates a name field live as it changes, including the programmatic
/// clears that happen when a prompt opens, cancels, or succeeds.
pub(in crate::ui) fn update_basename_validation(field: &gtk::Entry) -> bool {
    let text = field.text();
    match basename_field_error(text.as_str()) {
        None => {
            field.remove_css_class("error");
            field.set_tooltip_text(None);
            !text.is_empty()
        }
        Some(message) => {
            field.add_css_class("error");
            field.set_tooltip_text(Some(message));
            false
        }
    }
}

pub(in crate::ui) fn rename_stem_end(name: &str) -> i32 {
    let end = name
        .rfind('.')
        .filter(|position| *position > 0)
        .unwrap_or(name.len());
    name[..end].chars().count().min(i32::MAX as usize) as i32
}

impl super::BrowserView {
    pub(in crate::ui) fn install_inline_edit_dismissal(&self, root: &impl IsA<gtk::Widget>) {
        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&self.state);
        click.connect_pressed(move |gesture, _, x, y| {
            let Some(state) = weak.upgrade() else { return };
            state.pending_folder_rename.take();
            let target = gesture
                .widget()
                .and_then(|root| root.pick(x, y, gtk::PickFlags::DEFAULT));
            let rename = state
                .active_rename
                .borrow()
                .as_ref()
                .map(|active| (active.field.clone(), active.entry.is_directory()))
                .or_else(|| state.mode_views.borrow().active_rename_target());
            if let Some((field, directory)) = rename
                && !target
                    .as_ref()
                    .is_some_and(|target| target == &field || target.is_ancestor(&field))
            {
                if directory {
                    state.submit_rename(&field);
                } else {
                    state.cancel_rename();
                }
            }
            let field = state
                .active_new_entry
                .borrow()
                .as_ref()
                .map(|active| active.field.clone())
                .or_else(|| state.mode_views.borrow().active_new_entry_field());
            let Some(field) = field else { return };
            let inside = target
                .as_ref()
                .is_some_and(|target| target == &field || target.is_ancestor(&field));
            if !inside {
                state.cancel_new_entry_for_field(&field);
                state.mode_views.borrow().cancel_new_entry();
            }
        });
        root.add_controller(click);
    }
}

pub(in crate::ui) fn queue_folder_rename(
    browser: &Rc<crate::app::Browser>,
    entry: FileEntry,
    name: String,
) {
    if name == entry.display_name || validate_basename(&name).is_err() {
        return;
    }
    // A rename can synchronously refresh models; dispatch after GTK's focus walk.
    let browser = Rc::downgrade(browser);
    gtk::glib::idle_add_local_once(move || {
        if let Some(browser) = browser.upgrade() {
            browser.rename(entry, name);
        }
    });
}

impl ViewState {
    pub(super) fn rename_created_folder(self: &Rc<Self>, location: &Location) {
        let Some(pending) = self
            .pending_folder_rename
            .borrow()
            .clone()
            .filter(|pending| Some(pending.parent.clone()) == location.parent())
        else {
            return;
        };
        let location = location.clone();
        let weak = Rc::downgrade(self);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let selected = std::cell::Cell::new(false);
        // Wait for the refreshed listing and the virtualized row to be allocated.
        self.overlay.add_tick_callback(move |_, _| {
            let Some(state) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if !state
                .pending_folder_rename
                .borrow()
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, &pending))
            {
                return gtk::glib::ControlFlow::Break;
            }
            if std::time::Instant::now() >= deadline
                || state.browser.location_at(pending.depth).as_ref() != Some(&pending.parent)
            {
                state.pending_folder_rename.take();
                return gtk::glib::ControlFlow::Break;
            }
            let Some(snapshot) = state
                .browser
                .column_snapshot(pending.depth)
                .filter(|snapshot| !snapshot.loading)
            else {
                return gtk::glib::ControlFlow::Continue;
            };
            let position = state
                .browser
                .with_entries(pending.depth, 0..snapshot.count, |entries| {
                    entries.iter().position(|entry| entry.location == location)
                })
                .flatten();
            if let Some(position) = position {
                if !selected.replace(true) {
                    state.browser.select(pending.depth, position);
                } else if let Some(entry) = state.browser.entry_at(pending.depth, position)
                    && state.begin_rename_item(pending.depth, position, entry)
                {
                    state.pending_folder_rename.take();
                    return gtk::glib::ControlFlow::Break;
                }
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    pub(super) fn finish_folder_rename_for_field(self: &Rc<Self>, field: &gtk::Entry) {
        if self
            .active_rename
            .borrow()
            .as_ref()
            .is_some_and(|active| active.field == *field && active.entry.is_directory())
        {
            self.submit_rename(field);
        }
    }

    pub(super) fn cancel_new_entry_for_field(&self, field: &gtk::Entry) {
        if self
            .active_new_entry
            .borrow()
            .as_ref()
            .is_some_and(|active| active.field == *field)
        {
            self.cancel_new_entry();
        }
    }

    pub(super) fn begin_new_entry(
        self: &Rc<Self>,
        depth: usize,
        location: Location,
        is_directory: bool,
    ) {
        if is_trash_location(&location) {
            return;
        }
        if is_directory {
            self.cancel_new_entry();
            self.mode_views.borrow().cancel_new_entry();
            self.cancel_rename();
            if let Some(column) = self.columns.borrow().get(depth) {
                column.filter_entry.set_text("");
            }
            self.mode_views.borrow().clear_filter(depth);
            self.pending_folder_rename
                .replace(Some(Rc::new(PendingFolderRename {
                    depth,
                    parent: location.clone(),
                })));
            self.browser.create_new_folder(location);
            return;
        }
        if self.mode_views.borrow().mode() != BrowserMode::Columns {
            self.cancel_new_entry();
            self.mode_views
                .borrow()
                .begin_new_entry(depth, is_directory);
            return;
        }
        self.cancel_new_entry();
        self.cancel_rename();
        let columns = self.columns.borrow();
        let Some(column) = columns.get(depth) else {
            return;
        };
        let icon_name = if is_directory {
            crate::assets::icons::FOLDER
        } else {
            crate::assets::icons::DOCUMENTS
        };
        crate::assets::set_primary_icon(&column.new_entry_icon, icon_name);
        column.new_entry_entry.set_text("");
        column.new_entry_entry.remove_css_class("error");
        column.new_entry_entry.set_tooltip_text(None);
        column.new_entry_row.set_visible(true);
        self.active_new_entry.replace(Some(ActiveNewEntry {
            location,
            is_directory,
            row: column.new_entry_row.clone(),
            field: column.new_entry_entry.clone(),
        }));
        column.new_entry_entry.grab_focus();
    }

    pub(super) fn submit_new_entry(self: &Rc<Self>, field: &gtk::Entry) {
        if !self
            .active_new_entry
            .borrow()
            .as_ref()
            .is_some_and(|active| active.field == *field)
        {
            return;
        }
        let name = field.text().to_string();
        if name.trim().is_empty() {
            self.cancel_new_entry();
            return;
        }
        if !update_basename_validation(field) {
            field.grab_focus();
            return;
        }
        let Some(active) = self.active_new_entry.take() else {
            return;
        };
        active.row.set_visible(false);
        field.set_text("");
        if active.is_directory {
            self.browser.create_directory(active.location, name);
        } else {
            self.browser.create_file(active.location, name);
        }
    }

    pub(super) fn cancel_new_entry(&self) -> bool {
        let pending = self.pending_folder_rename.take().is_some();
        let Some(active) = self.active_new_entry.take() else {
            return pending;
        };
        active.field.set_text("");
        active.field.remove_css_class("error");
        active.field.set_tooltip_text(None);
        active.row.set_visible(false);
        true
    }

    pub(super) fn begin_rename(self: &Rc<Self>) -> bool {
        self.cancel_new_entry();
        self.sync_mode_selection();
        let Some((depth, source_position, entry)) = self.browser.rename_item() else {
            return false;
        };
        self.begin_rename_item(depth, source_position, entry)
    }

    fn begin_rename_item(
        self: &Rc<Self>,
        depth: usize,
        source_position: usize,
        entry: FileEntry,
    ) -> bool {
        if is_trash_location(&entry.location) {
            return false;
        }
        if self.mode_views.borrow().mode() != BrowserMode::Columns {
            return self
                .mode_views
                .borrow()
                .begin_rename(depth, source_position, &entry);
        }
        self.cancel_rename();
        let columns = self.columns.borrow();
        let Some(column) = columns.get(depth) else {
            return false;
        };
        let Some(filtered_position) = column.map.view_position(source_position) else {
            return false;
        };
        let row = column.bound_rows.borrow().iter().find_map(|bound| {
            let item = bound.item.upgrade()?;
            (item.position() == filtered_position).then(|| bound.row.upgrade())?
        });
        let Some(row) = row else {
            return false;
        };
        if !row.is_mapped() || row.width() <= 0 || column.presentation.stack.is_transition_running()
        {
            return false;
        }
        let Some(icon) = row.first_child() else {
            return false;
        };
        let Some(middle) = icon.next_sibling().and_downcast::<gtk::Overlay>() else {
            return false;
        };
        let Some(editor) = middle
            .child()
            .and_then(|content| content.first_child())
            .and_downcast::<gtk::Box>()
        else {
            return false;
        };
        let Some(label) = editor.first_child().and_downcast::<gtk::Label>() else {
            return false;
        };
        let Some(field) = label.next_sibling().and_downcast::<gtk::Entry>() else {
            return false;
        };
        let Some(spacer) = field.next_sibling().and_downcast::<gtk::Box>() else {
            return false;
        };
        let Some(size) = middle.last_child().and_downcast::<gtk::Label>() else {
            return false;
        };
        super::prepare_collection_inline_edit(column.list.upcast_ref(), filtered_position);
        field.remove_css_class("error");
        field.set_tooltip_text(None);
        field.set_sensitive(true);
        field.set_text(&entry.display_name);
        label.set_visible(false);
        spacer.set_visible(false);
        size.set_visible(false);
        field.set_visible(true);
        field.grab_focus();
        field.select_region(
            0,
            if entry.is_directory() {
                -1
            } else {
                rename_stem_end(&entry.display_name)
            },
        );
        self.active_rename.replace(Some(ActiveRename {
            entry,
            field,
            label,
            spacer,
            size,
        }));
        true
    }

    pub(super) fn cancel_rename(&self) -> bool {
        if self.mode_views.borrow().cancel_rename() {
            return true;
        }
        let Some(rename) = self.active_rename.take() else {
            return false;
        };
        rename.field.remove_css_class("error");
        rename.field.set_tooltip_text(None);
        rename.field.set_visible(false);
        rename.field.set_sensitive(true);
        rename.label.set_visible(true);
        rename.spacer.set_visible(true);
        rename.size.set_visible(!rename.size.label().is_empty());
        true
    }

    pub(super) fn submit_rename(self: &Rc<Self>, field: &gtk::Entry) {
        if self
            .mode_views
            .borrow()
            .active_rename_target()
            .is_some_and(|(active, _)| active == *field)
        {
            self.mode_views.borrow().submit_rename(field);
            return;
        }
        // `Browser::rename` rejects an invalid basename by emitting `RenameFailed` before it
        // returns, and that handler reads `active_rename` to flag the field, so the borrow
        // taken to read the entry must be released first.
        let entry = {
            let active = self.active_rename.borrow();
            let Some(rename) = active.as_ref().filter(|rename| rename.field == *field) else {
                return;
            };
            rename.entry.clone()
        };
        let new_name = field.text().to_string();
        if entry.is_directory() {
            self.cancel_rename();
            queue_folder_rename(&self.browser, entry, new_name);
            return;
        }
        if new_name == entry.display_name {
            self.cancel_rename();
            self.browser.focus_active();
            return;
        }
        field.remove_css_class("error");
        field.set_tooltip_text(None);
        field.set_sensitive(false);
        self.browser.rename(entry, new_name);
    }
}

#[cfg(test)]
mod tests;
