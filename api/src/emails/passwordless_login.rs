use filigree::email::templates::{render_template_pair, EmailContent, EmailTemplate, TeraError};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug)]
pub struct PasswordlessLoginRequestTemplate {
    pub user_name: Option<String>,
    pub url_scheme: &'static str,
    pub host: String,
    pub email: String,
    pub token: Uuid,
    pub redirect_to: Option<String>,
    pub invite: bool,
}

#[derive(Debug, Serialize)]
struct TemplateContext<'a> {
    user_name: &'a Option<String>,
    url: String,
    login_url: String,
}

impl EmailTemplate for PasswordlessLoginRequestTemplate {
    fn subject(&self) -> String {
        "Log in to chronicle".to_string()
    }

    fn render(&self, renderer: &tera::Tera) -> Result<EmailContent, TeraError> {
        let url = format!(
            "{scheme}://{host}/login?token={token}&email={email}{rd_key}{rd_value}{invite}",
            scheme = self.url_scheme,
            host = self.host,
            token = self.token,
            email = utf8_percent_encode(&self.email, NON_ALPHANUMERIC),
            rd_key = if self.redirect_to.is_some() {
                "&redirect_to="
            } else {
                ""
            },
            rd_value = self
                .redirect_to
                .as_deref()
                .map(|s| std::borrow::Cow::from(utf8_percent_encode(s, NON_ALPHANUMERIC)))
                .unwrap_or_default(),
            invite = if self.invite { "&invite=true" } else { "" },
        );

        let login_url = format!(
            "{scheme}://{host}/login",
            scheme = self.url_scheme,
            host = self.host
        );

        render_template_pair(
            renderer,
            &TemplateContext {
                user_name: &self.user_name,
                url,
                login_url,
            },
            "passwordless_login.html",
            "passwordless_login.txt",
        )
    }

    fn tags(&self) -> Vec<String> {
        vec!["passwordless_login".to_string()]
    }
}
