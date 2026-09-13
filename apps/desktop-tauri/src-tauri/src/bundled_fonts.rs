//! On-disk MiSans VF, loaded into a DirectWrite collection so the strip can
//! paint the default family without a system install.
//!
//! File lives in `apps/desktop-tauri/public/fonts/` and is copied into the
//! Tauri resource dir as `fonts/`. The other whitelist families are
//! install-guided and come from the system collection once the user installs
//! them.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory, IDWriteFactory5,
    IDWriteFontCollection, IDWriteFontCollection1, IDWriteFontSetBuilder1,
};
use windows::core::{HSTRING, Interface};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BundledFace {
    /// Name stored in settings and shown in the picker.
    pub picker_name: &'static str,
    /// `name` table family DirectWrite actually sees.
    pub native_name: &'static str,
    pub file_name: &'static str,
}

pub const BUNDLED_FACES: &[BundledFace] = &[BundledFace {
    picker_name: "MiSans VF",
    native_name: "MiSans VF",
    file_name: "MiSansVF.ttf",
}];

pub struct SharedDwrite {
    pub factory: IDWriteFactory,
    pub collection: Option<IDWriteFontCollection>,
}

static SHARED: OnceLock<SharedDwrite> = OnceLock::new();

pub fn shared() -> &'static SharedDwrite {
    SHARED.get_or_init(|| match load() {
        Ok(shared) => shared,
        Err(err) => {
            tracing::warn!("bundled fonts: DirectWrite setup failed: {err}");
            let factory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
                .expect("DWriteCreateFactory");
            SharedDwrite {
                factory,
                collection: None,
            }
        }
    })
}

fn load() -> windows::core::Result<SharedDwrite> {
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
    let collection = match font_dir() {
        Some(dir) => match unsafe { create_collection(&factory, &dir) } {
            Ok(collection) => Some(collection),
            Err(err) => {
                tracing::warn!(
                    path = %dir.display(),
                    "bundled fonts: collection failed: {err}"
                );
                None
            }
        },
        None => {
            tracing::warn!("bundled fonts: font directory not found");
            None
        }
    };
    Ok(SharedDwrite {
        factory,
        collection,
    })
}

unsafe fn create_collection(
    factory: &IDWriteFactory,
    dir: &Path,
) -> windows::core::Result<IDWriteFontCollection> {
    let factory5: IDWriteFactory5 = factory.cast()?;
    let builder: IDWriteFontSetBuilder1 = unsafe { factory5.CreateFontSetBuilder()? };
    for face in BUNDLED_FACES {
        let path = dir.join(face.file_name);
        if !path.is_file() {
            tracing::warn!(file = face.file_name, "bundled fonts: missing file");
            continue;
        }
        let file =
            unsafe { factory.CreateFontFileReference(&HSTRING::from(path.as_os_str()), None)? };
        unsafe { builder.AddFontFile(&file)? };
    }
    let set = unsafe { builder.CreateFontSet()? };
    let collection1: IDWriteFontCollection1 =
        unsafe { factory5.CreateFontCollectionFromFontSet(&set)? };
    collection1.cast()
}

/// Directory that contains `MiSansVF.ttf`, if it exists.
pub fn font_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(exe.join("fonts"));
        candidates.push(exe.join("resources").join("fonts"));
    }
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest).join("../public/fonts"));
        candidates.push(PathBuf::from(manifest).join("resources/fonts"));
    }
    candidates.push(PathBuf::from("public/fonts"));
    candidates
        .into_iter()
        .find(|dir| dir.join("MiSansVF.ttf").is_file())
}

pub fn files_present() -> bool {
    let Some(dir) = font_dir() else {
        return false;
    };
    BUNDLED_FACES
        .iter()
        .all(|face| dir.join(face.file_name).is_file())
}

fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

fn aliases_of(face: &BundledFace) -> Vec<String> {
    let mut names = vec![normalize(face.picker_name), normalize(face.native_name)];
    if face.picker_name == "MiSans VF" {
        names.push("misans".into());
    }
    names
}

pub fn match_face(name: &str) -> Option<&'static BundledFace> {
    let n = normalize(name);
    BUNDLED_FACES
        .iter()
        .find(|face| aliases_of(face).iter().any(|alias| alias == &n))
}

/// Native family name to pass to `CreateTextFormat`, plus the bundled
/// collection when this family is one of ours.
pub fn resolve<'a>(
    shared: &'a SharedDwrite,
    requested: &str,
) -> (String, Option<&'a IDWriteFontCollection>) {
    match match_face(requested) {
        Some(face) if shared.collection.is_some() => {
            (face.native_name.to_string(), shared.collection.as_ref())
        }
        _ => (requested.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_files_are_present() {
        assert!(
            files_present(),
            "expected MiSansVF.ttf under {:?}",
            font_dir()
        );
    }

    #[test]
    fn picker_names_map_to_native_faces() {
        assert_eq!(
            match_face("MiSans VF").map(|f| f.native_name),
            Some("MiSans VF")
        );
        assert!(match_face("Source Han Sans VF").is_none());
        assert!(match_face("Microsoft YaHei UI").is_none());
    }
}
