#![windows_subsystem = "windows"]

use std::env;
use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipArchive;

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"));

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let target_dir = match env::var_os("LOCALAPPDATA") {
        Some(local_app_data) => PathBuf::from(local_app_data).join("Floword_App"),
        None => env::temp_dir().join("Floword_App"),
    };

    if let Err(e) = extract_payload_if_needed(&target_dir) {
        eprintln!("Failed to prepare Floword application files: {}", e);
        return;
    }

    let main_exe = target_dir.join("Floword.exe");
    let fallback_exe = target_dir.join("artcraft.exe");
    let executable = if main_exe.exists() {
        main_exe
    } else {
        fallback_exe
    };

    if !executable.exists() {
        eprintln!("Floword binary not found at {}", executable.display());
        return;
    }

    let mut cmd = Command::new(&executable);
    cmd.current_dir(&target_dir);
    cmd.args(&args);

    match cmd.spawn() {
        Ok(mut child) => {
            let _ = child.wait();
        }
        Err(e) => {
            eprintln!("Failed to start {}: {}", executable.display(), e);
        }
    }
}

fn extract_payload_if_needed(target_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if PAYLOAD.is_empty() {
        // If payload is empty (e.g. debug stub), assume directory already staged
        return Ok(());
    }

    let version_file = target_dir.join(".floword_payload_version");
    let current_len = PAYLOAD.len().to_string();

    if version_file.exists() {
        if let Ok(saved_len) = fs::read_to_string(&version_file) {
            if saved_len.trim() == current_len && target_dir.join("Floword.exe").exists() {
                // Already extracted and up to date
                return Ok(());
            }
        }
    }

    fs::create_dir_all(target_dir)?;

    let reader = Cursor::new(PAYLOAD);
    let mut archive = ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    let _ = fs::write(&version_file, current_len);
    Ok(())
}
