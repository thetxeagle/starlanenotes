use axum::{
    extract::{Path, State},
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
        .route("/mcp/tools/{tool_name}", post(execute_mcp_tool))
}

pub async fn get_mcp_manifest(_auth: AuthUser) -> Json<McpServerManifest> {
    Json(mcp_server_manifest())
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
          WHERE sgp.space_id = s.id AND gm.user_id = ?1 \
          ORDER BY CASE sgp.role WHEN 'owner' THEN 1 WHEN 'editor' THEN 2 ELSE 3 END LIMIT 1), \
         s.created_at, s.updated_at \
         FROM spaces s \
         LEFT JOIN space_members sm ON s.id = sm.space_id AND sm.user_id = ?1 \
         WHERE sm.user_id IS NOT NULL \
            OR EXISTS (SELECT 1 FROM space_group_permissions sgp JOIN group_members gm ON gm.group_id = sgp.group_id WHERE sgp.space_id = s.id AND gm.user_id = ?1) \
            OR s.visibility IN ('internal', 'public') \
         ORDER BY s.name",
    )
    .bind(&auth.claims.sub)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

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
         FROM spaces WHERE id = ?1",
    )
    .bind(space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (id, name, slug, description, icon, accent_color, visibility, created_at, updated_at) =
        space.ok_or_else(|| AppError::NotFound("Space not found".into()))?;
    if visibility == "private" {
        crate::spaces::routes::require_space_role(
            state,
            space_id,
            &auth.claims.sub,
            &["owner", "editor", "viewer"],
        )
        .await?;
    }

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
    let space_id = required_input_string(input, "space_id")?;
    let document_id = required_input_string(input, "document_id")?;
    crate::spaces::routes::require_space_role(
        state,
        space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let row: Option<(
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, space_id, title, folder_id, content_path, created_by, created_at, updated_at \
         FROM documents WHERE id = ?1 AND space_id = ?2",
    )
    .bind(document_id)
    .bind(space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (id, space_id, title, folder_id, content_path, created_by, created_at, updated_at) =
        row.ok_or_else(|| AppError::NotFound("Document not found".into()))?;
    let content = storage::read_document(&content_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read document: {}", e)))?;

    Ok(json!({
        "id": id,
        "space_id": space_id,
        "folder_id": folder_id,
        "title": title,
        "content": content,
        "created_by": created_by,
        "created_at": created_at,
        "updated_at": updated_at
    }))
}

async fn execute_folders_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    require_mcp_readable_space(state, auth, space_id).await?;
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
             FROM folders WHERE space_id = ?1 ORDER BY sort_order, name",
    )
    .bind(space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

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
    let q = required_input_string(input, "q")?.trim().to_string();
    if q.is_empty() {
        return Err(AppError::BadRequest("MCP search query is required".into()));
    }
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
    let requested_space_id = optional_input_string(input, "space_id");
    if let Some(space_id) = requested_space_id {
        crate::spaces::routes::require_space_role(
            state,
            space_id,
            &auth.claims.sub,
            &["owner", "editor", "viewer"],
        )
        .await?;
    }

    let hits = state
        .search_index
        .search(&q, requested_space_id, limit)
        .map_err(|e| AppError::Internal(format!("Search error: {}", e)))?;
    let mut results = Vec::new();
    for hit in hits {
        if requested_space_id.is_none()
            && crate::spaces::routes::require_space_role(
                state,
                &hit.space_id,
                &auth.claims.sub,
                &["owner", "editor", "viewer"],
            )
            .await
            .is_err()
        {
            continue;
        }
        results.push(json!({
            "document_id": hit.doc_id,
            "space_id": hit.space_id,
            "title": hit.title,
            "excerpt": hit.excerpt
        }));
    }

    Ok(Value::Array(results))
}

async fn execute_graph_get(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    crate::spaces::routes::require_space_role(
        state,
        space_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;

    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, title, folder_id FROM documents WHERE space_id = ?1 ORDER BY title",
    )
    .bind(space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(json!({
        "nodes": rows
            .into_iter()
            .map(|(id, title, folder_id)| json!({
                "id": id,
                "title": title,
                "folder_id": folder_id,
                "connection_count": 0,
                "orphan": true
            }))
            .collect::<Vec<_>>(),
        "edges": []
    }))
}

async fn execute_tags_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    require_mcp_readable_space(state, auth, space_id).await?;
    let rows: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT t.id, t.name, t.color, t.created_at, COUNT(dt.document_id) as doc_count \
         FROM tags t LEFT JOIN doc_tags dt ON t.id = dt.tag_id \
         WHERE t.space_id = ?1 GROUP BY t.id, t.name, t.color, t.created_at ORDER BY t.name",
    )
    .bind(space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(Value::Array(
        rows.into_iter()
            .map(|(id, name, color, created_at, doc_count)| {
                json!({
                    "id": id,
                    "name": name,
                    "color": color,
                    "created_at": created_at,
                    "doc_count": doc_count
                })
            })
            .collect(),
    ))
}

async fn execute_properties_list(
    state: &AppState,
    auth: &AuthUser,
    input: &Value,
) -> Result<Value, AppError> {
    let space_id = required_input_string(input, "space_id")?;
    require_mcp_readable_space(state, auth, space_id).await?;
    let rows: Vec<(String, String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, name, property_type, options, sort_order FROM properties_schema \
         WHERE space_id = ?1 ORDER BY sort_order",
    )
    .bind(space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

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
    require_mcp_readable_space(state, auth, space_id).await?;
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
         FROM templates WHERE space_id = ?1 ORDER BY name",
    )
    .bind(space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

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
    require_mcp_admin(auth)?;
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
             WHERE (?1 IS NULL OR role = ?1) \
               AND (?2 IS NULL OR is_active = ?2) \
               AND (?3 IS NULL OR lower(email) LIKE ?3 OR lower(display_name) LIKE ?3) \
             ORDER BY email LIMIT ?4",
    )
    .bind(role_filter)
    .bind(active_filter.map(|active| if active { 1 } else { 0 }))
    .bind(query_like)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

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

async fn require_mcp_readable_space(
    state: &AppState,
    auth: &AuthUser,
    space_id: &str,
) -> Result<(), AppError> {
    crate::spaces::routes::require_space_role(
        state,
        space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await
    .map(|_| ())
}

fn require_mcp_admin(auth: &AuthUser) -> Result<(), AppError> {
    if auth.claims.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".into()));
    }
    if auth
        .api_key_scopes
        .as_ref()
        .is_some_and(|scopes| !scopes.iter().any(|scope| scope == "admin"))
    {
        return Err(AppError::Forbidden("Admin API key scope required".into()));
    }
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
        let member_state = mcp_execution_test_state().await;
        let member_error = execute_mcp_tool(
            State(member_state),
            mcp_auth_user("member", None),
            Path("starlane.admin.users.list".to_string()),
            Json(McpToolExecutionRequest { input: json!({}) }),
        )
        .await
        .expect_err("admin tools should reject non-admin users");
        assert!(
            matches!(member_error, AppError::Forbidden(message) if message == "Admin access required")
        );

        let scoped_state = mcp_execution_test_state().await;
        let scoped_error = execute_mcp_tool(
            State(scoped_state),
            mcp_auth_user("admin", Some(vec!["read"])),
            Path("starlane.admin.users.list".to_string()),
            Json(McpToolExecutionRequest { input: json!({}) }),
        )
        .await
        .expect_err("admin API keys should require the admin scope");
        assert!(
            matches!(scoped_error, AppError::Forbidden(message) if message == "Admin API key scope required")
        );
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
