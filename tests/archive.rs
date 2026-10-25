mod support;

use sdkman_contrib::archive::{ArchiveEntry, ArchiveFormat, list_entries};
use support::archives::{write_tar_gz, write_zip};

fn entry(components: &[&str], is_dir: bool, executable: bool) -> ArchiveEntry {
    ArchiveEntry {
        components: components.iter().map(|c| (*c).to_owned()).collect(),
        is_dir,
        executable,
    }
}

#[test]
fn reads_zip_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.zip");
    write_zip(
        &path,
        &[
            ("jpx-1.2.0/", 0o755),
            ("jpx-1.2.0/bin/jpx", 0o755),
            ("jpx-1.2.0/README", 0o644),
        ],
    );
    assert_eq!(
        list_entries(&path, ArchiveFormat::Zip).unwrap(),
        vec![
            entry(&["jpx-1.2.0"], true, false),
            entry(&["jpx-1.2.0", "bin", "jpx"], false, true),
            entry(&["jpx-1.2.0", "README"], false, false),
        ]
    );
}

#[test]
fn reads_tar_gz_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.tar.gz");
    write_tar_gz(
        &path,
        &[
            ("jpx-1.2.0/", 0o755),
            ("jpx-1.2.0/bin/jpx", 0o755),
            ("jpx-1.2.0/README", 0o644),
        ],
    );
    assert_eq!(
        list_entries(&path, ArchiveFormat::TarGz).unwrap(),
        vec![
            entry(&["jpx-1.2.0"], true, false),
            entry(&["jpx-1.2.0", "bin", "jpx"], false, true),
            entry(&["jpx-1.2.0", "README"], false, false),
        ]
    );
}

#[test]
fn drops_leading_dot_and_empty_components() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.tar.gz");
    write_tar_gz(&path, &[("./jpx-1.2.0//bin/jpx", 0o755)]);
    assert_eq!(
        list_entries(&path, ArchiveFormat::TarGz).unwrap(),
        vec![entry(&["jpx-1.2.0", "bin", "jpx"], false, true)]
    );
}

#[test]
fn skips_pax_global_header() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.tar.gz");
    let encoder = flate2::write::GzEncoder::new(
        std::fs::File::create(&path).unwrap(),
        flate2::Compression::default(),
    );
    let mut builder = tar::Builder::new(encoder);
    let mut global = tar::Header::new_gnu();
    global.set_entry_type(tar::EntryType::XGlobalHeader);
    global.set_size(0);
    builder
        .append_data(&mut global, "pax_global_header", std::io::empty())
        .unwrap();
    let mut file = tar::Header::new_gnu();
    file.set_mode(0o644);
    file.set_size(0);
    builder
        .append_data(&mut file, "jpx-1.2.0/README", std::io::empty())
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();
    assert_eq!(
        list_entries(&path, ArchiveFormat::TarGz).unwrap(),
        vec![entry(&["jpx-1.2.0", "README"], false, false)]
    );
}

#[test]
fn rejects_tar_gz_read_as_zip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.zip");
    write_tar_gz(&path, &[("jpx-1.2.0/README", 0o644)]);
    let error = list_entries(&path, ArchiveFormat::Zip).unwrap_err();
    assert!(error.starts_with("not a zip archive: "), "{error}");
}

#[test]
fn rejects_zip_read_as_tar_gz() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.tar.gz");
    write_zip(&path, &[("jpx-1.2.0/README", 0o644)]);
    let error = list_entries(&path, ArchiveFormat::TarGz).unwrap_err();
    assert!(error.starts_with("not a tar.gz archive: "), "{error}");
}
