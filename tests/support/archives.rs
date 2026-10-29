use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
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

pub fn sha256_of(path: &Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(path).unwrap()))
}

pub fn tar_gz(dir: &TempDir, name: &str, entries: &[(&str, u32)]) -> PathBuf {
    let path = dir.path().join(name);
    write_tar_gz(&path, entries);
    path
}
