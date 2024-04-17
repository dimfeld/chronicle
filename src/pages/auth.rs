use std::borrow::Cow;

use axum::{
    extract::{FromRequestParts, Request},
    response::{IntoResponse, Redirect, Response},
};
use axum_htmx::HxLocation;
use filigree::{auth::AuthInfo as _, errors::HttpError};
use futures::future::BoxFuture;
use http::{request::Parts, StatusCode, Uri};
use tower::{Layer, Service};

use crate::{
    auth::{AuthInfo, Authed},
    Error,
};

pub struct WebAuthed(pub Authed);

impl std::ops::Deref for WebAuthed {
    type Target = AuthInfo;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Into<Authed> for WebAuthed {
    fn into(self) -> Authed {
        self.0
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for WebAuthed
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match filigree::auth::get_auth_info_from_parts(parts).await {
            Ok(auth_info) => Ok(WebAuthed(Authed::new(auth_info))),
            Err(e) => match e.status_code() {
                StatusCode::UNAUTHORIZED => {
                    let login_url = make_login_link(Some(&parts.uri));
                    Err(redirect_body(login_url))
                }
                _ => {
                    let e = Error::from(e);
                    Err(super::generic_error::generic_error_page(&e))
                }
            },
        }
    }
}

fn redirect_body(to: Uri) -> Response {
    let t = to.to_string();
    (HxLocation::from_uri(to), Redirect::to(&t)).into_response()
}

pub fn make_login_link(redirect_to: Option<&Uri>) -> Uri {
    if let Some(r) = redirect_to {
        let redirect_to = r
            .path_and_query()
            .map(|p| {
                Cow::Owned(
                    url::form_urlencoded::byte_serialize(p.as_str().as_bytes()).collect::<String>(),
                )
            })
            .unwrap_or(Cow::Borrowed("/"));
        format!("/login?redirect_to={redirect_to}")
            .parse::<Uri>()
            .unwrap_or_else(|_| "/login".parse().unwrap())
    } else {
        "/login".parse().unwrap()
    }
}

/// Disallow fallback anonymous users and return a redirect to the login page.
#[allow(dead_code)]
pub fn web_not_anonymous() -> NotAnonymousLayer {
    NotAnonymousLayer {}
}

/// The middleware layer for disallowing anonymous fallback users
#[derive(Clone)]
pub struct NotAnonymousLayer {}

impl<S> Layer<S> for NotAnonymousLayer {
    type Service = NotAnonymousService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        NotAnonymousService { inner }
    }
}

/// The middleware service for disallowing anonymous fallback users
#[derive(Clone)]
pub struct NotAnonymousService<S> {
    inner: S,
}

impl<S> Service<Request> for NotAnonymousService<S>
where
    S: Service<Request, Response = axum::response::Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: IntoResponse,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let cloned = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, cloned);

        Box::pin(async move {
            let (request, info) = match filigree::auth::get_auth_info::<AuthInfo>(request).await {
                Ok(x) => x,
                Err(e) => return Ok(e.into_response()),
            };

            if info.is_anonymous() {
                let uri = request.uri();
                let to = make_login_link(Some(uri));
                return Ok(redirect_body(to));
            }

            inner.call(request).await
        })
    }
}
