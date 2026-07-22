pub mod api_keys;
pub mod jwt;
pub mod middleware;
pub mod oauth;
pub mod oidc;
pub mod password;
pub mod routes;
pub mod totp;

use axum::Router;

pub fn router() -> Router<crate::AppState> {
    routes::router()
}
