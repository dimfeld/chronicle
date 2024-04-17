use filigree::email::templates::{render_template_pair, EmailContent, EmailTemplate, TeraError};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct PasswordResetRequestTemplate {
    pub user_name: Option<String>,
    pub url_scheme: &'static str,
    pub host: String,
    pub email: String,
    pub token: Uuid,
}

#[derive(Debug, Serialize)]
struct TemplateContext<'a> {
    user_name: &'a Option<String>,
    url: String,
    regenerate_url: String,
}

impl EmailTemplate for PasswordResetRequestTemplate {
    fn subject(&self) -> String {
        "Reset your chronicle password".to_string()
    }

    fn render(&self, renderer: &tera::Tera) -> Result<EmailContent, TeraError> {
        let url = format!(
            "{scheme}://{host}/reset_password?token={token}&email={email}",
            scheme = self.url_scheme,
            host = self.host,
            token = self.token,
            email = utf8_percent_encode(&self.email, NON_ALPHANUMERIC),
        );

        let regenerate_url = format!(
            "{scheme}://{host}/reset_password",
            scheme = self.url_scheme,
            host = self.host
        );

        render_template_pair(
            renderer,
            &TemplateContext {
                user_name: &self.user_name,
                url,
                regenerate_url,
            },
            "password_reset_request.html",
            "password_reset_request.txt",
        )
    }

    fn tags(&self) -> Vec<String> {
        vec!["passwordless_login".to_string()]
    }
}
