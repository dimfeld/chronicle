use filigree::email::templates::create_templates;
use rust_embed::RustEmbed;

mod password_reset_request;
mod passwordless_login;

pub use password_reset_request::*;
pub use passwordless_login::*;

#[derive(RustEmbed)]
#[folder = "src/emails/templates"]
pub struct RootTemplates;

pub fn create_tera() -> tera::Tera {
    let files = RootTemplates::iter().map(|filename| {
        let data = RootTemplates::get(filename.as_ref()).unwrap();
        (filename, data)
    });
    create_templates(files)
}
