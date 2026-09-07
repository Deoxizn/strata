// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::{
    test_support::gtk_test,
    ui::{
        browser::{BrowserView, PeekBehavior},
        browser_modes::BrowserMode,
    },
};
use std::time::{Duration, Instant};

#[test]
fn an_empty_name_is_not_flagged_as_an_error() {
    assert!(basename_field_error("bad/name").is_some());
    assert!(
        basename_field_error("").is_none(),
        "an empty field is the normal starting state, not a user mistake"
    );
}

#[test]
fn inline_rename_selects_the_stem_but_keeps_the_extension() {
    assert_eq!(rename_stem_end("report.txt"), 6);
    assert_eq!(rename_stem_end("archive.tar.gz"), 11);
    assert_eq!(rename_stem_end("README"), 6);
    assert_eq!(rename_stem_end(".gitignore"), 10);
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "rename fixture did not settle");
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn icon_card_bounds(root: &gtk::Widget) -> Vec<(i32, i32, i32, i32)> {
    fn visit(widget: &gtk::Widget, root: &gtk::Widget, bounds: &mut Vec<(i32, i32, i32, i32)>) {
        if widget.has_css_class("icons-card")
            && widget.is_mapped()
            && let Some(rect) = widget.compute_bounds(root)
        {
            bounds.push((
                rect.x().round() as i32,
                rect.y().round() as i32,
                rect.width().round() as i32,
                rect.height().round() as i32,
            ));
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            visit(&current, root, bounds);
        }
    }

    let mut bounds = Vec::new();
    visit(root, root, &mut bounds);
    bounds.sort_unstable();
    bounds
}

#[test]
fn clicking_away_from_a_new_folder_submits_and_stays_responsive() {
    gtk_test(
        "ui::browser::inline_edit::tests::clicking_away_from_a_new_folder_submits_and_stays_responsive",
        || {
            let fixture = tempfile::tempdir().expect("directory fixture");
            let path = fixture.path().to_path_buf();
            std::fs::write(fixture.path().join("alpha.txt"), b"alpha").expect("fixture file");
            std::fs::create_dir(fixture.path().join("Child")).expect("fixture folder");
            let view = BrowserView::new(
                Rc::new(crate::adapters::LocalFileSource),
                PeekBehavior::default(),
            );
            view.set_operation_provider(Rc::new(crate::adapters::LocalOperationProvider));
            view.set_view_mode(BrowserMode::Columns);
            let window = gtk::Window::builder()
                .child(&view.widget())
                .default_width(800)
                .default_height(600)
                .build();
            window.present();
            let browser = view.browser();
            browser.navigate(Location::local(&path));
            wait_until(|| {
                browser
                    .column_snapshot(0)
                    .is_some_and(|snapshot| !snapshot.loading)
            });
            wait_until(|| window.is_visible());
            eprintln!("STEP: column loaded");

            view.state.begin_new_entry(0, Location::local(&path), true);
            let field = view
                .state
                .active_new_entry
                .borrow()
                .as_ref()
                .map(|active| active.field.clone())
                .expect("a new entry field is open");
            field.set_text("brand-new-folder");
            eprintln!("STEP: field ready");

            let controllers = field.observe_controllers();
            let leave = (0..controllers.n_items())
                .filter_map(|index| {
                    controllers
                        .item(index)
                        .and_downcast::<gtk::EventControllerFocus>()
                })
                .next()
                .expect("focus controller on the new entry field");
            leave.emit_by_name::<()>("leave", &[]);
            eprintln!("STEP: focus-leave emitted");
            browser.select(0, 1);
            view.state
                .columns
                .borrow()
                .first()
                .expect("column")
                .list
                .grab_focus();
            eprintln!("STEP: click selection applied");
            wait_until(|| view.state.active_new_entry.borrow().is_none());
            eprintln!("STEP: new entry no longer active");
            wait_until(|| path.join("brand-new-folder").is_dir());
            eprintln!("STEP: folder created on disk");
            wait_until(|| {
                browser
                    .column_snapshot(0)
                    .is_some_and(|snapshot| snapshot.count >= 3)
                    && view.widget().is_visible()
            });
            eprintln!("STEP: folder visible in column");
            browser.clear_observer();
            window.destroy();
        },
    );
}

#[test]
fn list_mode_new_folder_clicking_away_submits_and_stays_responsive() {
    gtk_test(
        "ui::browser::inline_edit::tests::list_mode_new_folder_clicking_away_submits_and_stays_responsive",
        || {
            let fixture = tempfile::tempdir().expect("directory fixture");
            let path = fixture.path().to_path_buf();
            std::fs::write(fixture.path().join("alpha.txt"), b"alpha").expect("fixture file");
            std::fs::create_dir(fixture.path().join("Child")).expect("fixture folder");
            let view = BrowserView::new(
                Rc::new(crate::adapters::LocalFileSource),
                PeekBehavior::default(),
            );
            view.set_operation_provider(Rc::new(crate::adapters::LocalOperationProvider));
            view.set_view_mode(BrowserMode::List);
            let window = gtk::Window::builder()
                .child(&view.widget())
                .default_width(800)
                .default_height(600)
                .build();
            window.present();
            let browser = view.browser();
            browser.navigate(Location::local(&path));
            wait_until(|| {
                browser
                    .column_snapshot(0)
                    .is_some_and(|snapshot| !snapshot.loading)
            });
            wait_until(|| window.is_visible());
            eprintln!("STEP: list loaded");

            view.state.begin_new_entry(0, Location::local(&path), true);
            let field = loop {
                if let Some(field) = view.state.mode_views.borrow().active_new_entry_field() {
                    break field;
                }
                glib::MainContext::default().iteration(false);
            };
            field.set_text("brand-new-folder");
            eprintln!("STEP: field ready");
            let list = field
                .ancestor(gtk::ListView::static_type())
                .and_downcast::<gtk::ListView>()
                .expect("list view holds the new entry field");

            let controllers = field.observe_controllers();
            let leave = (0..controllers.n_items())
                .filter_map(|index| {
                    controllers
                        .item(index)
                        .and_downcast::<gtk::EventControllerFocus>()
                })
                .next()
                .expect("focus controller on the list new entry field");
            leave.emit_by_name::<()>("leave", &[]);
            eprintln!("STEP: focus-leave emitted");
            browser.select(0, 1);
            list.grab_focus();
            eprintln!("STEP: click selection applied");
            wait_until(|| !view.state.mode_views.borrow().new_entry_is_active());
            eprintln!("STEP: new entry no longer active");
            wait_until(|| path.join("brand-new-folder").is_dir());
            eprintln!("STEP: folder created on disk");
            wait_until(|| {
                browser
                    .column_snapshot(0)
                    .is_some_and(|snapshot| snapshot.count >= 3)
                    && view.widget().is_visible()
            });
            eprintln!("STEP: folder visible in list");
            browser.clear_observer();
            window.destroy();
        },
    );
}

#[test]
fn columns_rename_hides_and_restores_the_size_badge() {
    gtk_test(
        "ui::browser::inline_edit::tests::columns_rename_hides_and_restores_the_size_badge",
        || {
            let fixture = tempfile::tempdir().expect("directory fixture");
            let name = "synthetic-quarterly-report-with-a-very-long-descriptive-basename-2026.txt";
            std::fs::write(fixture.path().join(name), b"body").expect("fixture file");

            let view = BrowserView::new(
                Rc::new(crate::adapters::LocalFileSource),
                PeekBehavior::default(),
            );
            view.set_view_mode(BrowserMode::Columns);
            let window = gtk::Window::builder()
                .child(&view.widget())
                .default_width(420)
                .default_height(300)
                .build();
            window.present();
            let browser = view.browser();
            browser.navigate(Location::local(fixture.path()));
            wait_until(|| {
                browser
                    .column_snapshot(0)
                    .is_some_and(|snapshot| !snapshot.loading && snapshot.count == 1)
            });
            browser.select(0, 0);
            wait_until(|| view.state.begin_rename());
            let size = view
                .state
                .active_rename
                .borrow()
                .as_ref()
                .map(|rename| rename.size.clone())
                .expect("a Columns rename is open");

            wait_until(|| !size.label().is_empty());
            assert!(
                !size.is_visible(),
                "the badge must stay hidden while renaming"
            );
            assert!(view.state.cancel_rename());
            assert!(size.is_visible(), "cancelling must restore the size badge");

            browser.clear_observer();
            window.destroy();
        },
    );
}

#[test]
fn submitting_an_invalid_rename_flags_the_field_in_every_view_mode() {
    gtk_test(
        "ui::browser::inline_edit::tests::submitting_an_invalid_rename_flags_the_field_in_every_view_mode",
        || {
            let fixture = tempfile::tempdir().expect("directory fixture");
            let file = fixture.path().join("notes.txt");
            std::fs::write(&file, b"body").expect("fixture file");
            for index in 0..5 {
                std::fs::write(fixture.path().join(format!("sample-{index}.txt")), b"body")
                    .expect("fixture file");
            }

            for mode in [BrowserMode::Columns, BrowserMode::List, BrowserMode::Icons] {
                let view = BrowserView::new(
                    Rc::new(crate::adapters::LocalFileSource),
                    PeekBehavior::default(),
                );
                view.set_view_mode(mode);
                let window = gtk::Window::builder()
                    .child(&view.widget())
                    .default_width(800)
                    .default_height(600)
                    .build();
                window.present();
                let browser = view.browser();
                browser.navigate(Location::local(fixture.path()));
                wait_until(|| {
                    browser
                        .column_snapshot(0)
                        .is_some_and(|snapshot| !snapshot.loading && snapshot.count == 6)
                });
                let widget = view.widget();
                let bounds_before = (mode == BrowserMode::Icons).then(|| {
                    wait_until(|| {
                        let bounds = icon_card_bounds(&widget);
                        bounds.len() == 6
                            && bounds
                                .iter()
                                .all(|(_, _, width, height)| *width > 0 && *height > 0)
                    });
                    icon_card_bounds(&widget)
                });
                browser.select(0, 0);
                wait_until(|| view.state.begin_rename());
                let field = view
                    .state
                    .active_rename
                    .borrow()
                    .as_ref()
                    .map(|rename| rename.field.clone())
                    .or_else(|| view.state.mode_views.borrow().active_rename_field())
                    .expect("an inline rename field is open");

                if let Some(bounds_before) = bounds_before {
                    wait_until(|| field.is_mapped());
                    let deadline = Instant::now() + Duration::from_millis(100);
                    while Instant::now() < deadline {
                        glib::MainContext::default().iteration(false);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    assert_eq!(
                        icon_card_bounds(&widget),
                        bounds_before,
                        "opening the Icons rename field must not reflow the grid"
                    );
                }

                for (name, message) in [
                    ("", "Enter a name"),
                    ("bad/name", "Names cannot contain /"),
                    (".", "That name is reserved"),
                ] {
                    field.set_text(name);
                    field.emit_by_name::<()>("activate", &[]);
                    assert!(
                        field.has_css_class("error"),
                        "{mode:?} did not flag {name:?}"
                    );
                    assert_eq!(
                        field.tooltip_text().as_deref(),
                        Some(message),
                        "{mode:?} explains why {name:?} was rejected"
                    );
                    assert!(field.is_sensitive(), "{mode:?} left the field disabled");
                }

                assert!(file.is_file(), "{mode:?} left the entry untouched");
                browser.clear_observer();
                window.destroy();
            }
        },
    );
}
