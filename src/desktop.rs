//! Linux desktop integration.
//!
//! Wayland has no protocol for a window to set its task-bar icon. The shell instead
//! looks up `app_id` among the installed `.desktop` files and uses that entry's
//! `Icon=`. A bare binary launched from a terminal therefore gets the generic
//! Wayland icon until this entry and the themed icon are installed.

use crate::icon;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
pub fn install() -> Result<PathBuf, String> {
    let data = data_home()?;
    let apps = data.join("applications");
    let icons = data.join("icons").join("hicolor");
    std::fs::create_dir_all(&apps).map_err(|e| format!("Could not create {}: {e}", apps.display()))?;

    let exe = std::env::current_exe().map_err(|e| format!("Could not find the CncCraft executable: {e}"))?;
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=CncCraft\n\
         GenericName=CNC design and toolpaths\n\
         Comment=Design, toolpaths, G-code and machine control for CNC routers\n\
         Exec={}\n\
         Icon=cnccraft\n\
         Terminal=false\n\
         Categories=Graphics;Engineering;\n\
         Keywords=CNC;router;woodworking;G-code;CAD;CAM;\n\
         StartupNotify=true\n\
         StartupWMClass=cnccraft\n",
        exec_path(&exe)
    );
    let desktop_path = apps.join("cnccraft.desktop");
    std::fs::write(&desktop_path, entry).map_err(|e| format!("Could not write {}: {e}", desktop_path.display()))?;

    // The shell asks for a size and falls back to scaling the closest one. Install the
    // usual range so panels and launchers both get a crisp icon.
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let dir = icons.join(format!("{size}x{size}")).join("apps");
        std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        let rgba = icon::icon_rgba(size);
        let image = image::RgbaImage::from_raw(size, size, rgba).ok_or_else(|| "Could not build the app icon".to_string())?;
        let path = dir.join("cnccraft.png");
        image.save(&path).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    }

    // These only refresh caches; both tools are optional, missing ones are fine, and
    // their diagnostics are not useful to a user who just wants the icon.
    let quiet = || (std::process::Stdio::null(), std::process::Stdio::null());
    let (out, err) = quiet();
    let _ = std::process::Command::new("update-desktop-database").arg(&apps).stdout(out).stderr(err).status();
    let (out, err) = quiet();
    let _ = std::process::Command::new("gtk-update-icon-cache").args(["-f", "-t"]).arg(&icons).stdout(out).stderr(err).status();
    Ok(desktop_path)
}

#[cfg(not(target_os = "linux"))]
pub fn install() -> Result<PathBuf, String> {
    Err("The desktop icon only needs installing on Linux".into())
}

#[cfg(target_os = "linux")]
fn data_home() -> Result<PathBuf, String> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .ok_or_else(|| "Neither XDG_DATA_HOME nor HOME is set".to_string())
}

/// Quote a path for the Desktop Entry `Exec` key and pass dropped files through.
#[cfg(target_os = "linux")]
fn exec_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%");
    format!("\"{text}\" %F")
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn exec_quotes_spaces_and_desktop_entry_codes() {
        let got = exec_path(Path::new("/home/me/CnC Craft/cnccraft"));
        assert_eq!(got, "\"/home/me/CnC Craft/cnccraft\" %F");
        assert_eq!(exec_path(Path::new("/tmp/100%/cnccraft")), "\"/tmp/100%%/cnccraft\" %F");
    }
}
