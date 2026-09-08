// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::services::CreateDirectoryRequest;

fn create(parent: &Path, name: &str, unique_name: bool, count: usize) -> Vec<OperationEvent> {
    let _guard = ASYNC_MAIN_CONTEXT_DEFAULT.lock();
    let context = glib::MainContext::default();
    let _owner = context.acquire().expect("exclusive main context");
    let events = Rc::new(RefCell::new(Vec::new()));
    let _loads: Vec<_> = (0..count)
        .map(|id| {
            let received = events.clone();
            LocalOperationProvider.create_directory(
                CreateDirectoryRequest {
                    id: OperationRequestId(id as u64),
                    parent: Location::local(parent),
                    name: name.to_owned(),
                    unique_name,
                },
                Rc::new(move |event| received.borrow_mut().push(event)),
            )
        })
        .collect();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while events.borrow().len() < count {
        assert!(
            std::time::Instant::now() < deadline,
            "directory creation did not finish"
        );
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    events.take()
}

#[test]
fn unique_directory_creation_retries_atomic_collisions_without_overwriting() {
    let fixture = tempfile::tempdir().expect("fixture");
    fs::write(fixture.path().join("new folder"), b"keep").expect("existing file");
    std::os::unix::fs::symlink("missing", fixture.path().join("new folder (1)"))
        .expect("dangling link");
    fs::create_dir(fixture.path().join("new folder (2)")).expect("existing folder");
    let events = create(fixture.path(), "new folder", true, 2);
    let created: HashSet<_> = events
        .into_iter()
        .map(|event| match event {
            OperationEvent::DirectoryCreated { location, .. } => location,
            other => panic!("unexpected creation event: {other:?}"),
        })
        .collect();
    assert_eq!(
        created,
        HashSet::from([
            Location::local(fixture.path().join("new folder (3)")),
            Location::local(fixture.path().join("new folder (4)")),
        ])
    );
    assert_eq!(
        fs::read(fixture.path().join("new folder")).expect("existing contents"),
        b"keep"
    );
    assert!(fixture.path().join("new folder (1)").is_symlink());
    assert!(fixture.path().join("new folder (2)").is_dir());
    assert!(fixture.path().join("new folder (3)").is_dir());
    assert!(fixture.path().join("new folder (4)").is_dir());
}

#[test]
fn unique_directory_creation_uses_the_first_available_name() {
    let fixture = tempfile::tempdir().expect("fixture");
    fs::create_dir(fixture.path().join("new folder (1)")).expect("numbered folder");
    assert!(
        matches!(create(fixture.path(), "new folder", true, 1).as_slice(), [OperationEvent::DirectoryCreated { location, .. }] if location == &Location::local(fixture.path().join("new folder")))
    );
    fs::remove_dir(fixture.path().join("new folder (1)")).expect("release first suffix");
    fs::create_dir(fixture.path().join("new folder (2)")).expect("later suffix");
    assert!(
        matches!(create(fixture.path(), "new folder", true, 1).as_slice(), [OperationEvent::DirectoryCreated { location, .. }] if location == &Location::local(fixture.path().join("new folder (1)")))
    );
}

#[test]
fn exact_creation_still_reports_conflicts_and_unique_creation_rejects_invalid_names() {
    let fixture = tempfile::tempdir().expect("fixture");
    fs::create_dir(fixture.path().join("existing")).expect("existing folder");
    assert!(matches!(
        create(fixture.path(), "existing", false, 1).as_slice(),
        [OperationEvent::Failed { .. }]
    ));
    for name in ["", "   ", ".", "..", "nested/name", "nul\0name"] {
        assert!(matches!(
            create(fixture.path(), name, true, 1).as_slice(),
            [OperationEvent::Failed { .. }]
        ));
    }
    assert_eq!(fs::read_dir(fixture.path()).expect("listing").count(), 1);
}
