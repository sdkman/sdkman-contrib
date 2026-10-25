use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use sdkman_contrib::publish::RELEASE_PLATFORMS;
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// A name ending in `/` becomes a directory entry. The mode is the Unix permission bits.
pub fn write_zip(path: &Path, entries: &[(&str, u32)]) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    for (name, mode) in entries {
        let options = SimpleFileOptions::default().unix_permissions(*mode);
        if name.ends_with('/') {
            writer.add_directory(*name, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
        }
    }
    writer.finish().unwrap();
}

/// A name ending in `/` becomes a directory entry. The mode is the Unix permission bits.
pub fn write_tar_gz(path: &Path, entries: &[(&str, u32)]) {
    let encoder = GzEncoder::new(File::create(path).unwrap(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, mode) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(if name.ends_with('/') {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        header.set_mode(*mode);
        header.set_size(0);
        builder
            .append_data(&mut header, name, std::io::empty())
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();
}

pub fn tar_gz(dir: &TempDir, name: &str, entries: &[(&str, u32)]) -> PathBuf {
    let path = dir.path().join(name);
    write_tar_gz(&path, entries);
    path
}

pub fn write_release(dir: &Path, version: &str, base_directory: &str) {
    for release_platform in &RELEASE_PLATFORMS {
        write_release_archive(dir, version, release_platform.suffix, base_directory);
    }
}

pub fn write_release_archive(dir: &Path, version: &str, suffix: &str, base_directory: &str) {
    let path = dir.join(format!("contrib-{version}-{suffix}"));
    let binary = if suffix.ends_with(".zip") {
        "contrib.exe"
    } else {
        "contrib"
    };
    let entries = [
        (format!("{base_directory}/"), 0o755),
        (format!("{base_directory}/bin/"), 0o755),
        (format!("{base_directory}/bin/{binary}"), 0o755),
    ];
    let entries: Vec<(&str, u32)> = entries
        .iter()
        .map(|(name, mode)| (name.as_str(), *mode))
        .collect();
    if suffix.ends_with(".zip") {
        write_zip(&path, &entries);
    } else {
        write_tar_gz(&path, &entries);
    }
}
