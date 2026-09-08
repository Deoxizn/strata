// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn rename_field(view: &BrowserView) -> Option<gtk::Entry> {
    view.state
        .active_rename
        .borrow()
        .as_ref()
        .map(|active| active.field.clone())
        .or_else(|| view.state.mode_views.borrow().active_rename_field())
}

fn with_new_folder(mode: BrowserMode, run: impl FnOnce(&BrowserView, &std::path::Path)) {
    let fixture = tempfile::tempdir().expect("fixture");
    let view = BrowserView::new(
        Rc::new(crate::adapters::LocalFileSource),
        PeekBehavior::default(),
    );
    view.set_operation_provider(Rc::new(crate::adapters::LocalOperationProvider));
    view.set_view_mode(mode);
    let window = gtk::Window::builder()
        .child(&view.widget())
        .default_width(800)
        .default_height(600)
        .build();
    view.install_inline_edit_dismissal(&window);
    window.present();
    view.browser().navigate(Location::local(fixture.path()));
    wait_until(|| {
        view.browser()
            .column_snapshot(0)
            .is_some_and(|snapshot| !snapshot.loading)
    });
    view.create_new_folder();
    run(&view, fixture.path());
    view.browser().clear_observer();
    window.destroy();
}

#[test]
fn new_folders_start_a_fully_selected_rename_and_invalid_names_keep_the_created_folder() {
    gtk_test(
        "ui::browser::inline_edit::tests::folders::new_folders_start_a_fully_selected_rename_and_invalid_names_keep_the_created_folder",
        || {
            for mode in [BrowserMode::Columns, BrowserMode::List, BrowserMode::Icons] {
                for name in ["", " \u{2003}\u{00a0}", ".", "..", "nested/name"] {
                    with_new_folder(mode, |view, path| {
                        wait_until(|| rename_field(view).is_some());
                        let field = rename_field(view).expect("folder rename field");
                        assert!(path.join("new folder").is_dir());
                        assert_eq!(field.text(), "new folder");
                        assert_eq!(field.selection_bounds(), Some((0, 10)));
                        field.set_text(name);
                        field.emit_activate();
                        wait_until(|| !view.rename_is_active());
                        assert!(path.join("new folder").is_dir());
                        assert_eq!(std::fs::read_dir(path).expect("listing").count(), 1);
                    });
                }
            }
        },
    );
}

#[test]
fn navigating_before_creation_completes_does_not_open_an_editor_in_the_new_location() {
    gtk_test(
        "ui::browser::inline_edit::tests::folders::navigating_before_creation_completes_does_not_open_an_editor_in_the_new_location",
        || {
            for mode in [BrowserMode::Columns, BrowserMode::List, BrowserMode::Icons] {
                with_new_folder(mode, |view, path| {
                    let elsewhere = path.join("elsewhere");
                    std::fs::create_dir(&elsewhere).expect("navigation target");
                    view.browser().navigate(Location::local(&elsewhere));
                    wait_until(|| path.join("new folder").is_dir());
                    wait_until(|| {
                        view.browser().column_snapshot(0).is_some_and(|snapshot| {
                            !snapshot.loading && snapshot.location == Location::local(&elsewhere)
                        })
                    });
                    assert!(!view.rename_is_active());
                    assert!(!view.new_entry_is_active());
                    assert!(!elsewhere.join("new folder").exists());
                });
            }
        },
    );
}

#[test]
fn cancelling_before_creation_completes_does_not_open_a_late_editor_or_delete_the_folder() {
    gtk_test(
        "ui::browser::inline_edit::tests::folders::cancelling_before_creation_completes_does_not_open_a_late_editor_or_delete_the_folder",
        || {
            for mode in [BrowserMode::Columns, BrowserMode::List, BrowserMode::Icons] {
                with_new_folder(mode, |view, path| {
                    assert!(view.cancel_new_entry());
                    wait_until(|| path.join("new folder").is_dir());
                    wait_until(|| {
                        view.browser()
                            .column_snapshot(0)
                            .is_some_and(|snapshot| snapshot.count == 1)
                    });
                    while gtk::glib::MainContext::default().pending() {
                        gtk::glib::MainContext::default().iteration(false);
                    }
                    assert!(!view.rename_is_active());
                    assert!(!view.new_entry_is_active());
                    assert!(path.join("new folder").is_dir());
                });
            }
        },
    );
}
