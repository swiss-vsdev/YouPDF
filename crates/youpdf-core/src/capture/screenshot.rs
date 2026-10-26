//! Screenshot capture via macOS `screencapture` and clipboard.
//!
//! ponytail: macOS only; Windows/Linux stubs return NotSupported, add
//! SnippingTool.exe /clip and flameshot gui -r later via #[cfg].

use std::path::PathBuf;
use std::process::Command;
use youpdf_shared::{Error, Result};

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

pub fn capture_region_interactive() -> Result<PathBuf> {
    let temp = std::env::temp_dir().join(format!("screencapture-{}.png", timestamp()));
    let status = Command::new("screencapture")
        .arg("-i")
        .arg("-x")
        .arg(&temp)
        .status()
        .map_err(|e| Error::Pdf(format!("screencapture spawn: {e}")))?;
    if !status.success() {
        return Err(Error::Pdf("screencapture exited with error".into()));
    }
    if temp.exists() {
        Ok(temp)
    } else {
        Err(Error::Pdf("capture failed: file not created".into()))
    }
}

pub fn capture_window(window_id: u32) -> Result<PathBuf> {
    let temp = std::env::temp_dir().join(format!("screencapture-{}.png", timestamp()));
    let status = Command::new("screencapture")
        .arg("-l")
        .arg(window_id.to_string())
        .arg("-x")
        .arg(&temp)
        .status()
        .map_err(|e| Error::Pdf(format!("screencapture spawn: {e}")))?;
    if !status.success() {
        return Err(Error::Pdf("screencapture exited with error".into()));
    }
    if temp.exists() {
        Ok(temp)
    } else {
        Err(Error::Pdf("capture failed: file not created".into()))
    }
}

pub fn clipboard_as_png() -> Result<Vec<u8>> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg("the clipboard as «class PNGf»")
        .output()
        .map_err(|e| Error::Pdf(format!("osascript spawn: {e}")))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(Error::Pdf("clipboard does not contain PNG".into()))
    }
}
