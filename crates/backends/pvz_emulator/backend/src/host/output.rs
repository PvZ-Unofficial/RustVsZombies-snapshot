use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use rsvz_backend_api::artifact::SessionArtifact;
use serde::Serialize;

pub(super) fn write_control<T: Serialize>(path: &Path, run_id: &str, value: &T) -> Result<(), String> {
    write_atomic(path, run_id, |writer| {
        serde_json::to_writer(&mut *writer, value).map_err(|error| error.to_string())?;
        writer.write_all(b"\n").map_err(|error| error.to_string())
    })
}

pub(super) fn write_artifact(path: &Path, run_id: &str, artifact: &SessionArtifact) -> Result<(), String> {
    write_atomic(path, run_id, |writer| {
        artifact.write_json(writer).map_err(|error| error.to_string())
    })
}

fn write_atomic(
    path: &Path, run_id: &str, write: impl FnOnce(&mut dyn Write) -> Result<(), String>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("output path has no UTF-8 file name")?;
    let temporary = parent.join(format!(".{name}.{run_id}.tmp"));
    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        let mut writer = BufWriter::new(file);
        write(&mut writer)?;
        writer.flush().map_err(|error| error.to_string())?;
        drop(writer);
        if path.exists() {
            return Err(format!("output path already exists: {}", path.display()));
        }
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _cleanup_result = fs::remove_file(&temporary);
    }
    result
}
