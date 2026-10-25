mod support;

use sdkman_contrib::archive::{
    ArchiveEntry, ArchiveFormat, Verdict, check_layout, inspect_archive, list_entries,
};
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

fn layout(entries: &[ArchiveEntry], platform: &str) -> Verdict {
    check_layout(entries, "jpx", "1.2.0", platform)
}

fn conventional_entries() -> Vec<ArchiveEntry> {
    vec![
        entry(&["jpx-1.2.0"], true, false),
        entry(&["jpx-1.2.0", "bin"], true, false),
        entry(&["jpx-1.2.0", "bin", "jpx"], false, true),
    ]
}

#[test]
fn accepts_conventional_layout() {
    assert_eq!(
        layout(&conventional_entries(), "LINUX_X64"),
        Verdict::default()
    );
}

#[test]
fn accepts_layout_without_directory_entries() {
    let entries = vec![entry(&["jpx-1.2.0", "bin", "jpx"], false, true)];
    assert_eq!(layout(&entries, "LINUX_X64"), Verdict::default());
}

#[test]
fn fails_with_two_base_directories() {
    let mut entries = conventional_entries();
    entries.push(entry(&["other", "README"], false, false));
    assert_eq!(
        layout(&entries, "LINUX_X64").failures,
        vec!["does not hold exactly one base directory"]
    );
}

#[test]
fn fails_with_a_file_beside_the_base_directory() {
    let mut entries = conventional_entries();
    entries.push(entry(&["README"], false, false));
    assert_eq!(
        layout(&entries, "LINUX_X64").failures,
        vec!["does not hold exactly one base directory"]
    );
}

#[test]
fn fails_with_only_a_base_directory() {
    let entries = vec![entry(&["jpx-1.2.0"], true, false)];
    assert_eq!(
        layout(&entries, "LINUX_X64").failures,
        vec!["does not hold exactly one base directory"]
    );
}

#[test]
fn fails_without_bin() {
    let entries = vec![
        entry(&["jpx-1.2.0"], true, false),
        entry(&["jpx-1.2.0", "lib", "jpx.jar"], false, false),
        entry(&["jpx-1.2.0", "lib", "bin", "jpx"], false, true),
    ];
    assert_eq!(
        layout(&entries, "LINUX_X64").failures,
        vec!["no bin/ under jpx-1.2.0/"]
    );
}

#[test]
fn fails_when_bin_is_a_file() {
    let entries = vec![entry(&["jpx-1.2.0", "bin"], false, true)];
    assert_eq!(
        layout(&entries, "LINUX_X64").failures,
        vec!["no bin/ under jpx-1.2.0/"]
    );
}

#[test]
fn fails_without_executable_in_bin() {
    let entries = vec![
        entry(&["jpx-1.2.0", "bin", "jpx"], false, false),
        entry(&["jpx-1.2.0", "bin", "sub", "tool"], false, true),
        entry(&["jpx-1.2.0", "lib", "tool"], false, true),
    ];
    assert_eq!(
        layout(&entries, "LINUX_X64").failures,
        vec!["no executable file in jpx-1.2.0/bin/"]
    );
}

#[test]
fn windows_needs_no_executable() {
    let entries = vec![entry(&["jpx-1.2.0", "bin", "jpx.exe"], false, false)];
    assert_eq!(layout(&entries, "WINDOWS_X64"), Verdict::default());
}

#[test]
fn warns_on_unconventional_base() {
    let entries = vec![entry(&["tool", "bin", "jpx"], false, true)];
    let verdict = layout(&entries, "LINUX_X64");
    assert!(verdict.failures.is_empty());
    assert_eq!(
        verdict.warnings,
        vec!["base directory is 'tool' (expected 'jpx-1.2.0')"]
    );
}

#[test]
fn inspects_a_written_archive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.zip");
    write_zip(&path, &[("jpx-1.2.0/bin/jpx", 0o755)]);
    let verdict = inspect_archive(&path, ArchiveFormat::Zip, "jpx", "1.2.0", "UNIVERSAL");
    assert_eq!(verdict, Verdict::default());
}

#[test]
fn inspect_turns_a_read_error_into_one_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpx.zip");
    write_tar_gz(&path, &[("jpx-1.2.0/bin/jpx", 0o755)]);
    let verdict = inspect_archive(&path, ArchiveFormat::Zip, "jpx", "1.2.0", "UNIVERSAL");
    assert_eq!(verdict.failures.len(), 1);
    assert!(verdict.failures[0].starts_with("not a zip archive: "));
}
