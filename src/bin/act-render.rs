use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use act_desktop_app::media::{NativeRenderer, RendererConfig};

fn main() -> ExitCode {
    match run() {
        Ok(receipt) => {
            println!("render receipt: {}", receipt.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<PathBuf, String> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if !(2..=3).contains(&arguments.len()) {
        return Err("usage: act-render <project.json> <project-root> [render-receipt.json]".into());
    }
    let project_path = PathBuf::from(&arguments[0]);
    let project_root = PathBuf::from(&arguments[1]);
    let receipt_path = arguments
        .get(2)
        .map_or_else(|| PathBuf::from("render-receipt.json"), PathBuf::from);

    let renderer = NativeRenderer::new(RendererConfig::default());
    let receipt = renderer
        .render(&project_path, &project_root)
        .map_err(|error| format!("{}: {error}", serde_json::to_string(&error.code()).unwrap()))?;
    write_receipt(&project_root, &receipt_path, &receipt)
}

fn write_receipt(
    project_root: &Path,
    receipt_path: &Path,
    receipt: &act_interfaces::creator_media::CreatorRenderReceipt,
) -> Result<PathBuf, String> {
    let root = project_root
        .canonicalize()
        .map_err(|error| format!("receipt root is unavailable: {error}"))?;
    let destination = if receipt_path.is_absolute() {
        receipt_path.to_path_buf()
    } else {
        root.join(receipt_path)
    };
    let parent = destination
        .parent()
        .ok_or_else(|| "receipt path has no parent".to_owned())?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| format!("receipt parent must already exist: {error}"))?;
    if !canonical_parent.starts_with(&root) {
        return Err("receipt must stay inside the project root".into());
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
        .map_err(|error| format!("receipt will not overwrite an existing file: {error}"))?;
    serde_json::to_writer_pretty(&mut file, receipt)
        .map_err(|error| format!("receipt could not be serialized: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("receipt could not be finalized: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("receipt could not be synced: {error}"))?;
    Ok(destination)
}
