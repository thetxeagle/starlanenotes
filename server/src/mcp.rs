use axum::{
    extract::{FromRef, FromRequestParts, Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/mcp/manifest", get(get_mcp_manifest))
        .route("/mcp/tools/{tool_name}", post(execute_mcp_tool_http))
}

pub async fn get_mcp_manifest(_auth: AuthUser) -> Json<McpServerManifest> {
    Json(mcp_server_manifest())
}

/// Read POST tools use an explicit key ceiling; the request method and identity
/// evidence are never rewritten. New tools do not inherit this allowlist.
struct McpToolAuth(AuthUser);
impl<S> FromRequestParts<S> for McpToolAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = crate::auth::middleware::AuthError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Path(name) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| crate::auth::middleware::AuthError::InvalidToken)?;
        let operation = if mcp_tool_is_executable_http(&name)
            && matches!(
                name.as_str(),
                "starlane.spaces.list"
                    | "starlane.spaces.get"
                    | "starlane.folders.list"
                    | "starlane.documents.get"
                    | "starlane.search.query"
                    | "starlane.graph.get"
                    | "starlane.tags.list"
                    | "starlane.properties.list"
                    | "starlane.templates.list"
                    | "starlane.admin.users.list"
            ) {
            crate::auth::middleware::ApiKeyRequestOperation::Read
        } else {
            crate::auth::middleware::ApiKeyRequestOperation::for_method(&parts.method)
        };
        let state = AppState::from_ref(state);
        crate::auth::middleware::authenticate_request(parts, &state, operation)
            .await
            .map(Self)
    }
}
#[cfg(test)]
thread_local! {
    static MCP_AFTER_DOCUMENT_READ:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);
    static MCP_AFTER_FINAL_DOCUMENT_SOURCE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);
    static MCP_AFTER_FINAL_PAGE_VIEW:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);
    static MCP_BEFORE_FINAL_RESPONSE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);
}
/// Test pause occurs before the consumer's last current resource proof, never
/// after a proof-complete shared result. Production has no additional await.
#[cfg(test)]
async fn before_mcp_final_resource_proof() {
    #[cfg(test)]
    {
        let hook = MCP_BEFORE_FINAL_RESPONSE.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook {
            hook.await;
        }
    }
}
async fn execute_mcp_tool_http(
    State(state): State<AppState>,
    McpToolAuth(auth): McpToolAuth,
    Path(name): Path<String>,
    Json(request): Json<McpToolExecutionRequest>,
) -> Result<Json<McpToolExecutionResponse>, AppError> {
    execute_mcp_tool(State(state), auth, Path(name), Json(request)).await
}

pub async fn execute_mcp_tool(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tool_name): Path<String>,
    Json(request): Json<McpToolExecutionRequest>,
) -> Result<Json<McpToolExecutionResponse>, AppError> {
    let tool =
        find_mcp_tool(&tool_name).ok_or_else(|| AppError::BadRequest("Unknown MCP tool".into()))?;
    if !mcp_tool_is_executable_http(tool.name) {
        return Err(AppError::Forbidden(
            "MCP tool execution is currently limited to read-only HTTP tools".into(),
        ));
    }

    require_mcp_current_read(&state, &auth, tool.name == "starlane.admin.users.list").await?;
    let result = match tool.name {
        "starlane.spaces.list" => execute_spaces_list(&state, &auth).await?,
        "starlane.spaces.get" => execute_space_get(&state, &auth, &request.input).await?,
        "starlane.folders.list" => execute_folders_list(&state, &auth, &request.input).await?,
        "starlane.documents.get" => execute_document_get(&state, &auth, &request.input).await?,
        "starlane.search.query" => execute_search_query(&state, &auth, &request.input).await?,
        "starlane.graph.get" => execute_graph_get(&state, &auth, &request.input).await?,
        "starlane.tags.list" => execute_tags_list(&state, &auth, &request.input).await?,
        "starlane.properties.list" => {
            execute_properties_list(&state, &auth, &request.input).await?
        }
        "starlane.templates.list" => execute_templates_list(&state, &auth, &request.input).await?,
        "starlane.admin.users.list" => {
            execute_admin_users_list(&state, &auth, &request.input).await?
        }
        _ => {
            return Err(AppError::Forbidden(
                "MCP tool execution is currently limited to read-only HTTP tools".into(),
            ));
        }
    };

    Ok(Json(McpToolExecutionResponse {
        tool: tool.name,
        read_only: true,
        result,
    }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpScope {
    NotesRead,
    NotesWrite,
    NotesDelete,
    NotesAdmin,
    NotesExport,
    NotesImport,
}

impl McpScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotesRead => "notes:read",
            Self::NotesWrite => "notes:write",
            Self::NotesDelete => "notes:delete",
            Self::NotesAdmin => "notes:admin",
            Self::NotesExport => "notes:export",
            Self::NotesImport => "notes:import",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpConfirmationPolicy {
    None,
    Required,
    RequiredForBulk,
}

impl McpConfirmationPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Required => "required",
            Self::RequiredForBulk => "required_for_bulk",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpToolFamily {
    Spaces,
    Folders,
    Documents,
    Metadata,
    Templates,
    Search,
    Graph,
    Workflows,
    ImportsExports,
    Admin,
}

impl McpToolFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Spaces => "spaces",
            Self::Folders => "folders",
            Self::Documents => "documents",
            Self::Metadata => "metadata",
            Self::Templates => "templates",
            Self::Search => "search",
            Self::Graph => "graph",
            Self::Workflows => "workflows",
            Self::ImportsExports => "imports_exports",
            Self::Admin => "admin",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct McpToolDefinition {
    pub name: &'static str,
    pub family: McpToolFamily,
    pub required_scopes: &'static [McpScope],
    pub confirmation: McpConfirmationPolicy,
    pub description: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct McpServerManifest {
    pub schema_version: u8,
    pub transport_status: &'static str,
    pub scopes: Vec<&'static str>,
    pub summary: McpCatalogSummary,
    pub tools: Vec<McpToolManifestEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct McpCatalogSummary {
    pub tool_count: usize,
    pub by_family: BTreeMap<&'static str, usize>,
    pub by_scope: BTreeMap<&'static str, usize>,
    pub by_confirmation: BTreeMap<&'static str, usize>,
    pub by_runtime_mode: BTreeMap<&'static str, usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct McpToolManifestEntry {
    pub name: &'static str,
    pub family: McpToolFamily,
    pub required_scopes: Vec<&'static str>,
    pub confirmation: McpConfirmationPolicy,
    pub description: &'static str,
    pub runtime_modes: Vec<&'static str>,
    pub input_schema: Value,
    pub output_schema: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct McpToolExecutionRequest {
    #[serde(default)]
    pub input: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpToolExecutionResponse {
    pub tool: &'static str,
    pub read_only: bool,
    pub result: Value,
}

pub const MCP_READ_SCOPE: &[McpScope] = &[McpScope::NotesRead];
pub const MCP_WRITE_SCOPE: &[McpScope] = &[McpScope::NotesRead, McpScope::NotesWrite];
pub const MCP_DELETE_SCOPE: &[McpScope] = &[McpScope::NotesRead, McpScope::NotesDelete];
pub const MCP_EXPORT_SCOPE: &[McpScope] = &[McpScope::NotesRead, McpScope::NotesExport];
pub const MCP_IMPORT_SCOPE: &[McpScope] = &[McpScope::NotesRead, McpScope::NotesImport];
pub const MCP_ADMIN_SCOPE: &[McpScope] = &[McpScope::NotesRead, McpScope::NotesAdmin];
pub const MCP_SERVER_RUNTIME_MODES: &[&str] = &["server"];

pub const MCP_TOOL_CATALOG: &[McpToolDefinition] = &[
    McpToolDefinition {
        name: "starlane.spaces.list",
        family: McpToolFamily::Spaces,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "List spaces visible to the caller.",
    },
    McpToolDefinition {
        name: "starlane.spaces.get",
        family: McpToolFamily::Spaces,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "Get space metadata and structure.",
    },
    McpToolDefinition {
        name: "starlane.folders.list",
        family: McpToolFamily::Folders,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "List folders in a readable space.",
    },
    McpToolDefinition {
        name: "starlane.documents.get",
        family: McpToolFamily::Documents,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "Read a document and its metadata.",
    },
    McpToolDefinition {
        name: "starlane.documents.create",
        family: McpToolFamily::Documents,
        required_scopes: MCP_WRITE_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "Create a document draft.",
    },
    McpToolDefinition {
        name: "starlane.documents.update",
        family: McpToolFamily::Documents,
        required_scopes: MCP_WRITE_SCOPE,
        confirmation: McpConfirmationPolicy::RequiredForBulk,
        description: "Update document metadata or content.",
    },
    McpToolDefinition {
        name: "starlane.documents.delete",
        family: McpToolFamily::Documents,
        required_scopes: MCP_DELETE_SCOPE,
        confirmation: McpConfirmationPolicy::Required,
        description: "Delete a document.",
    },
    McpToolDefinition {
        name: "starlane.search.query",
        family: McpToolFamily::Search,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "Run full-text or Dataview-style searches.",
    },
    McpToolDefinition {
        name: "starlane.graph.get",
        family: McpToolFamily::Graph,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "Read graph data for a space.",
    },
    McpToolDefinition {
        name: "starlane.tags.list",
        family: McpToolFamily::Metadata,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "List tags in a readable space.",
    },
    McpToolDefinition {
        name: "starlane.properties.list",
        family: McpToolFamily::Metadata,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "List property schemas in a readable space.",
    },
    McpToolDefinition {
        name: "starlane.templates.list",
        family: McpToolFamily::Templates,
        required_scopes: MCP_READ_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "List document templates in a readable space.",
    },
    McpToolDefinition {
        name: "starlane.workflows.record_event",
        family: McpToolFamily::Workflows,
        required_scopes: MCP_WRITE_SCOPE,
        confirmation: McpConfirmationPolicy::RequiredForBulk,
        description: "Record workflow delegation, escalation, or transition events.",
    },
    McpToolDefinition {
        name: "starlane.export.snapshot",
        family: McpToolFamily::ImportsExports,
        required_scopes: MCP_EXPORT_SCOPE,
        confirmation: McpConfirmationPolicy::Required,
        description: "Export a vault or server-space snapshot.",
    },
    McpToolDefinition {
        name: "starlane.import.preview",
        family: McpToolFamily::ImportsExports,
        required_scopes: MCP_IMPORT_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "Preview an import plan without writing data.",
    },
    McpToolDefinition {
        name: "starlane.import.execute",
        family: McpToolFamily::ImportsExports,
        required_scopes: MCP_IMPORT_SCOPE,
        confirmation: McpConfirmationPolicy::Required,
        description: "Execute an import after preview.",
    },
    McpToolDefinition {
        name: "starlane.admin.users.list",
        family: McpToolFamily::Admin,
        required_scopes: MCP_ADMIN_SCOPE,
        confirmation: McpConfirmationPolicy::None,
        description: "List users for admin automation.",
    },
    McpToolDefinition {
        name: "starlane.admin.audit.export",
        family: McpToolFamily::Admin,
        required_scopes: MCP_ADMIN_SCOPE,
        confirmation: McpConfirmationPolicy::Required,
        description: "Export audit records.",
    },
];

pub fn find_mcp_tool(name: &str) -> Option<&'static McpToolDefinition> {
    MCP_TOOL_CATALOG.iter().find(|tool| tool.name == name)
}

pub fn mcp_scope_tokens() -> Vec<&'static str> {
    [
        McpScope::NotesRead,
        McpScope::NotesWrite,
        McpScope::NotesDelete,
        McpScope::NotesAdmin,
        McpScope::NotesExport,
        McpScope::NotesImport,
    ]
    .into_iter()
    .map(McpScope::as_str)
    .collect()
}

pub fn mcp_server_manifest() -> McpServerManifest {
    McpServerManifest {
        schema_version: 1,
        transport_status: "read_only_http_tools",
        scopes: mcp_scope_tokens(),
        summary: mcp_catalog_summary(),
        tools: MCP_TOOL_CATALOG
            .iter()
            .map(|tool| McpToolManifestEntry {
                name: tool.name,
                family: tool.family,
                required_scopes: tool
                    .required_scopes
                    .iter()
                    .map(|scope| scope.as_str())
                    .collect(),
                confirmation: tool.confirmation,
                description: tool.description,
                runtime_modes: mcp_tool_runtime_modes(tool.name),
                input_schema: mcp_tool_input_schema(tool.name),
                output_schema: mcp_tool_output_schema(tool.name),
            })
            .collect(),
    }
}

pub fn mcp_catalog_summary() -> McpCatalogSummary {
    let mut by_family = BTreeMap::new();
    let mut by_scope = BTreeMap::new();
    let mut by_confirmation = BTreeMap::new();
    let mut by_runtime_mode = BTreeMap::new();

    for tool in MCP_TOOL_CATALOG {
        *by_family.entry(tool.family.as_str()).or_insert(0) += 1;
        *by_confirmation
            .entry(tool.confirmation.as_str())
            .or_insert(0) += 1;
        for runtime_mode in mcp_tool_runtime_modes(tool.name) {
            *by_runtime_mode.entry(runtime_mode).or_insert(0) += 1;
        }
        for scope in tool.required_scopes {
            *by_scope.entry(scope.as_str()).or_insert(0) += 1;
        }
    }

    McpCatalogSummary {
        tool_count: MCP_TOOL_CATALOG.len(),
        by_family,
        by_scope,
        by_confirmation,
        by_runtime_mode,
    }
}

pub fn mcp_tool_runtime_modes(name: &str) -> Vec<&'static str> {
    if find_mcp_tool(name).is_none() {
        return Vec::new();
    }

    MCP_SERVER_RUNTIME_MODES.to_vec()
}

pub fn mcp_tool_input_schema(name: &str) -> Value {
    match name {
        "starlane.spaces.list" => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        "starlane.spaces.get" => object_schema([("space_id", string_schema("uuid"))], ["space_id"]),
        "starlane.folders.list" => {
            object_schema([("space_id", string_schema("uuid"))], ["space_id"])
        }
        "starlane.documents.get" => object_schema(
            [
                ("space_id", string_schema("uuid")),
                ("document_id", string_schema("uuid")),
            ],
            ["space_id", "document_id"],
        ),
        "starlane.documents.create" => object_schema(
            [
                ("space_id", string_schema("uuid")),
                ("folder_id", nullable_string_schema("uuid")),
                ("title", json!({ "type": "string", "minLength": 1 })),
                ("content", json!({ "type": "string" })),
                (
                    "tags",
                    json!({ "type": "array", "items": { "type": "string" } }),
                ),
                (
                    "properties",
                    json!({ "type": "object", "additionalProperties": { "type": "string" } }),
                ),
            ],
            ["space_id", "title"],
        ),
        "starlane.documents.update" => object_schema(
            [
                ("space_id", string_schema("uuid")),
                ("document_id", string_schema("uuid")),
                ("title", json!({ "type": "string", "minLength": 1 })),
                ("content", json!({ "type": "string" })),
                ("folder_id", nullable_string_schema("uuid")),
                ("expected_updated_at", string_schema("date-time")),
            ],
            ["space_id", "document_id"],
        ),
        "starlane.documents.delete" => object_schema(
            [
                ("space_id", string_schema("uuid")),
                ("document_id", string_schema("uuid")),
                (
                    "hard_delete",
                    json!({ "type": "boolean", "default": false }),
                ),
            ],
            ["space_id", "document_id"],
        ),
        "starlane.search.query" => object_schema(
            [
                ("q", json!({ "type": "string", "minLength": 1 })),
                ("space_id", string_schema("uuid")),
                (
                    "limit",
                    json!({ "type": "integer", "minimum": 1, "maximum": 100 }),
                ),
            ],
            ["q"],
        ),
        "starlane.graph.get" => object_schema([("space_id", string_schema("uuid"))], ["space_id"]),
        "starlane.tags.list" => object_schema([("space_id", string_schema("uuid"))], ["space_id"]),
        "starlane.properties.list" => {
            object_schema([("space_id", string_schema("uuid"))], ["space_id"])
        }
        "starlane.templates.list" => {
            object_schema([("space_id", string_schema("uuid"))], ["space_id"])
        }
        "starlane.workflows.record_event" => object_schema(
            [
                ("space_id", string_schema("uuid")),
                ("document_id", string_schema("uuid")),
                (
                    "event_type",
                    json!({ "type": "string", "enum": ["transition", "delegated", "escalated", "comment"] }),
                ),
                ("target_stage_id", nullable_string_schema("uuid")),
                ("target_user_id", nullable_string_schema("uuid")),
                ("note", json!({ "type": "string" })),
                ("due_at", nullable_string_schema("date-time")),
            ],
            ["space_id", "document_id", "event_type"],
        ),
        "starlane.export.snapshot" => object_schema(
            [
                (
                    "scope",
                    json!({ "type": "string", "enum": ["local_vault", "server_space"] }),
                ),
                ("space_id", nullable_string_schema("uuid")),
                (
                    "include_assets",
                    json!({ "type": "boolean", "default": true }),
                ),
            ],
            ["scope"],
        ),
        "starlane.import.preview" => {
            object_schema([("snapshot", json!({ "type": "object" }))], ["snapshot"])
        }
        "starlane.import.execute" => object_schema(
            [
                ("snapshot", json!({ "type": "object" })),
                (
                    "conflict_strategy",
                    json!({ "type": "string", "enum": ["skip_existing", "update_existing"] }),
                ),
                ("confirmed_preview_id", json!({ "type": "string" })),
            ],
            ["snapshot", "confirmed_preview_id"],
        ),
        "starlane.admin.users.list" => json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "role": { "type": "string", "enum": ["admin", "member"] },
                "active": { "type": "boolean" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 100 }
            },
            "additionalProperties": false
        }),
        "starlane.admin.audit.export" => object_schema(
            [
                ("since", string_schema("date-time")),
                ("until", string_schema("date-time")),
                ("actor_user_id", nullable_string_schema("uuid")),
                ("action", json!({ "type": "string" })),
                (
                    "format",
                    json!({ "type": "string", "enum": ["json", "csv"] }),
                ),
            ],
            [],
        ),
        _ => json!({
            "type": "object",
            "description": "Tool input schema is reserved for the runtime implementation.",
            "additionalProperties": true
        }),
    }
}

pub fn mcp_tool_output_schema(name: &str) -> Value {
    match name {
        "starlane.spaces.list" => array_schema(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "name": { "type": "string" },
                "slug": { "type": "string" },
                "visibility": { "type": "string" }
            },
            "required": ["id", "name", "slug"]
        })),
        "starlane.spaces.get" => json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "name": { "type": "string" },
                "slug": { "type": "string" },
                "folders": { "type": "array", "items": { "type": "object" } }
            },
            "required": ["id", "name", "slug"]
        }),
        "starlane.folders.list" => array_schema(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "space_id": { "type": "string", "format": "uuid" },
                "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                "name": { "type": "string" },
                "icon": { "type": "string", "nullable": true },
                "sort_order": { "type": "integer" },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" }
            },
            "required": ["id", "space_id", "name", "sort_order", "created_at", "updated_at"]
        })),
        "starlane.documents.get" => json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "space_id": { "type": "string", "format": "uuid" },
                "title": { "type": "string" },
                "content": { "type": "string" },
                "updated_at": { "type": "string", "format": "date-time" }
            },
            "required": ["id", "space_id", "title", "content"]
        }),
        "starlane.documents.create" | "starlane.documents.update" => document_output_schema(),
        "starlane.documents.delete" => json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["deleted"] },
                "document_id": { "type": "string", "format": "uuid" }
            },
            "required": ["status", "document_id"]
        }),
        "starlane.search.query" => array_schema(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "space_id": { "type": "string", "format": "uuid" },
                "title": { "type": "string" },
                "excerpt": { "type": "string" }
            },
            "required": ["id", "space_id", "title"]
        })),
        "starlane.graph.get" => json!({
            "type": "object",
            "properties": {
                "nodes": { "type": "array", "items": { "type": "object" } },
                "edges": { "type": "array", "items": { "type": "object" } }
            },
            "required": ["nodes", "edges"]
        }),
        "starlane.tags.list" => array_schema(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "name": { "type": "string" },
                "color": { "type": "string" },
                "created_at": { "type": "string", "format": "date-time", "nullable": true },
                "doc_count": { "type": "integer" }
            },
            "required": ["id", "name", "color"]
        })),
        "starlane.properties.list" => array_schema(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "name": { "type": "string" },
                "property_type": { "type": "string" },
                "options": { "type": "object", "nullable": true },
                "sort_order": { "type": "integer" }
            },
            "required": ["id", "name", "property_type", "sort_order"]
        })),
        "starlane.templates.list" => array_schema(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "space_id": { "type": "string", "format": "uuid" },
                "name": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "content": { "type": "string" },
                "properties": { "type": "object", "nullable": true },
                "tags": { "type": "array", "items": { "type": "string" } },
                "created_by": { "type": "string", "format": "uuid" },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" }
            },
            "required": ["id", "space_id", "name", "content", "tags", "created_by", "created_at", "updated_at"]
        })),
        "starlane.workflows.record_event" => json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "document_id": { "type": "string", "format": "uuid" },
                "event_type": { "type": "string" },
                "actor_user_id": { "type": "string", "format": "uuid" },
                "created_at": { "type": "string", "format": "date-time" }
            },
            "required": ["id", "document_id", "event_type", "actor_user_id", "created_at"]
        }),
        "starlane.export.snapshot" => json!({
            "type": "object",
            "properties": {
                "schema_version": { "type": "integer" },
                "space_count": { "type": "integer" },
                "document_count": { "type": "integer" },
                "asset_count": { "type": "integer" },
                "snapshot": { "type": "object" }
            },
            "required": ["schema_version", "space_count", "document_count", "asset_count", "snapshot"]
        }),
        "starlane.import.preview" => json!({
            "type": "object",
            "properties": {
                "space_count": { "type": "integer" },
                "document_count": { "type": "integer" },
                "warnings": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["space_count", "document_count", "warnings"]
        }),
        "starlane.import.execute" => json!({
            "type": "object",
            "properties": {
                "imported_space_count": { "type": "integer" },
                "imported_document_count": { "type": "integer" },
                "skipped_count": { "type": "integer" },
                "warnings": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["imported_space_count", "imported_document_count", "skipped_count", "warnings"]
        }),
        "starlane.admin.users.list" => array_schema(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "email": { "type": "string", "format": "email" },
                "display_name": { "type": "string" },
                "role": { "type": "string", "enum": ["admin", "member"] },
                "is_active": { "type": "boolean" }
            },
            "required": ["id", "email", "display_name", "role", "is_active"]
        })),
        "starlane.admin.audit.export" => json!({
            "type": "object",
            "properties": {
                "format": { "type": "string", "enum": ["json", "csv"] },
                "record_count": { "type": "integer" },
                "content_type": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["format", "record_count", "content_type", "content"]
        }),
        _ => json!({
            "type": "object",
            "description": "Tool output schema is reserved for the runtime implementation.",
            "additionalProperties": true
        }),
    }
}

pub fn mcp_tool_requires_confirmation(name: &str, affected_records: usize) -> bool {
    let Some(tool) = find_mcp_tool(name) else {
        return true;
    };

    match tool.confirmation {
        McpConfirmationPolicy::None => false,
        McpConfirmationPolicy::Required => true,
        McpConfirmationPolicy::RequiredForBulk => affected_records > 10,
    }
}

pub fn mcp_tool_allows_scopes(name: &str, granted_scopes: &[McpScope]) -> bool {
    let Some(tool) = find_mcp_tool(name) else {
        return false;
    };
    tool.required_scopes
        .iter()
        .all(|scope| granted_scopes.contains(scope))
}

pub fn mcp_tool_is_executable_read_only(name: &str) -> bool {
    let Some(tool) = find_mcp_tool(name) else {
        return false;
    };
    tool.required_scopes == MCP_READ_SCOPE && tool.confirmation == McpConfirmationPolicy::None
}

pub fn mcp_tool_is_executable_http(name: &str) -> bool {
    let Some(tool) = find_mcp_tool(name) else {
        return false;
    };
    (tool.required_scopes == MCP_READ_SCOPE || tool.required_scopes == MCP_ADMIN_SCOPE)
        && tool.confirmation == McpConfirmationPolicy::None
}

async fn execute_spaces_list(state: &AppState, auth: &AuthUser) -> Result<Value, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT s.id, s.name, s.slug, s.description, s.icon, s.accent_color, s.visibility, \
         sm.role, \
         (SELECT sgp.role FROM space_group_permissions sgp \
          JOIN group_members gm ON gm.group_id = sgp.group_id \
          WHERE sgp.space_id = s.id AND gm.user_id = $1 \
          ORDER BY CASE sgp.role WHEN 'owner' THEN 1 WHEN 'editor' THEN 2 ELSE 3 END LIMIT 1), \
         s.created_at, s.updated_at \
         FROM spaces s \
         LEFT JOIN space_members sm ON s.id = sm.space_id AND sm.user_id = $1 \
         ORDER BY s.name",
    )
    .bind(&auth.claims.sub)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let mut current = Vec::new();
    for row in rows {
        if mcp_space_visible(&mut conn, auth, &row.0).await? {
            current.push(row);
        }
    }
    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    crate::spaces::access::require_current_human_read(&mut conn, auth).await?;
    let mut rows = Vec::new();
    for row in current {
        if mcp_space_visible(&mut conn, auth, &row.0).await? {
            rows.push(row);
        }
    }
    Ok(Value::Array(
        rows.into_iter()
            .map(
                |(
                    id,
                    name,
                    slug,
                    description,
                    icon,
                    accent_color,
                    visibility,
                    direct_role,
                    group_role,
                    created_at,
                    updated_at,
                )| {
                    json!({
                        "id": id,
                        "name": name,
                        "slug": slug,
                        "description": description,
                        "icon": icon,
                        "accent_color": accent_color,
                        "visibility": visibility,
                        "member_role": direct_role.or(group_role),
                        "created_at": created_at,
                        "updated_at": updated_at
                    })
                },
            )
            .collect(),
    ))
}

async fn execute_space_get(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    mcp_space_view(&mut conn, auth, space_id).await?;
    let space: Option<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, name, slug, description, icon, accent_color, visibility, created_at, updated_at \
         FROM spaces WHERE id = $1",
    )
    .bind(space_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let (id, name, slug, description, icon, accent_color, visibility, created_at, updated_at) =
        space.ok_or_else(|| AppError::NotFound("Space not found".into()))?;

    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    mcp_space_view(&mut conn, auth, space_id).await?;
    Ok(json!({
        "id": id,
        "name": name,
        "slug": slug,
        "description": description,
        "icon": icon,
        "accent_color": accent_color,
        "visibility": visibility,
        "created_at": created_at,
        "updated_at": updated_at
    }))
}

async fn execute_document_get(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space = required_input_string(input, "space_id")?;
    let document = required_input_string(input, "document_id")?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    mcp_page_view(&mut conn, auth, space, document).await?;
    let row:Option<(String,String,String,Option<String>,String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,title,folder_id,content_path,created_by,created_at,updated_at,published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(document).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let (id, space_id, title, folder_id, path, created_by, created_at, updated_at, published) =
        row.ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
    crate::spaces::documents::require_current_document_read_source(
        &mut conn,
        space,
        &id,
        &published,
        &path,
        &updated_at,
    )
    .await?;
    let content = storage::read_document(&path)
        .await
        .map_err(|_| AppError::NotFound("Document content unavailable".into()))?;
    #[cfg(test)]
    {
        let hook = MCP_AFTER_DOCUMENT_READ.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook {
            hook.await;
        }
    }
    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    mcp_page_view(&mut conn, auth, space, &id).await?;
    #[cfg(test)]
    {
        let hook = MCP_AFTER_FINAL_PAGE_VIEW.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook {
            hook.await;
        }
    }
    // Source-row equality does not retain member authority after the final wait.
    // Legacy View deliberately excludes unsupported guest MCP page access.
    mcp_page_view(&mut conn, auth, space, &id).await?;
    crate::spaces::documents::require_current_document_read_source(
        &mut conn,
        space,
        &id,
        &published,
        &path,
        &updated_at,
    )
    .await?;
    #[cfg(test)]
    {let hook=MCP_AFTER_FINAL_DOCUMENT_SOURCE.with(|slot|slot.borrow_mut().take());if let Some(hook)=hook {hook.await;}}
    crate::spaces::access::require_current_human_member_read(&mut conn, auth).await?;
    Ok(
        json!({"id":id,"space_id":space_id,"folder_id":folder_id,"title":title,"content":content,"created_by":created_by,"created_at":created_at,"updated_at":updated_at}),
    )
}

async fn execute_folders_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    mcp_space_view(&mut conn, auth, space_id).await?;

    let rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        i32,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, space_id, parent_id, name, icon, sort_order, created_at, updated_at \
             FROM folders WHERE space_id = $1 ORDER BY sort_order, name",
    )
    .bind(space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    mcp_space_view(&mut conn, auth, space_id).await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(
                |(id, space_id, parent_id, name, icon, sort_order, created_at, updated_at)| {
                    json!({
                        "id": id,
                        "space_id": space_id,
                        "parent_id": parent_id,
                        "name": name,
                        "icon": icon,
                        "sort_order": sort_order,
                        "created_at": created_at,
                        "updated_at": updated_at
                    })
                },
            )
            .collect(),
    ))
}

async fn execute_search_query(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let q = required_input_string(input, "q")?.trim().to_owned();
    if q.len() > 500 {
        return Err(AppError::BadRequest(
            "MCP search query must be at most 500 characters".into(),
        ));
    }
    let limit = input
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 100) as usize;
    let query = crate::search::SearchQuery {
        q: Some(q),
        space_id: optional_input_string(input, "space_id").map(str::to_owned),
        limit: Some(limit),
        is_draft: Some(false),
        ..Default::default()
    };
    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    let hits = crate::search::search_visible_pages(state, auth, &query).await?;
    Ok(Value::Array(hits.into_iter().map(|hit|json!({"document_id":hit.document_id,"space_id":hit.space_id,"title":hit.title,"excerpt":hit.excerpt})).collect()))
}

async fn execute_graph_get(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space = required_input_string(input, "space_id")?;
    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    serde_json::to_value(crate::spaces::graph::read_space_graph(state, auth, space).await?)
        .map_err(AppError::internal)
}

async fn execute_tags_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space = required_input_string(input, "space_id")?;
    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    serde_json::to_value(crate::spaces::tags::read_tag_catalog(state, auth, space).await?)
        .map_err(AppError::internal)
}

async fn execute_properties_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    mcp_space_view(&mut conn, auth, space_id).await?;

    let rows: Vec<(String, String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, name, property_type, options, sort_order FROM properties_schema \
         WHERE space_id = $1 ORDER BY sort_order",
    )
    .bind(space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    mcp_space_view(&mut conn, auth, space_id).await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|(id, name, property_type, options, sort_order)| {
                json!({
                    "id": id,
                    "name": name,
                    "property_type": property_type,
                    "options": options.and_then(|value| serde_json::from_str::<Value>(&value).ok()),
                    "sort_order": sort_order
                })
            })
            .collect(),
    ))
}

async fn execute_templates_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    mcp_space_view(&mut conn, auth, space_id).await?;

    let rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, space_id, name, description, content, properties, tags, created_by, created_at, updated_at \
         FROM templates WHERE space_id = $1 ORDER BY name",
    )
    .bind(space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    mcp_space_view(&mut conn, auth, space_id).await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(
                |(
                    id,
                    space_id,
                    name,
                    description,
                    content,
                    properties,
                    tags,
                    created_by,
                    created_at,
                    updated_at,
                )| {
                    json!({
                        "id": id,
                        "space_id": space_id,
                        "name": name,
                        "description": description,
                        "content": content,
                        "properties": properties.and_then(|value| serde_json::from_str::<Value>(&value).ok()),
                        "tags": tags
                            .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
                            .unwrap_or_default(),
                        "created_by": created_by,
                        "created_at": created_at,
                        "updated_at": updated_at
                    })
                },
            )
            .collect(),
    ))
}

async fn execute_admin_users_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    require_mcp_current_read(state, auth, true).await?;
    let role_filter = optional_input_string(input, "role");
    if let Some(role) = role_filter {
        if !matches!(role, "admin" | "member") {
            return Err(AppError::BadRequest(
                "MCP admin user role filter must be admin or member".into(),
            ));
        }
    }
    let query_filter = optional_input_string(input, "query")
        .map(|query| query.to_lowercase())
        .filter(|query| !query.is_empty());
    let active_filter = input.get("active").and_then(Value::as_bool);
    let limit = input
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .clamp(1, 200) as i64;
    let query_like = query_filter.as_ref().map(|query| format!("%{}%", query));

    let rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        i32,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, email, display_name, avatar_url, role, is_active, created_at, updated_at \
             FROM users \
             WHERE ($1 IS NULL OR role = $1) \
               AND ($2 IS NULL OR is_active = $2) \
               AND ($3 IS NULL OR lower(email) LIKE $3 OR lower(display_name) LIKE $3) \
             ORDER BY email LIMIT $4",
    )
    .bind(role_filter)
    .bind(active_filter.map(|active| if active { 1 } else { 0 }))
    .bind(query_like)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    #[cfg(test)]
    before_mcp_final_resource_proof().await;
    require_mcp_current_read(state, auth, true).await?;
    Ok(Value::Array(
        rows.into_iter()
            .map(
                |(id, email, display_name, avatar_url, role, is_active, created_at, updated_at)| {
                    json!({
                        "id": id,
                        "email": email,
                        "display_name": display_name,
                        "avatar_url": avatar_url,
                        "role": role,
                        "is_active": is_active != 0,
                        "created_at": created_at,
                        "updated_at": updated_at
                    })
                },
            )
            .collect(),
    ))
}

async fn require_mcp_current_read(
    state: &AppState,
    auth: &AuthUser,
    admin: bool,
) -> Result<(), AppError> {
    use crate::spaces::access::{
        principal_from_auth, ApiKeyScope, CurrentKeyCapability, Principal,
    };
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let principal = principal_from_auth(auth)?;
    if let Principal::ServiceKey { key_id } = principal {
        if !admin {
            return Err(AppError::Unauthorized(
                "MCP human principal required".into(),
            ));
        }
        return match crate::spaces::access::resolve_current_key(&mut conn, key_id).await? {
            CurrentKeyCapability::Service(capability)
                if capability.scopes.contains(&ApiKeyScope::Admin) =>
            {
                Ok(())
            }
            _ => Err(AppError::Forbidden("Admin capability required".into())),
        };
    }
    let actor = crate::spaces::access::require_current_human_read(&mut conn, auth).await?;
    if admin {
        if let Principal::UserKey { key_id, .. } = principal {
            match crate::spaces::access::resolve_current_key(&mut conn, key_id).await? {
                CurrentKeyCapability::User(capability)
                    if capability.user_id == actor
                        && capability.scopes.contains(&ApiKeyScope::Admin) => {}
                _ => return Err(AppError::Forbidden("Admin API key scope required".into())),
            }
        }
        crate::spaces::access::require_current_human_instance_admin(&mut conn, auth)
            .await.map_err(|_| AppError::Forbidden("Admin access required".into()))?;
    }
    Ok(())
}
async fn mcp_space_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<(), AppError> {
    let access = crate::spaces::access::resolve_space_access(
        conn,
        crate::spaces::access::principal_from_auth(auth)?,
        space,
        crate::spaces::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(crate::spaces::access::Permission::View) {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(())
}
async fn mcp_space_visible(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<bool, AppError> {
    match mcp_space_view(conn, auth, space).await {
        Ok(()) => Ok(true),
        Err(AppError::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
}
async fn mcp_page_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    document: &str,
) -> Result<(), AppError> {
    crate::spaces::access::resolve_page_access(
        conn,
        crate::spaces::access::principal_from_auth(auth)?,
        space,
        document,
        crate::spaces::access::PageOperation::View,
        crate::spaces::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}

fn required_input_string<'a>(input: &'a Value, key: &str) -> Result<&'a str, AppError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest(format!("Missing required MCP input: {}", key)))
}

fn optional_input_string<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn object_schema<const N: usize, const R: usize>(
    properties: [(&'static str, Value); N],
    required: [&'static str; R],
) -> Value {
    let properties = properties
        .into_iter()
        .map(|(name, schema)| (name.to_string(), schema))
        .collect::<serde_json::Map<String, Value>>();
    let required = required.into_iter().collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn array_schema(item_schema: Value) -> Value {
    json!({
        "type": "array",
        "items": item_schema
    })
}

fn string_schema(format: &str) -> Value {
    json!({
        "type": "string",
        "format": format
    })
}

fn nullable_string_schema(format: &str) -> Value {
    json!({
        "type": "string",
        "format": format,
        "nullable": true
    })
}

fn document_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "space_id": { "type": "string", "format": "uuid" },
            "folder_id": { "type": "string", "format": "uuid", "nullable": true },
            "title": { "type": "string" },
            "content": { "type": "string" },
            "created_at": { "type": "string", "format": "date-time" },
            "updated_at": { "type": "string", "format": "date-time" }
        },
        "required": ["id", "space_id", "title", "content", "created_at", "updated_at"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::jwt::{Claims, TokenType};
    use axum::extract::{Path, State};
    use serde_json::json;
    use sqlx::any::AnyPoolOptions;
    use std::collections::HashSet;

    async fn mcp_execution_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        let index_path =
            std::env::temp_dir().join(format!("starlane-notes-mcp-test-{}", uuid::Uuid::new_v4()));

        AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path).expect("search index"),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
            ws_rooms: crate::sync::WsRooms::default(),
        }
    }

    fn mcp_auth_user(role: &str, api_key_scopes: Option<Vec<&str>>) -> AuthUser {
        AuthUser {
            claims: Claims {
                sub: "user-1".to_string(),
                sid: "test-session".to_string(),
                email: "user@example.test".to_string(),
                role: role.to_string(),
                exp: 4_102_444_800,
                iat: 1_783_542_000,
                token_type: TokenType::Access,
            },
            api_key_scopes: api_key_scopes
                .map(|scopes| scopes.into_iter().map(str::to_string).collect()),
        }
    }

    async fn current_mcp_state() -> AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let slug = format!("mcp-read-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let path = crate::storage::write_document(
            &slug,
            "target-a",
            "Visible target",
            "",
            "MCP searchable nebula",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE documents SET title='Visible target',content_path=$1 WHERE id='target-a'",
        )
        .bind(path)
        .execute(&state.db)
        .await
        .unwrap();
        state
            .search_index
            .upsert(crate::search::index::DocEntry {
                doc_id: "target-a".into(),
                space_id: "space-a".into(),
                title: "Visible target".into(),
                body: "MCP searchable nebula".into(),
                tags: String::new(),
            })
            .unwrap();
        sqlx::query("INSERT INTO tags(id,space_id,name) VALUES('mcp-tag','space-a','Tag')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES('target-a','mcp-tag')")
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    async fn current_mcp_key(
        state: &AppState,
        id: &str,
        user: Option<&str>,
        scopes: &str,
    ) -> String {
        let key = format!("plaintext-{id}");
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,'MCP',$3,'prefix',$4,$5)").bind(id).bind(user).bind(crate::auth::api_keys::hash_api_key(&key)).bind(if user.is_none(){1_i64}else{0}).bind(scopes).execute(&state.db).await.unwrap();
        key
    }
    async fn call_mcp(
        state: &AppState,
        key: &str,
        tool: &str,
        input: Value,
    ) -> (axum::http::StatusCode, Value) {
        use tower::ServiceExt;
        let response = super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/mcp/tools/{tool}"))
                    .header("X-API-Key", key)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(json!({"input":input}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }
    #[tokio::test]
    async fn current_router_admits_read_keys_for_all_enabled_human_tools_and_retains_write_gate() {
        let state = current_mcp_state().await;
        let read = current_mcp_key(&state, "read", Some("viewer-a"), "[\"read\"]").await;
        for (tool, input) in [
            ("starlane.spaces.list", json!({})),
            ("starlane.spaces.get", json!({"space_id":"space-a"})),
            ("starlane.folders.list", json!({"space_id":"space-a"})),
            (
                "starlane.documents.get",
                json!({"space_id":"space-a","document_id":"target-a"}),
            ),
            ("starlane.graph.get", json!({"space_id":"space-a"})),
            ("starlane.tags.list", json!({"space_id":"space-a"})),
            ("starlane.properties.list", json!({"space_id":"space-a"})),
            ("starlane.templates.list", json!({"space_id":"space-a"})),
            (
                "starlane.search.query",
                json!({"q":"nebula","space_id":"space-a"}),
            ),
        ] {
            let (status, body) = call_mcp(&state, &read, tool, input).await;
            assert_eq!(status, axum::http::StatusCode::OK, "{tool}: {body}");
            assert_eq!(body["tool"], tool);
            assert_eq!(body["read_only"], true);
        }
        let (status, body) = call_mcp(
            &state,
            &read,
            "starlane.documents.get",
            json!({"space_id":"space-a","document_id":"target-a"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(body["result"]["content"]
            .as_str()
            .unwrap()
            .contains("nebula"));
        assert_eq!(body["result"]["title"], "Visible target");
        let (_, hits) = call_mcp(
            &state,
            &read,
            "starlane.search.query",
            json!({"q":"nebula","space_id":"space-a"}),
        )
        .await;
        assert_eq!(hits["result"].as_array().unwrap().len(), 1);
        assert_eq!(hits["result"][0]["title"], "Visible target");
        assert!(hits["result"][0]["excerpt"]
            .as_str()
            .unwrap()
            .contains("nebula"));
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        let peer =
            crate::storage::write_document(&slug, "visible-peer", "Peer", "", "/docs/target-a")
                .await
                .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('visible-peer','space-a','Peer',$1,'owner-a','owner-a','published')").bind(peer).execute(&state.db).await.unwrap();
        let (_, graph) = call_mcp(
            &state,
            &read,
            "starlane.graph.get",
            json!({"space_id":"space-a"}),
        )
        .await;
        assert_eq!(graph["result"]["edges"].as_array().unwrap().len(), 1);
        assert!(graph["result"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|node| node["connection_count"] == 1 && node["orphan"] == false));
        let write = current_mcp_key(&state, "write", Some("owner-a"), "[\"write\"]").await;
        for key in [&read, &write] {
            assert_eq!(
                call_mcp(
                    &state,
                    key,
                    "starlane.documents.create",
                    json!({"space_id":"space-a","title":"Deferred"})
                )
                .await
                .0,
                axum::http::StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            call_mcp(
                &state,
                &write,
                "starlane.documents.get",
                json!({"space_id":"space-a","document_id":"target-a"})
            )
            .await
            .0,
            axum::http::StatusCode::FORBIDDEN
        );
        assert_eq!(
            call_mcp(&state, &read, "starlane.unknown", json!({}))
                .await
                .0,
            axum::http::StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn ordinary_auth_routes_keep_method_key_ceiling_while_mcp_reads_use_explicit_admission() {
        use tower::ServiceExt;
        async fn normal(auth: AuthUser) -> String {
            auth.claims.sub
        }
        let state = current_mcp_state().await;
        let read = current_mcp_key(&state, "normal-read", Some("owner-a"), "[\"read\"]").await;
        let write = current_mcp_key(&state, "normal-write", Some("owner-a"), "[\"write\"]").await;
        let app = axum::Router::new()
            .route("/normal", axum::routing::get(normal).post(normal))
            .with_state(state.clone());
        for (key, method, expected) in [
            (&read, "GET", 200),
            (&read, "POST", 403),
            (&write, "GET", 403),
            (&write, "POST", 200),
        ] {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri("/normal")
                        .header("X-API-Key", key)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
        }
        let service = current_mcp_key(&state, "normal-service", None, "[\"admin\"]").await;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/normal")
                    .header("X-API-Key", service)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1000)
            .await
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            "service:normal-service"
        );
    }

    #[tokio::test]
    async fn current_router_page_reads_counts_and_catalogs_hide_private_ancestors_and_blocks() {
        let state = current_mcp_state().await;
        let read = current_mcp_key(&state, "private-read", Some("viewer-a"), "[\"read\"]").await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('hidden-parent','space-a','Hidden parent','','owner-a','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='hidden-parent' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            call_mcp(
                &state,
                &read,
                "starlane.documents.get",
                json!({"space_id":"space-a","document_id":"target-a"})
            )
            .await
            .0,
            axum::http::StatusCode::NOT_FOUND
        );
        let (_, graph) = call_mcp(
            &state,
            &read,
            "starlane.graph.get",
            json!({"space_id":"space-a"}),
        )
        .await;
        assert_eq!(graph["result"]["nodes"], json!([]));
        let (_, tags) = call_mcp(
            &state,
            &read,
            "starlane.tags.list",
            json!({"space_id":"space-a"}),
        )
        .await;
        assert_eq!(tags["result"][0]["doc_count"], 0);
        let (_, hits) = call_mcp(
            &state,
            &read,
            "starlane.search.query",
            json!({"q":"nebula","space_id":"space-a"}),
        )
        .await;
        assert_eq!(hits["result"], json!([]));
        sqlx::query(
            "UPDATE documents SET parent_id=NULL,archived_at='archived' WHERE id='target-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            call_mcp(
                &state,
                &read,
                "starlane.documents.get",
                json!({"space_id":"space-a","document_id":"target-a"})
            )
            .await
            .0,
            axum::http::StatusCode::NOT_FOUND
        );
        sqlx::query(
            "UPDATE documents SET archived_at=NULL,deleted_at='deleted' WHERE id='target-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            call_mcp(
                &state,
                &read,
                "starlane.documents.get",
                json!({"space_id":"space-a","document_id":"target-a"})
            )
            .await
            .0,
            axum::http::StatusCode::NOT_FOUND
        );
        sqlx::query("UPDATE spaces SET visibility='public' WHERE id='space-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            call_mcp(
                &state,
                &read,
                "starlane.spaces.get",
                json!({"space_id":"space-a"})
            )
            .await
            .0,
            axum::http::StatusCode::NOT_FOUND
        );
        let (_, spaces) = call_mcp(&state, &read, "starlane.spaces.list", json!({})).await;
        assert!(!spaces["result"].to_string().contains("space-a"));
    }
    #[tokio::test]
    async fn current_router_admin_requires_actual_human_or_explicit_service_admin_capability() {
        let state = current_mcp_state().await;
        let key = current_mcp_key(&state, "admin", Some("owner-a"), "[\"admin\"]").await;
        assert_eq!(
            call_mcp(&state, &key, "starlane.admin.users.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::FORBIDDEN
        );
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            call_mcp(&state, &key, "starlane.admin.users.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::OK
        );
        let read = current_mcp_key(&state, "admin-read", Some("owner-a"), "[\"read\"]").await;
        assert_eq!(
            call_mcp(&state, &read, "starlane.admin.users.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::FORBIDDEN
        );
        let service = current_mcp_key(&state, "service", None, "[\"admin\"]").await;
        assert_eq!(
            call_mcp(&state, &service, "starlane.admin.users.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::OK
        );
        assert_eq!(
            call_mcp(&state, &service, "starlane.spaces.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::UNAUTHORIZED
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='service'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            call_mcp(&state, &service, "starlane.admin.users.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn current_router_rechecks_key_expiry_downgrade_inactive_owner_and_global_role_provenance(
    ) {
        let state = current_mcp_state().await;
        let read = current_mcp_key(&state, "current-key", Some("viewer-a"), "[\"read\"]").await;
        sqlx::query("DELETE FROM space_members WHERE user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('opaque-read-role','ADMIN','[\"view\"]')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('mcp-global','MCP group')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('mcp-global-member','mcp-global','viewer-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('mcp-global','opaque-read-role')").execute(&state.db).await.unwrap();
        assert_eq!(
            call_mcp(
                &state,
                &read,
                "starlane.documents.get",
                json!({"space_id":"space-a","document_id":"target-a"})
            )
            .await
            .0,
            axum::http::StatusCode::OK
        );
        sqlx::query("UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id='current-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            call_mcp(&state, &read, "starlane.spaces.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::UNAUTHORIZED
        );
        sqlx::query(
            "UPDATE api_keys SET expires_at=NULL,scopes='[\"write\"]' WHERE id='current-key'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            call_mcp(&state, &read, "starlane.spaces.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::FORBIDDEN
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='current-key'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            call_mcp(&state, &read, "starlane.spaces.list", json!({}))
                .await
                .0,
            axum::http::StatusCode::UNAUTHORIZED
        );
    }

    async fn wal_mcp_state() -> AppState {
        let mut state = crate::spaces::properties::tests::property_test_state().await;
        let file = std::env::temp_dir().join(format!("mcp-wal-{}.db", uuid::Uuid::new_v4()));
        state.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(2)
            .after_connect(|conn, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&format!("sqlite://{}?mode=rwc", file.display()))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        for sql in [
            "INSERT INTO users(id,email,display_name,role) VALUES('owner-a','mcp-wal@example.test','Owner','member')",
            "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('metadata-owner-a','owner-a','hash','2999-01-01T00:00:00Z')",
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','Space','mcp-wal','owner-a')",
            "INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner','space-a','owner-a','owner')",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Target','','owner-a','owner-a','published')",
        ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
        let slug = format!("mcp-wal-{}", uuid::Uuid::new_v4());
        let path = crate::storage::write_document(
            &slug,
            "target-a",
            "Published target",
            "",
            "UNSHARED CANONICAL BODY",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(slug)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='target-a'")
            .bind(path)
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    #[tokio::test]
    async fn current_router_rechecks_external_key_revocation_and_document_source_after_io() {
        for change in [
            "DELETE FROM api_keys WHERE id='wait-key'",
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
        ] {
            let state = wal_mcp_state().await;
            let key = current_mcp_key(&state, "wait-key", Some("owner-a"), "[\"read\"]").await;
            let db = state.db.clone();
            super::MCP_AFTER_DOCUMENT_READ.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {sqlx::query(change).execute(&db).await.unwrap();if change.contains("published_at") {sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,content,base_updated_at) VALUES('own-draft','space-a','target-a','owner-a','Private','Readable different draft','base')").execute(&db).await.unwrap();}})));
            let (status, body) = call_mcp(
                &state,
                &key,
                "starlane.documents.get",
                json!({"space_id":"space-a","document_id":"target-a"}),
            )
            .await;
            assert!(!status.is_success(), "{change}: {body}");
            assert!(!body.to_string().contains("UNSHARED CANONICAL"));
        }
        let state = wal_mcp_state().await;
        let key = current_mcp_key(&state, "late-key", Some("owner-a"), "[\"read\"]").await;
        let db = state.db.clone();
        super::MCP_BEFORE_FINAL_RESPONSE.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='late-key'")
                    .execute(&db)
                    .await
                    .unwrap();
            }))
        });
        let (status, body) = call_mcp(&state, &key, "starlane.spaces.list", json!({})).await;
        assert!(!status.is_success(), "{body}");
        assert!(!body.to_string().contains("mcp-wal"));
    }
    #[tokio::test]
    async fn current_router_final_resource_proof_denies_external_block_and_unpublication() {
        for (tool, change) in [
            ("starlane.documents.get", "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')"),
            ("starlane.documents.get", "UPDATE documents SET published_at=NULL WHERE id='target-a'"),
            ("starlane.spaces.get", "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')"),
        ] {
            let state = wal_mcp_state().await;
            let key = current_mcp_key(&state, "final-key", Some("owner-a"), "[\"read\"]").await;
            let db = state.db.clone();
            super::MCP_BEFORE_FINAL_RESPONSE.with(|slot| *slot.borrow_mut()=Some(Box::pin(async move {
                sqlx::query(change).execute(&db).await.unwrap();
                if change.contains("published_at") {
                    sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,content,base_updated_at) VALUES('own-draft','space-a','target-a','owner-a','Private','Readable different draft','base')").execute(&db).await.unwrap();
                }
            })));
            let (status, body)=call_mcp(&state,&key,tool,json!({"space_id":"space-a","document_id":"target-a"})).await;
            assert!(!status.is_success(), "{tool}: {change}: {body}");
            assert!(!body.to_string().contains("UNSHARED CANONICAL"));
            assert!(!body.to_string().contains("mcp-wal"));
            // This regression uses an externally committed second connection,
            // not an in-transaction trigger or a rolled-back mutation.
            let changed:i64=sqlx::query_scalar(if change.contains("published_at") {"SELECT COUNT(*) FROM documents WHERE id='target-a' AND published_at IS NULL"} else {"SELECT COUNT(*) FROM space_user_blocks WHERE space_id='space-a' AND user_id='owner-a'"}).fetch_one(&state.db).await.unwrap();
            assert_eq!(changed,1);
        }
    }
    #[tokio::test]
    async fn guest_mcp_actual_router_late_classification_after_final_page_policy_denies_body() {
        let state=wal_mcp_state().await;
        let key=current_mcp_key(&state,"guest-final-key",Some("owner-a"),"[\"read\"]").await;
        let input=json!({"space_id":"space-a","document_id":"target-a"});
        let (status,body)=call_mcp(&state,&key,"starlane.documents.get",input.clone()).await;
        assert!(status.is_success(),"{body}");assert!(body.to_string().contains("UNSHARED CANONICAL"));
        let db=state.db.clone();super::MCP_AFTER_FINAL_PAGE_VIEW.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
            sqlx::query("UPDATE users SET account_kind='guest' WHERE id='owner-a'").execute(&db).await.unwrap();
        })));
        let (status,body)=call_mcp(&state,&key,"starlane.documents.get",input).await;
        assert!(!status.is_success(),"{body}");assert!(!body.to_string().contains("UNSHARED CANONICAL"));
        let kind:String=sqlx::query_scalar("SELECT account_kind FROM users WHERE id='owner-a'").fetch_one(&state.db).await.unwrap();assert_eq!(kind,"guest");
    }

    #[tokio::test]
    async fn guest_mcp_stored_admin_kind_cannot_expose_users_service_admin_unchanged() {
        let state=current_mcp_state().await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'").execute(&state.db).await.unwrap();
        let key=current_mcp_key(&state,"guest-admin-kind",Some("owner-a"),"[\"admin\"]").await;
        assert_eq!(call_mcp(&state,&key,"starlane.admin.users.list",json!({})).await.0,axum::http::StatusCode::OK);
        sqlx::query("UPDATE users SET account_kind='guest' WHERE id='owner-a'").execute(&state.db).await.unwrap();
        let (status,body)=call_mcp(&state,&key,"starlane.admin.users.list",json!({})).await;assert!(!status.is_success(),"{body}");
        let service=current_mcp_key(&state,"guest-ordinary-service",None,"[\"admin\"]").await;
        assert_eq!(call_mcp(&state,&service,"starlane.admin.users.list",json!({})).await.0,axum::http::StatusCode::OK);
        assert!(!call_mcp(&state,&service,"starlane.documents.get",json!({"space_id":"space-a","document_id":"target-a"})).await.0.is_success());
    }
    #[tokio::test]
    async fn guest_mcp_actual_router_final_source_wait_classification_denies_body() {
        let state=wal_mcp_state().await;let key=current_mcp_key(&state,"guest-source-key",Some("owner-a"),"[\"read\"]").await;
        let input=json!({"space_id":"space-a","document_id":"target-a"});
        let (status,body)=call_mcp(&state,&key,"starlane.documents.get",input.clone()).await;assert!(status.is_success(),"{body}");assert!(body.to_string().contains("UNSHARED CANONICAL"));
        let db=state.db.clone();super::MCP_AFTER_FINAL_DOCUMENT_SOURCE.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {sqlx::query("UPDATE users SET account_kind='guest' WHERE id='owner-a'").execute(&db).await.unwrap();})));
        let (status,body)=call_mcp(&state,&key,"starlane.documents.get",input).await;assert!(!status.is_success(),"{body}");assert!(!body.to_string().contains("UNSHARED CANONICAL"));
        let kind:String=sqlx::query_scalar("SELECT account_kind FROM users WHERE id='owner-a'").fetch_one(&state.db).await.unwrap();assert_eq!(kind,"guest");
    }
    #[tokio::test]
    async fn current_router_canonical_source_is_last_after_typed_page_wait() {
        let state = wal_mcp_state().await;
        let key = current_mcp_key(&state, "final-source-key", Some("owner-a"), "[\"read\"]").await;
        sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,content,base_updated_at) VALUES('own-draft','space-a','target-a','owner-a','Private','Readable different draft','base')").execute(&state.db).await.unwrap();
        let db = state.db.clone();
        super::MCP_AFTER_FINAL_PAGE_VIEW.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
                    .execute(&db)
                    .await
                    .unwrap();
            }))
        });
        let (status, body) = call_mcp(
            &state,
            &key,
            "starlane.documents.get",
            json!({"space_id":"space-a","document_id":"target-a"}),
        )
        .await;
        assert!(!status.is_success(), "{body}");
        assert!(!body.to_string().contains("UNSHARED CANONICAL"));
        let mut conn = state.db.acquire().await.unwrap();
        assert!(
            super::mcp_page_view(
                &mut conn,
                &crate::spaces::properties::tests::test_auth("owner-a"),
                "space-a",
                "target-a"
            )
            .await
            .is_ok(),
            "Own different draft still permits generic page View"
        );
        let published: Option<String> =
            sqlx::query_scalar("SELECT published_at FROM documents WHERE id='target-a'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert!(published.is_none());
    }
    #[tokio::test]
    async fn current_router_bearer_read_requires_current_real_session_and_instance_admin() {
        use tower::ServiceExt;
        let state = current_mcp_state().await;
        let token = crate::auth::jwt::create_token_pair_for_session(
            "owner-a",
            "owner-a@example.test",
            "admin",
            &state.jwt_secret,
            Some("metadata-owner-a"),
            30,
        )
        .unwrap()
        .access_token;
        let call = |tool: &str| {
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/mcp/tools/{tool}"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    json!({"input":{"space_id":"space-a","document_id":"target-a"}}).to_string(),
                ))
                .unwrap()
        };
        let app = super::router().with_state(state.clone());
        assert_eq!(
            app.clone()
                .oneshot(call("starlane.documents.get"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(call("starlane.admin.users.list"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::FORBIDDEN
        );
        let mut expired_claims = crate::spaces::properties::tests::test_auth("owner-a").claims;
        expired_claims.exp = chrono::Utc::now().timestamp() - 600;
        let expired = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &expired_claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp/tools/starlane.spaces.list")
                    .header("authorization", format!("Bearer {expired}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("{\"input\":{}}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        sqlx::query("DELETE FROM sessions WHERE id='metadata-owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(call("starlane.documents.get"))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn catalog_uses_stable_namespaced_tool_names() {
        let mut names = HashSet::new();
        for tool in MCP_TOOL_CATALOG {
            assert!(tool.name.starts_with("starlane."));
            assert!(tool.name.matches('.').count() >= 2);
            assert!(names.insert(tool.name));
        }
    }

    #[test]
    fn confirmation_policy_matches_safety_boundaries() {
        assert!(!mcp_tool_requires_confirmation("starlane.documents.get", 1));
        assert!(mcp_tool_requires_confirmation(
            "starlane.documents.delete",
            1
        ));
        assert!(!mcp_tool_requires_confirmation(
            "starlane.documents.update",
            10
        ));
        assert!(mcp_tool_requires_confirmation(
            "starlane.documents.update",
            11
        ));
        assert!(mcp_tool_requires_confirmation("starlane.unknown", 1));
    }

    #[test]
    fn scope_checks_require_every_declared_scope() {
        assert!(mcp_tool_allows_scopes(
            "starlane.documents.get",
            &[McpScope::NotesRead]
        ));
        assert!(!mcp_tool_allows_scopes(
            "starlane.documents.create",
            &[McpScope::NotesRead]
        ));
        assert!(mcp_tool_allows_scopes(
            "starlane.documents.create",
            &[McpScope::NotesRead, McpScope::NotesWrite]
        ));
        assert!(!mcp_tool_allows_scopes(
            "starlane.admin.audit.export",
            &[McpScope::NotesRead, McpScope::NotesExport]
        ));
    }

    #[test]
    fn executable_tools_are_limited_to_read_only_catalog_entries() {
        assert!(mcp_tool_is_executable_read_only("starlane.spaces.list"));
        assert!(mcp_tool_is_executable_read_only("starlane.folders.list"));
        assert!(mcp_tool_is_executable_read_only("starlane.documents.get"));
        assert!(mcp_tool_is_executable_read_only("starlane.search.query"));
        assert!(mcp_tool_is_executable_read_only("starlane.graph.get"));
        assert!(mcp_tool_is_executable_read_only("starlane.tags.list"));
        assert!(mcp_tool_is_executable_read_only("starlane.properties.list"));
        assert!(mcp_tool_is_executable_read_only("starlane.templates.list"));
        assert!(mcp_tool_is_executable_http("starlane.admin.users.list"));
        assert!(!mcp_tool_is_executable_read_only(
            "starlane.admin.users.list"
        ));
        assert!(!mcp_tool_is_executable_read_only(
            "starlane.documents.create"
        ));
        assert!(!mcp_tool_is_executable_read_only(
            "starlane.export.snapshot"
        ));
        assert!(!mcp_tool_is_executable_http("starlane.admin.audit.export"));
        assert!(!mcp_tool_is_executable_read_only("starlane.unknown"));
    }

    #[tokio::test]
    async fn http_execution_rejects_unknown_mcp_tools() {
        let state = mcp_execution_test_state().await;
        let error = execute_mcp_tool(
            State(state),
            mcp_auth_user("member", None),
            Path("starlane.unknown".to_string()),
            Json(McpToolExecutionRequest { input: json!({}) }),
        )
        .await
        .expect_err("unknown tools should be rejected");

        assert!(matches!(error, AppError::BadRequest(message) if message == "Unknown MCP tool"));
    }

    #[tokio::test]
    async fn http_execution_rejects_deferred_write_tools() {
        let state = mcp_execution_test_state().await;
        let error = execute_mcp_tool(
            State(state),
            mcp_auth_user("admin", Some(vec!["admin"])),
            Path("starlane.documents.create".to_string()),
            Json(McpToolExecutionRequest {
                input: json!({
                    "space_id": "space-1",
                    "title": "Deferred write"
                }),
            }),
        )
        .await
        .expect_err("write tools should remain deferred for HTTP execution");

        assert!(
            matches!(error, AppError::Forbidden(message) if message.contains("read-only HTTP tools"))
        );
    }

    #[tokio::test]
    async fn http_admin_execution_requires_admin_role_and_api_key_scope() {
        let state = current_mcp_state().await;
        let auth = crate::spaces::properties::tests::test_auth("owner-a");
        assert!(matches!(
            require_mcp_current_read(&state, &auth, true).await,
            Err(AppError::Forbidden(_))
        ));
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let _key = current_mcp_key(&state, "scope-proof", Some("owner-a"), "[\"read\"]").await;
        let mut auth = crate::spaces::properties::tests::test_auth("owner-a");
        auth.claims.sid = "api-key:scope-proof".into();
        auth.api_key_scopes = Some(vec!["admin".into()]);
        assert!(matches!(
            require_mcp_current_read(&state, &auth, true).await,
            Err(AppError::Forbidden(_))
        ));
    }

    #[test]
    fn scope_names_match_documented_tokens() {
        assert_eq!(McpScope::NotesRead.as_str(), "notes:read");
        assert_eq!(McpScope::NotesWrite.as_str(), "notes:write");
        assert_eq!(McpScope::NotesDelete.as_str(), "notes:delete");
        assert_eq!(McpScope::NotesAdmin.as_str(), "notes:admin");
        assert_eq!(McpScope::NotesExport.as_str(), "notes:export");
        assert_eq!(McpScope::NotesImport.as_str(), "notes:import");
    }

    #[test]
    fn manifest_serializes_scope_tokens_and_tool_catalog() {
        let manifest = mcp_server_manifest();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.transport_status, "read_only_http_tools");
        assert!(manifest.scopes.contains(&"notes:read"));
        assert_eq!(manifest.tools.len(), MCP_TOOL_CATALOG.len());
        assert_eq!(manifest.summary.tool_count, MCP_TOOL_CATALOG.len());
        assert_eq!(manifest.summary.by_family["documents"], 4);
        assert_eq!(manifest.summary.by_confirmation["required"], 4);
        assert_eq!(
            manifest.summary.by_runtime_mode["server"],
            MCP_TOOL_CATALOG.len()
        );
        assert_eq!(
            manifest.summary.by_scope["notes:read"],
            MCP_TOOL_CATALOG.len()
        );
        assert!(manifest
            .tools
            .iter()
            .any(|tool| tool.name == "starlane.documents.get"
                && tool.required_scopes == vec!["notes:read"]
                && tool.runtime_modes == vec!["server"]
                && tool.input_schema["required"] == json!(["space_id", "document_id"])));

        let serialized = serde_json::to_string(&manifest).expect("manifest serializes");
        assert!(serialized.contains("notes:read"));
        assert!(serialized.contains("starlane.documents.get"));
        assert!(serialized.contains("input_schema"));
        assert!(serialized.contains("output_schema"));
        assert!(serialized.contains("runtime_modes"));
        assert!(serialized.contains("by_family"));
    }

    #[test]
    fn catalog_runtime_modes_are_explicitly_server_backed() {
        for tool in MCP_TOOL_CATALOG {
            assert_eq!(
                mcp_tool_runtime_modes(tool.name),
                vec!["server"],
                "{}",
                tool.name
            );
        }

        assert!(mcp_tool_runtime_modes("starlane.unknown").is_empty());
    }

    #[test]
    fn read_only_tools_have_concrete_io_schemas() {
        assert_eq!(
            mcp_tool_input_schema("starlane.spaces.get")["required"],
            json!(["space_id"])
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.folders.list")["items"]["required"][2],
            "name"
        );
        assert_eq!(
            mcp_tool_input_schema("starlane.search.query")["properties"]["limit"]["maximum"],
            100
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.graph.get")["required"],
            json!(["nodes", "edges"])
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.admin.users.list")["items"]["required"],
            json!(["id", "email", "display_name", "role", "is_active"])
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.tags.list")["items"]["required"],
            json!(["id", "name", "color"])
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.properties.list")["items"]["required"],
            json!(["id", "name", "property_type", "sort_order"])
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.templates.list")["items"]["required"][2],
            "name"
        );
    }

    #[test]
    fn write_import_export_and_admin_tools_have_concrete_io_schemas() {
        assert_eq!(
            mcp_tool_input_schema("starlane.documents.create")["required"],
            json!(["space_id", "title"])
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.documents.delete")["properties"]["status"]["enum"],
            json!(["deleted"])
        );
        assert_eq!(
            mcp_tool_input_schema("starlane.workflows.record_event")["properties"]["event_type"]
                ["enum"][1],
            "delegated"
        );
        assert_eq!(
            mcp_tool_input_schema("starlane.import.execute")["required"],
            json!(["snapshot", "confirmed_preview_id"])
        );
        assert_eq!(
            mcp_tool_output_schema("starlane.admin.audit.export")["required"],
            json!(["format", "record_count", "content_type", "content"])
        );
    }

    #[test]
    fn every_catalog_tool_has_concrete_manifest_schemas() {
        for tool in MCP_TOOL_CATALOG {
            assert_ne!(
                mcp_tool_input_schema(tool.name)["description"],
                "Tool input schema is reserved for the runtime implementation.",
                "{} input schema should be concrete",
                tool.name
            );
            assert_ne!(
                mcp_tool_output_schema(tool.name)["description"],
                "Tool output schema is reserved for the runtime implementation.",
                "{} output schema should be concrete",
                tool.name
            );
        }
    }
}
