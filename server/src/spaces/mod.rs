pub mod access;
pub mod shared_pages;
pub mod page_viewer_grant_management;
pub(crate) mod page_viewer_grants;
pub mod attachments;
pub(crate) mod asset_audiences;
pub mod comments;
pub mod documents;
pub(crate) mod document_deletion;
pub(crate) mod space_deletion;
pub mod drafts;
pub mod draft_discussions;
pub(crate) mod draft_save;
mod draft_save_source;
pub(crate) mod draft_birth;
pub(crate) mod draft_transfer;
pub(crate) mod draft_publication;
pub(crate) mod draft_purge;
pub(crate) mod draft_owner;
pub(crate) mod published_source;
pub mod folders;
pub mod graph;
pub mod maintenance;
pub(crate) mod lifecycle;
pub mod moves;
pub mod mutation_guards;
pub mod page_metadata;
pub mod page_access;
pub mod page_restrictions;
pub mod properties;
pub mod property_validation;
pub mod routes;
pub mod tags;
pub mod templates;
pub mod versions;
pub(crate) mod publication_snapshots;
pub mod publication_snapshot_readers;
pub mod publication_snapshot_recovery;
pub mod workflows;

use crate::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    let router = routes::router()
        .merge(comments::router())
        .merge(drafts::router())
        .merge(draft_discussions::router())
        .merge(draft_save_source::router())
        .merge(versions::router())
        .merge(publication_snapshot_readers::router())
        .merge(publication_snapshot_recovery::router())
        .merge(tags::router())
        .merge(attachments::router())
        .merge(asset_audiences::router())
        .merge(properties::router())
        .merge(templates::router())
        .merge(workflows::router())
        .merge(graph::router())
        .merge(moves::router())
        .merge(page_metadata::router())
        .merge(shared_pages::router())
        .merge(page_viewer_grant_management::router())
        .merge(page_access::router())
        .merge(page_restrictions::router())
        .merge(maintenance::router());
    #[cfg(target_os = "linux")]
    let router = router.merge(crate::storage::attachment_versions::router());
    router
}

#[cfg(all(test,target_os="linux"))]
mod asset_audience_tests;
