use std::path::Path;

use filigree::vite_manifest::{watch::ManifestWatcher, Manifest, ManifestError};
use maud::{html, Markup, DOCTYPE};

use super::auth::WebAuthed;

use super::auth::WebAuthed;

pub static MANIFEST: Manifest = Manifest::new();

pub fn init_page_layout(
    manifest_path: Option<&Path>,
    watch_manifest: bool,
) -> Result<Option<ManifestWatcher>, error_stack::Report<ManifestError>> {
    let Some(manifest_path) = manifest_path else {
        return Ok(None);
    };

    let base_url = "";
    MANIFEST.read_manifest(base_url, manifest_path)?;

    let watcher = if watch_manifest {
        Some(filigree::vite_manifest::watch::watch_manifest(
            base_url.to_string(),
            manifest_path.to_path_buf(),
            &MANIFEST,
        ))
    } else {
        None
    };

    Ok(watcher)
}

/// The HTML shell that every page should be wrapped in to enable basic functionality.
pub fn page_wrapper(title: &str, slot: Markup) -> Markup {
    let client_tags = MANIFEST.index();
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                (client_tags)
                title { (title) }
            }
            body
                hx-boost="true"
                hx-ext="alpine-morph,head-support"
            {
                (slot)
            }
        }
    }
}

/// The root layout of the application
pub fn root_layout(auth: Option<&WebAuthed>, slot: Markup) -> Markup {
    html! {
        (slot)
    }
}

/// The root layout of the application, as a full HTML page
pub fn root_layout_page(auth: Option<&WebAuthed>, title: &str, slot: Markup) -> Markup {
    page_wrapper(title, root_layout(auth, slot))
}
