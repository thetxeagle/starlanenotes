pub mod attachments;
pub mod comments;
pub mod documents;
pub mod folders;
pub mod graph;
pub mod properties;
pub mod routes;
pub mod tags;
pub mod templates;
pub mod versions;
pub mod workflows;

use crate::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    routes::router()
        .merge(comments::router())
        .merge(versions::router())
        .merge(tags::router())
        .merge(attachments::router())
        .merge(properties::router())
        .merge(templates::router())
        .merge(workflows::router())
        .merge(graph::router())
}
