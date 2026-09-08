// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::ui::browser_modes::{ActiveModeNewEntry, finish_mode_new_entry};

fn prompt() -> ActiveModeNewEntry {
    let stack = gtk::Stack::new();
    let view = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let field = gtk::Entry::new();
    view.append(&field);
    stack.add_named(&view, Some("content"));
    stack.add_named(&gtk::Label::new(Some("Empty")), Some("status"));
    ActiveModeNewEntry {
        is_directory: true,
        field,
        placeholder: Some(gtk::StringList::new(&[""])),
        stack: Some(stack),
        source_model: Some(gtk::StringList::new(&[])),
        view: view.upcast(),
    }
}

fn drain_idle() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

#[test]
fn deferred_new_entry_cleanup_preserves_a_reopened_prompt() {
    gtk_test(
        "ui::browser_modes::tests::new_entry::deferred_new_entry_cleanup_preserves_a_reopened_prompt",
        || {
            let active = prompt();
            let placeholder = active.placeholder.as_ref().expect("placeholder");
            let stack = active.stack.as_ref().expect("status stack");
            finish_mode_new_entry(&active);
            assert_eq!(placeholder.n_items(), 1, "teardown must wait for idle");
            placeholder.splice(0, 1, &[""]);
            active.field.set_text("replacement");
            active.view.add_css_class("creating-entry");
            drain_idle();
            assert_eq!(placeholder.n_items(), 1);
            assert_eq!(active.field.text(), "replacement");
            assert!(active.view.has_css_class("creating-entry"));
            assert_eq!(stack.visible_child_name().as_deref(), Some("content"));
        },
    );
}

#[test]
fn deferred_new_entry_cleanup_uses_the_current_listing() {
    gtk_test(
        "ui::browser_modes::tests::new_entry::deferred_new_entry_cleanup_uses_the_current_listing",
        || {
            let active = prompt();
            finish_mode_new_entry(&active);
            active
                .source_model
                .as_ref()
                .expect("listing")
                .append("dv\tcreated");
            drain_idle();
            assert_eq!(
                active.placeholder.as_ref().expect("placeholder").n_items(),
                0
            );
            assert_eq!(
                active
                    .stack
                    .as_ref()
                    .expect("status stack")
                    .visible_child_name()
                    .as_deref(),
                Some("content")
            );
        },
    );
}

#[test]
fn deferred_new_entry_cleanup_restores_an_empty_pane() {
    gtk_test(
        "ui::browser_modes::tests::new_entry::deferred_new_entry_cleanup_restores_an_empty_pane",
        || {
            let active = prompt();
            finish_mode_new_entry(&active);
            drain_idle();
            assert_eq!(
                active.placeholder.as_ref().expect("placeholder").n_items(),
                0
            );
            assert_eq!(
                active
                    .stack
                    .as_ref()
                    .expect("status stack")
                    .visible_child_name()
                    .as_deref(),
                Some("status")
            );
        },
    );
}

#[test]
fn deferred_new_entry_cleanup_does_not_require_a_status_stack() {
    gtk_test(
        "ui::browser_modes::tests::new_entry::deferred_new_entry_cleanup_does_not_require_a_status_stack",
        || {
            let mut active = prompt();
            active.stack = None;
            finish_mode_new_entry(&active);
            drain_idle();
            assert_eq!(
                active.placeholder.as_ref().expect("placeholder").n_items(),
                0
            );
        },
    );
}

#[test]
fn deferred_new_entry_cleanup_does_not_require_the_field_to_survive() {
    gtk_test(
        "ui::browser_modes::tests::new_entry::deferred_new_entry_cleanup_does_not_require_the_field_to_survive",
        || {
            let active = prompt();
            let placeholder = active.placeholder.as_ref().expect("placeholder").clone();
            let field = active.field.downgrade();
            finish_mode_new_entry(&active);
            drop(active);
            assert!(field.upgrade().is_none());
            drain_idle();
            assert_eq!(placeholder.n_items(), 0);
        },
    );
}
