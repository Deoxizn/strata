# SPDX-License-Identifier: GPL-3.0-or-later
"""Finder-style folder creation and rename finalization with real input."""

import pytest

from harness.modes import ALL_MODES


def begin_folder_edit(strata, new):
    strata.select_entry("readme.md")
    if new:
        strata.keyboard.press("ctrl+shift+n")
        original = "new folder"
    else:
        strata.select_entry_with_keyboard("archive")
        strata.keyboard.press("F2")
        original = "archive"
    field = strata.editable_field()
    strata.wait(lambda: field.text == original, "the original name to be selected")
    assert strata.fixture.path(original).is_dir()
    return field, original


def wait_for_edit_closed(strata):
    strata.wait(
        lambda: strata.window.find(role="text", states={"editable"}) is None,
        "the folder name editor to close",
    )
    assert "Gtk-CRITICAL" not in strata.application.log()


@pytest.mark.parametrize("mode", ALL_MODES)
@pytest.mark.parametrize("via_menu", [False, True])
def test_new_folder_exists_before_typing_and_backspace_clears_its_selected_name(strata, mode, via_menu):
    strata.select_entry("readme.md")
    if via_menu:
        strata.pointer.right_click(strata.pane(), at=strata.background_point())
        strata.choose_menu_item("New Folder")
    else:
        strata.keyboard.press("ctrl+shift+n")
    field = strata.editable_field()
    strata.wait(lambda: field.text == "new folder", "the default name to appear")
    assert strata.fixture.path("new folder").is_dir()
    strata.keyboard.press("BackSpace")
    strata.wait(lambda: field.text == "", "one Backspace to clear the whole name")
    strata.keyboard.press("Return")
    wait_for_edit_closed(strata)
    strata.entry("new folder")
    assert strata.fixture.path("new folder").is_dir()


@pytest.mark.parametrize("mode", ALL_MODES)
def test_new_folder_uses_the_first_free_number_without_overwriting(strata, mode):
    strata.fixture.path("new folder").write_text("keep\n")
    strata.fixture.path("new folder (1)").symlink_to("missing")
    strata.fixture.path("new folder (2)").mkdir()
    strata.select_entry("readme.md")
    strata.keyboard.press("ctrl+shift+n")
    field = strata.editable_field()
    strata.wait(lambda: field.text == "new folder (3)", "the first available numbered name")
    assert strata.fixture.path("new folder (3)").is_dir()
    strata.keyboard.press("Escape")
    wait_for_edit_closed(strata)
    assert strata.fixture.path("new folder").read_text() == "keep\n"
    assert strata.fixture.path("new folder (1)").is_symlink()
    assert strata.fixture.path("new folder (2)").is_dir()
    assert strata.fixture.path("new folder (3)").is_dir()


@pytest.mark.parametrize("mode", ALL_MODES)
@pytest.mark.parametrize("new", [False, True], ids=["existing", "new"])
@pytest.mark.parametrize("target", ["file", "folder", "background", "sidebar", "tab", "enter"])
def test_leaving_a_valid_folder_name_commits_it(strata, mode, new, target):
    field, original = begin_folder_edit(strata, new)
    strata.fixture.path(original + "/marker.txt").write_text("keep\n")
    strata.keyboard.type_text("renamed.folder")
    strata.wait(lambda: field.text == "renamed.folder", "typing to replace the selected name")
    if target == "sidebar":
        strata.pointer.click(strata.sidebar_button("Home"))
    elif target == "background":
        strata.pointer.click(strata.pane(), at=strata.background_point())
    elif target in ("tab", "enter"):
        strata.keyboard.press("Tab" if target == "tab" else "Return")
    else:
        strata.pointer.click(strata.entry("todo.txt" if target == "file" else "documents"))
    strata.wait(lambda: strata.fixture.path("renamed.folder").is_dir(), "the rename on disk")
    wait_for_edit_closed(strata)
    assert not strata.fixture.path(original).exists()
    assert strata.fixture.path("renamed.folder/marker.txt").read_text() == "keep\n"
    if target == "sidebar":
        strata.wait_for_directory(strata.environment.home.name)
        assert not (strata.environment.home / "renamed.folder").exists()


@pytest.mark.parametrize("mode", ALL_MODES)
@pytest.mark.parametrize("new", [False, True], ids=["existing", "new"])
@pytest.mark.parametrize("name", ["", "   ", ".", "..", "bad/name", "/absolute"])
@pytest.mark.parametrize("action", ["enter", "click"])
def test_invalid_folder_names_retain_the_original(strata, mode, new, name, action):
    field, original = begin_folder_edit(strata, new)
    strata.keyboard.press("BackSpace")
    strata.keyboard.type_text(name)
    strata.wait(lambda: field.text == name, "the proposed name to appear")
    if action == "enter":
        strata.keyboard.press("Return")
    else:
        strata.pointer.click(strata.pane(), at=strata.background_point())
    wait_for_edit_closed(strata)
    strata.entry(original)
    assert strata.fixture.path(original).is_dir()
    assert not strata.fixture.path("bad").exists()


@pytest.mark.parametrize("mode", ALL_MODES)
@pytest.mark.parametrize("new", [False, True], ids=["existing", "new"])
def test_escape_preserves_the_original_folder_name(strata, mode, new):
    field, original = begin_folder_edit(strata, new)
    strata.keyboard.type_text("discarded")
    strata.wait(lambda: field.text == "discarded", "the proposed name to appear")
    strata.keyboard.press("Escape")
    wait_for_edit_closed(strata)
    strata.entry(original)
    assert strata.fixture.path(original).is_dir()
    assert not strata.fixture.path("discarded").exists()


@pytest.mark.parametrize("mode", ALL_MODES)
def test_new_folder_clears_a_filter_that_would_hide_its_editor(strata, mode):
    strata.select_entry("readme.md")
    strata.keyboard.press("ctrl+f")
    filter_field = strata.editable_field()
    strata.keyboard.type_text("no-matching-entry")
    strata.wait(lambda: filter_field.text == "no-matching-entry", "the filter query")
    strata.wait(lambda: strata.entry_names() == [], "the filter to hide existing entries")
    strata.keyboard.press("ctrl+shift+n")
    field = strata.editable_field()
    strata.wait(lambda: field.text == "new folder", "the visible new-folder editor")
    assert filter_field.text == ""
    assert strata.fixture.path("new folder").is_dir()
    strata.keyboard.press("Escape")
    strata.entry("new folder")


@pytest.mark.parametrize("mode", ALL_MODES)
def test_repeated_folder_renames_select_the_entire_dotted_name(strata, mode):
    original = "archive"
    for replacement in ["archive.v1", "archive.v2", "archive.v3"]:
        strata.select_entry("readme.md")
        strata.select_entry_with_keyboard(original)
        strata.keyboard.press("F2")
        field = strata.editable_field()
        strata.keyboard.type_text(replacement)
        strata.wait(lambda: field.text == replacement, "the entire old name to be replaced")
        strata.keyboard.press("Return")
        strata.wait(lambda: strata.fixture.path(replacement).is_dir(), "the renamed folder")
        strata.entry(replacement)
        assert not strata.fixture.path(original).exists()
        original = replacement
