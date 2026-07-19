//! Parity suite seed (RFC v2 §9): one behavior-spec-derived assertion set,
//! run against every backend available on this host. On Linux CI that is
//! `platform-mock` + `platform-linux`; the Windows leg will add
//! `platform-windows` when its Dir impl lands (R1). The suite living as a
//! generic function over `&dyn Dir` is the point — a backend passes or it
//! doesn't, with no backend-specific test text to drift.

use std::ffi::OsStr;

use platform::error::ErrorKind;
use platform::fs::{Dir, FileType, OpenOptions};

/// The shared assertions. Grows with `docs/behavior/fs.md`.
fn assert_fs_behavior(root: &dyn Dir) {
    // create → write → read round-trip, byte-faithful
    let mut f = root
        .open(OsStr::new("a.bin"), &OpenOptions::create_truncate())
        .expect("create");
    f.write(b"one \xff two").expect("write");
    drop(f);
    let mut f = root.open(OsStr::new("a.bin"), &OpenOptions::read()).expect("open");
    let mut buf = [0u8; 64];
    let n = f.read(&mut buf).expect("read");
    assert_eq!(&buf[..n], b"one \xff two");

    // metadata: type and length agree with what was written
    let md = root.metadata(OsStr::new("a.bin")).expect("metadata");
    assert_eq!(md.file_type, FileType::File);
    assert_eq!(md.len, 9);

    // missing entries are NotFound with path context
    let e = root.metadata(OsStr::new("missing")).expect_err("must fail");
    assert_eq!(e.kind, ErrorKind::NotFound);
    assert!(e.path.is_some());

    // create_new refuses to clobber
    let e = root
        .open(
            OsStr::new("a.bin"),
            &OpenOptions { write: true, create_new: true, ..Default::default() },
        )
        .err()
        .expect("create_new over existing must fail");
    assert_eq!(e.kind, ErrorKind::AlreadyExists);

    // directories: create, list through a child capability, refuse
    // non-empty removal, then remove bottom-up
    root.create_dir(OsStr::new("d")).expect("mkdir");
    let d = root.open_dir(OsStr::new("d")).expect("open_dir");
    d.open(OsStr::new("inner"), &OpenOptions::create_truncate()).expect("create");
    let names: Vec<_> = d.read_dir().expect("read_dir").into_iter().map(|e| e.name).collect();
    assert!(names.contains(&OsStr::new("inner").to_os_string()));
    let e = root.remove_dir(OsStr::new("d")).expect_err("non-empty removal must fail");
    assert_eq!(e.kind, ErrorKind::DirectoryNotEmpty);
    d.remove_file(OsStr::new("inner")).expect("rm inner");
    root.remove_dir(OsStr::new("d")).expect("rmdir");
    root.remove_file(OsStr::new("a.bin")).expect("rm");
}

#[test]
fn mock_backend_conforms() {
    assert_fs_behavior(&platform_mock::MockDir::root());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_backend_conforms() {
    let tmp = std::env::temp_dir().join(format!("rustils-parity-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mk tempdir");
    let root = platform_linux::LinuxDir::open_ambient(&tmp).expect("open ambient");
    assert_fs_behavior(&root);
    std::fs::remove_dir_all(&tmp).ok();
}
