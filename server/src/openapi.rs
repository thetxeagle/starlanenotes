use axum::{
    http::{header::CONTENT_TYPE, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use serde_json::{json, Value};
use utoipa_scalar::Scalar;

pub async fn scalar_docs() -> Html<String> {
    Html(Scalar::new(openapi_spec()).to_html())
}

pub async fn openapi_json() -> Json<Value> {
    Json(openapi_spec())
}

pub async fn openapi_yaml() -> impl IntoResponse {
    match serde_yaml::to_string(&openapi_spec()) {
        Ok(body) => ([(CONTENT_TYPE, "application/yaml")], body).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "openapi_yaml_failed",
                "message": error.to_string(),
            })),
        )
            .into_response(),
    }
}

pub fn openapi_spec() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Starlane Notes API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Self-hosted Starlane Notes REST API. This first-pass reference makes the current route surface discoverable while deeper request/response schemas continue to expand."
        },
        "servers": [
            { "url": "/api/v1", "description": "Versioned API base path" }
        ],
        "tags": [
            { "name": "Health" },
            { "name": "Authentication" },
            { "name": "Administration" },
            { "name": "Users and Groups" },
            { "name": "Spaces" },
            { "name": "Documents" },
            { "name": "Templates" },
            { "name": "Workflows" },
            { "name": "Metadata" },
            { "name": "Search" },
            { "name": "Graph" },
            { "name": "Webhooks" },
            { "name": "Notifications" },
            { "name": "MCP" },
            { "name": "Sync" },
            { "name": "AI" },
            { "name": "SCIM" }
        ],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                },
                "scimBearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Bearer token configured with SCIM_BEARER_TOKEN. This is separate from Starlane Notes JWT access tokens."
                }
            },
            "schemas": {
                "Error": {
                    "type": "object",
                    "properties": {
                        "error": { "type": "string" },
                        "message": { "type": "string" }
                    },
                    "required": ["error", "message"]
                },
                "IdResponse": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["id"]
                },
                "HealthResponse": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["ok"] },
                        "version": { "type": "string" }
                    },
                    "required": ["status", "version"]
                },
                "HealthUnavailableResponse": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["error"] },
                        "message": { "type": "string" }
                    },
                    "required": ["status", "message"]
                },
                "PasswordResetRequest": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email" }
                    },
                    "required": ["email"],
                    "additionalProperties": false
                },
                "PasswordResetRequestResponse": {
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"]
                },
                "PasswordResetConfirmRequest": {
                    "type": "object",
                    "properties": {
                        "token": { "type": "string", "minLength": 32, "maxLength": 128 },
                        "password": { "type": "string", "minLength": 8, "maxLength": 128 }
                    },
                    "required": ["token", "password"],
                    "additionalProperties": false
                },
                "Space": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "slug": { "type": "string" },
                        "description": { "type": "string", "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "accent_color": { "type": "string", "nullable": true },
                        "visibility": { "type": "string", "enum": ["private", "internal", "public"] },
                        "member_role": { "type": "string", "nullable": true },
                        "created_by": { "type": "string", "format": "uuid" },
                        "owner_username": { "type": "string", "format": "email" },
                        "owner_display_name": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "name", "slug", "description", "icon", "accent_color", "visibility", "member_role", "created_by", "owner_username", "owner_display_name", "created_at", "updated_at"]
                },
                "SpaceCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "description": { "type": "string", "maxLength": 1000, "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "accent_color": { "type": "string", "nullable": true },
                        "visibility": { "type": "string", "enum": ["private", "internal", "public"], "nullable": true },
                        "source_id": { "type": "string", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "updated_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name"]
                },
                "SpaceUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "maxLength": 100, "nullable": true },
                        "description": { "type": "string", "maxLength": 1000, "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "accent_color": { "type": "string", "nullable": true },
                        "visibility": { "type": "string", "enum": ["private", "internal", "public"], "nullable": true }
                    },
                    "minProperties": 1
                },
                "Folder": {
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
                    "required": ["id", "space_id", "parent_id", "name", "icon", "sort_order", "created_at", "updated_at"]
                },
                "FolderCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "source_id": { "type": "string", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "updated_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name"]
                },
                "FolderUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "maxLength": 100, "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true }
                    },
                    "minProperties": 1
                },
                "FolderMoveRequest": {
                    "type": "object",
                    "properties": {
                        "target_space_id": { "type": "string", "format": "uuid" },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true }
                    },
                    "required": ["target_space_id"]
                },
                "FolderReorderRequest": {
                    "type": "object",
                    "properties": {
                        "sort_order": { "type": "integer" }
                    },
                    "required": ["sort_order"]
                },
                "UserDirectoryEntry": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "email": { "type": "string", "format": "email" },
                        "display_name": { "type": "string" },
                        "avatar_url": { "type": "string", "nullable": true }
                    },
                    "required": ["id", "email", "display_name", "avatar_url"]
                },
                "SpaceMember": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" },
                        "email": { "type": "string", "format": "email" },
                        "display_name": { "type": "string" },
                        "avatar_url": { "type": "string", "nullable": true },
                        "role": { "type": "string", "enum": ["owner", "editor", "viewer"] },
                        "joined_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["user_id", "email", "display_name", "avatar_url", "role", "joined_at"]
                },
                "SpaceMemberRequest": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" },
                        "role": { "type": "string", "enum": ["owner", "editor", "viewer"], "nullable": true }
                    },
                    "required": ["user_id"]
                },
                "Group": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "description": { "type": "string", "nullable": true },
                        "member_count": { "type": "integer", "minimum": 0 },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "name", "description", "member_count", "created_at", "updated_at"]
                },
                "GroupCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "description": { "type": "string", "nullable": true }
                    },
                    "required": ["name"]
                },
                "GroupMember": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" },
                        "email": { "type": "string", "format": "email" },
                        "display_name": { "type": "string" },
                        "role": { "type": "string", "enum": ["admin", "member"] },
                        "joined_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["user_id", "email", "display_name", "role", "joined_at"]
                },
                "GroupMemberAddRequest": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["user_id"]
                },
                "SpaceGroupGrant": {
                    "type": "object",
                    "properties": {
                        "group_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "description": { "type": "string", "nullable": true },
                        "role": { "type": "string", "enum": ["owner", "editor", "viewer"] },
                        "member_count": { "type": "integer", "minimum": 0 },
                        "granted_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["group_id", "name", "description", "role", "member_count", "granted_at"]
                },
                "SpaceGroupGrantRequest": {
                    "type": "object",
                    "properties": {
                        "group_id": { "type": "string", "format": "uuid" },
                        "role": { "type": "string", "enum": ["owner", "editor", "viewer"], "nullable": true }
                    },
                    "required": ["group_id"]
                },
                "User": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "email": { "type": "string", "format": "email" },
                        "display_name": { "type": "string" },
                        "avatar_url": { "type": "string", "nullable": true },
                        "role": { "type": "string", "enum": ["admin", "member"] },
                        "is_active": { "type": "boolean" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "email", "display_name", "avatar_url", "role", "is_active", "created_at", "updated_at"]
                },
                "CurrentUserProfile": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "email": { "type": "string", "format": "email" },
                        "username": {
                            "type": "string",
                            "pattern": "^[A-Za-z0-9_-]{1,64}$",
                            "nullable": true
                        },
                        "display_name": { "type": "string" },
                        "avatar_url": { "type": "string", "maxLength": 500, "nullable": true },
                        "role": { "type": "string", "enum": ["admin", "member"] },
                        "totp_enabled": { "type": "boolean" }
                    },
                    "required": ["id", "email", "username", "display_name", "avatar_url", "role", "totp_enabled"]
                },
                "UpdateProfileRequest": {
                    "type": "object",
                    "properties": {
                        "display_name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "avatar_url": { "type": "string", "maxLength": 500, "nullable": true },
                        "username": {
                            "type": "string",
                            "pattern": "^[A-Za-z0-9_-]{1,64}$",
                            "nullable": true
                        }
                    },
                    "required": ["display_name"],
                    "additionalProperties": false
                },
                "RegisterRequest": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email", "maxLength": 254 },
                        "display_name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "password": { "type": "string", "minLength": 8, "maxLength": 128 }
                    },
                    "required": ["email", "display_name", "password"],
                    "additionalProperties": false
                },
                "LoginRequest": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email", "maxLength": 254 },
                        "password": { "type": "string", "minLength": 1, "maxLength": 128 },
                        "totp_code": { "type": "string", "nullable": true }
                    },
                    "required": ["email", "password"],
                    "additionalProperties": false
                },
                "RefreshTokenRequest": {
                    "type": "object",
                    "properties": {
                        "refresh_token": { "type": "string", "minLength": 1 }
                    },
                    "required": ["refresh_token"],
                    "additionalProperties": false
                },
                "TokenRefreshResponse": {
                    "type": "object",
                    "properties": {
                        "access_token": { "type": "string" },
                        "refresh_token": { "type": "string" },
                        "expires_in": { "type": "integer", "minimum": 1 }
                    },
                    "required": ["access_token", "refresh_token", "expires_in"]
                },
                "AuthResponse": {
                    "type": "object",
                    "properties": {
                        "access_token": { "type": "string" },
                        "refresh_token": { "type": "string" },
                        "expires_in": { "type": "integer", "minimum": 1 },
                        "user": { "$ref": "#/components/schemas/CurrentUserProfile" }
                    },
                    "required": ["access_token", "refresh_token", "expires_in", "user"]
                },
                "AuthConfigResponse": {
                    "type": "object",
                    "properties": {
                        "google_enabled": { "type": "boolean" },
                        "github_enabled": { "type": "boolean" },
                        "oidc_enabled": { "type": "boolean" },
                        "oidc_display_name": { "type": "string" },
                        "require_2fa": { "type": "boolean" },
                        "allow_public_signups": { "type": "boolean" },
                        "setup_completed": { "type": "boolean" },
                        "site_name": { "type": "string" },
                        "max_upload_size_mb": { "type": "integer", "minimum": 1 }
                    },
                    "required": [
                        "google_enabled",
                        "github_enabled",
                        "oidc_enabled",
                        "oidc_display_name",
                        "require_2fa",
                        "allow_public_signups",
                        "setup_completed",
                        "site_name",
                        "max_upload_size_mb"
                    ]
                },
                "NativeHandoffStartRequest": { "type": "object", "properties": { "code_challenge": { "type": "string", "minLength": 43, "maxLength": 128 } }, "required": ["code_challenge"], "additionalProperties": false },
                "NativeHandoffStartResponse": { "type": "object", "properties": { "handoff_id": { "type": "string", "format": "uuid" }, "authorization_url": { "type": "string", "format": "uri" } }, "required": ["handoff_id", "authorization_url"] },
                "NativeHandoffCompleteRequest": { "type": "object", "properties": { "handoff_id": { "type": "string", "format": "uuid" } }, "required": ["handoff_id"], "additionalProperties": false },
                "NativeHandoffCompleteResponse": { "type": "object", "properties": { "handoff_id": { "type": "string", "format": "uuid" }, "code": { "type": "string" } }, "required": ["handoff_id", "code"] },
                "NativeHandoffExchangeRequest": { "type": "object", "properties": { "handoff_id": { "type": "string", "format": "uuid" }, "code": { "type": "string" }, "code_verifier": { "type": "string", "minLength": 43, "maxLength": 128 } }, "required": ["handoff_id", "code", "code_verifier"], "additionalProperties": false },
                "TotpSetupResponse": {
                    "type": "object",
                    "properties": {
                        "qr_code_url": { "type": "string" },
                        "recovery_codes": {
                            "type": "array",
                            "items": { "type": "string", "pattern": "^[0-9]{4}-[0-9]{4}$" }
                        }
                    },
                    "required": ["qr_code_url", "recovery_codes"]
                },
                "TotpVerifyRequest": {
                    "type": "object",
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "Current 6-digit authenticator code, or one unused recovery code where accepted.",
                            "minLength": 6,
                            "maxLength": 16
                        }
                    },
                    "required": ["code"],
                    "additionalProperties": false
                },
                "TotpStatusResponse": {
                    "type": "object",
                    "properties": {
                        "totp_enabled": { "type": "boolean" }
                    },
                    "required": ["totp_enabled"]
                },
                "Passkey": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "last_used_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["id", "name", "created_at", "last_used_at"]
                },
                "PasskeyChallengeResponse": {
                    "type": "object",
                    "properties": {
                        "challenge_id": { "type": "string", "format": "uuid" },
                        "public_key": {
                            "type": "object",
                            "description": "WebAuthn publicKey credential creation/request options. Clients convert base64url fields to ArrayBuffers before calling navigator.credentials."
                        }
                    },
                    "required": ["challenge_id", "public_key"]
                },
                "PasskeyRegisterStartRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "maxLength": 80, "nullable": true }
                    },
                    "additionalProperties": false
                },
                "PasskeyRegisterFinishRequest": {
                    "type": "object",
                    "properties": {
                        "challenge_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string", "maxLength": 80, "nullable": true },
                        "credential": {
                            "type": "object",
                            "description": "JSON-serializable PublicKeyCredential response returned by navigator.credentials.create."
                        }
                    },
                    "required": ["challenge_id", "credential"],
                    "additionalProperties": false
                },
                "PasskeyLoginStartRequest": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email", "maxLength": 254 }
                    },
                    "required": ["email"],
                    "additionalProperties": false
                },
                "PasskeyLoginFinishRequest": {
                    "type": "object",
                    "properties": {
                        "challenge_id": { "type": "string", "format": "uuid" },
                        "credential": {
                            "type": "object",
                            "description": "JSON-serializable PublicKeyCredential assertion returned by navigator.credentials.get."
                        }
                    },
                    "required": ["challenge_id", "credential"],
                    "additionalProperties": false
                },
                "AdminUserCreateRequest": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "display_name": { "type": "string", "minLength": 1 },
                        "role": { "type": "string", "enum": ["admin", "member"], "default": "member" }
                    },
                    "required": ["email", "display_name"]
                },
                "AdminUserCreateResponse": {
                    "allOf": [
                        { "$ref": "#/components/schemas/User" },
                        {
                            "type": "object",
                            "properties": {
                                "temporary_password": { "type": "string" }
                            },
                            "required": ["temporary_password"]
                        }
                    ]
                },
                "AdminUserUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "role": { "type": "string", "enum": ["admin", "member"] },
                        "is_active": { "type": "boolean" }
                    },
                    "minProperties": 1,
                    "additionalProperties": false
                },
                "AdminUserBulkLifecycleRequest": {
                    "type": "object",
                    "properties": {
                        "user_ids": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 100,
                            "items": { "type": "string", "format": "uuid" }
                        },
                        "action": { "type": "string", "enum": ["activate", "deactivate"] }
                    },
                    "required": ["user_ids", "action"],
                    "additionalProperties": false
                },
                "AdminUserBulkLifecycleSkippedUser": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string" },
                        "reason": {
                            "type": "string",
                            "enum": [
                                "empty_user_id",
                                "duplicate_user_id",
                                "user_not_found",
                                "already_active",
                                "already_inactive",
                                "cannot_deactivate_self",
                                "last_active_admin_required"
                            ]
                        }
                    },
                    "required": ["user_id", "reason"]
                },
                "AdminUserBulkLifecycleResponse": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["activate", "deactivate"] },
                        "requested_count": { "type": "integer", "minimum": 0 },
                        "updated_count": { "type": "integer", "minimum": 0 },
                        "skipped_count": { "type": "integer", "minimum": 0 },
                        "skipped": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/AdminUserBulkLifecycleSkippedUser" }
                        }
                    },
                    "required": ["action", "requested_count", "updated_count", "skipped_count", "skipped"]
                },
                "AdminPasswordResetResponse": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "temporary_password": { "type": "string" }
                    },
                    "required": ["email", "temporary_password"]
                },
                "AdminConfigEntry": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" },
                        "label": { "type": "string" },
                        "value": { "type": "string" },
                        "effective_value": { "type": "string" },
                        "source": { "type": "string", "enum": ["env", "database", "default"] },
                        "env_var": { "type": "string", "nullable": true },
                        "locked": { "type": "boolean" },
                        "secret": { "type": "boolean" }
                    },
                    "required": ["key", "label", "value", "effective_value", "source", "env_var", "locked", "secret"]
                },
                "AdminConfigUpsertRequest": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "pattern": "^[a-z0-9._-]+$"
                        },
                        "value": { "type": "string" }
                    },
                    "required": ["key", "value"]
                },
                "SearchReindexResponse": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["reindex_complete"] },
                        "documents_indexed": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["status", "documents_indexed"]
                },
                "AdminUserImportPreviewUser": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "display_name": { "type": "string" },
                        "role": { "type": "string", "enum": ["admin", "member", "viewer"] },
                        "spaces": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["email", "display_name", "role", "spaces"]
                },
                "AdminUserImportSkippedUser": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "reason": { "type": "string" }
                    },
                    "required": ["email", "reason"]
                },
                "AdminUserImportError": {
                    "type": "object",
                    "properties": {
                        "line": { "type": "integer", "minimum": 1 },
                        "error": { "type": "string" }
                    },
                    "required": ["line", "error"]
                },
                "AdminUserImportCredential": {
                    "type": "object",
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "temporary_password": { "type": "string" }
                    },
                    "required": ["email", "temporary_password"]
                },
                "AdminUserImportPreview": {
                    "type": "object",
                    "properties": {
                        "total": { "type": "integer", "minimum": 0 },
                        "new_users": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/AdminUserImportPreviewUser" }
                        },
                        "skipped": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/AdminUserImportSkippedUser" }
                        },
                        "errors": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/AdminUserImportError" }
                        }
                    },
                    "required": ["total", "new_users", "skipped", "errors"]
                },
                "AdminUserImportResult": {
                    "type": "object",
                    "properties": {
                        "created": { "type": "integer", "minimum": 0 },
                        "skipped": { "type": "integer", "minimum": 0 },
                        "errors": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/AdminUserImportError" }
                        },
                        "created_users": {
                            "type": "array",
                            "description": "One-time credentials for newly imported users. Returned only to the authenticated Admin.",
                            "items": { "$ref": "#/components/schemas/AdminUserImportCredential" }
                        }
                    },
                    "required": ["created", "skipped", "errors", "created_users"]
                },
                "AuditLogEntry": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "user_id": { "type": "string", "format": "uuid", "nullable": true },
                        "user_email": { "type": "string", "format": "email", "nullable": true },
                        "ip_address": { "type": "string", "nullable": true },
                        "action": { "type": "string" },
                        "target_type": { "type": "string", "nullable": true },
                        "target_id": { "type": "string", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "user_id", "user_email", "ip_address", "action", "target_type", "target_id", "created_at"]
                },
                "PluginRuntimeAuditRequest": {
                    "type": "object",
                    "properties": {
                        "plugin_id": {
                            "type": "string",
                            "minLength": 1,
                            "pattern": "\\S",
                            "description": "Plugin identifier. Must include at least one non-whitespace character."
                        },
                        "plugin_name": { "type": "string", "nullable": true },
                        "action": {
                            "type": "string",
                            "enum": ["runtime_enable_blocked", "runtime_space_enable_blocked"]
                        },
                        "runtime_state": {
                            "type": "string",
                            "enum": ["disabled", "blocked_runtime_unavailable"]
                        },
                        "reason": { "type": "string", "nullable": true },
                        "space_id": { "type": "string", "nullable": true },
                        "space_name": { "type": "string", "nullable": true }
                    },
                    "required": ["plugin_id", "action", "runtime_state"]
                },
                "UserSession": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "device_name": { "type": "string", "nullable": true },
                        "ip_address": { "type": "string", "nullable": true },
                        "user_agent": { "type": "string", "nullable": true },
                        "last_seen_at": { "type": "string", "format": "date-time" },
                        "expires_at": { "type": "string", "format": "date-time" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "device_name", "ip_address", "user_agent", "last_seen_at", "expires_at", "created_at"]
                },
                "ApiKey": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "key_prefix": { "type": "string" },
                        "is_service_account": { "type": "boolean" },
                        "scopes": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["read", "write", "admin"] }
                        },
                        "last_used_at": { "type": "string", "format": "date-time", "nullable": true },
                        "expires_at": { "type": "string", "format": "date-time", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "name", "key_prefix", "is_service_account", "scopes", "last_used_at", "expires_at", "created_at"]
                },
                "ApiKeyCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "scopes": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["read", "write", "admin"] },
                            "default": ["read", "write"]
                        },
                        "expires_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name"]
                },
                "ServiceApiKeyCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "scopes": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["read", "write", "admin"] },
                            "default": ["admin"],
                            "description": "Service account keys must include the admin scope."
                        },
                        "expires_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name"]
                },
                "ApiKeyCreateResponse": {
                    "allOf": [
                        { "$ref": "#/components/schemas/ApiKey" },
                        {
                            "type": "object",
                            "properties": {
                                "key": { "type": "string" }
                            },
                            "required": ["key"]
                        }
                    ]
                },
                "Tag": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "color": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "doc_count": { "type": "integer", "nullable": true }
                    },
                    "required": ["id", "name", "color", "created_at", "doc_count"]
                },
                "TagCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "color": { "type": "string", "nullable": true },
                        "source_id": { "type": "string", "format": "uuid", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name"]
                },
                "PropertySchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "property_type": { "type": "string" },
                        "options": { "type": ["object", "array", "string", "number", "boolean", "null"] },
                        "sort_order": { "type": "integer" }
                    },
                    "required": ["id", "name", "property_type", "options", "sort_order"]
                },
                "PropertySchemaCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "property_type": { "type": "string", "minLength": 1 },
                        "options": { "type": ["object", "array", "string", "number", "boolean", "null"] },
                        "source_id": { "type": "string", "format": "uuid", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name", "property_type"]
                },
                "PropertySchemaReorderRequest": {
                    "type": "object",
                    "properties": {
                        "property_ids": {
                            "type": "array",
                            "items": { "type": "string", "format": "uuid" }
                        }
                    },
                    "required": ["property_ids"]
                },
                "DocumentPropertyValue": {
                    "type": "object",
                    "properties": {
                        "property_id": { "type": "string", "format": "uuid" },
                        "property_name": { "type": "string" },
                        "property_type": { "type": "string" },
                        "options": { "type": ["object", "array", "string", "number", "boolean", "null"] },
                        "value": { "type": "string", "nullable": true }
                    },
                    "required": ["property_id", "property_name", "property_type", "options", "value"]
                },
                "Backlink": {
                    "type": "object",
                    "description": "A readable document in the current space that links to the requested document.",
                    "properties": {
                        "document_id": { "type": "string", "format": "uuid" },
                        "title": { "type": "string" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "space_name": { "type": "string" }
                    },
                    "required": ["document_id", "title", "space_id", "space_name"]
                },
                "DocumentPropertyValuesUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "values": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "source_id": { "type": "string", "format": "uuid", "nullable": true },
                                    "property_id": { "type": "string", "format": "uuid" },
                                    "value": { "type": "string", "nullable": true },
                                    "created_at": { "type": "string", "format": "date-time", "nullable": true },
                                    "updated_at": { "type": "string", "format": "date-time", "nullable": true }
                                },
                                "required": ["property_id"]
                            }
                        }
                    },
                    "required": ["values"]
                },
                "DocumentComment": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "document_id": { "type": "string", "format": "uuid" },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                        "user_id": { "type": "string", "format": "uuid" },
                        "user_name": { "type": "string" },
                        "content": { "type": "string" },
                        "selection_start": { "type": "integer", "nullable": true },
                        "selection_end": { "type": "integer", "nullable": true },
                        "selection_text": { "type": "string", "nullable": true },
                        "resolved": { "type": "boolean" },
                        "resolved_by": { "type": "string", "format": "uuid", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" },
                        "replies": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/DocumentComment" }
                        }
                    },
                    "required": ["id", "document_id", "parent_id", "user_id", "user_name", "content", "selection_start", "selection_end", "selection_text", "resolved", "resolved_by", "created_at", "updated_at", "replies"]
                },
                "DocumentCommentCreateRequest": {
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "minLength": 1, "maxLength": 10000 },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                        "selection_start": { "type": "integer", "nullable": true },
                        "selection_end": { "type": "integer", "nullable": true },
                        "selection_text": { "type": "string", "nullable": true }
                    },
                    "required": ["content"]
                },
                "DocumentCommentUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "minLength": 1, "maxLength": 10000 }
                    },
                    "required": ["content"]
                },
                "DocumentReaction": {
                    "type": "object",
                    "properties": {
                        "emoji": { "type": "string" },
                        "count": { "type": "integer", "minimum": 0 },
                        "user_reacted": { "type": "boolean" }
                    },
                    "required": ["emoji", "count", "user_reacted"]
                },
                "DocumentReactionToggleRequest": {
                    "type": "object",
                    "properties": {
                        "emoji": { "type": "string", "minLength": 1 }
                    },
                    "required": ["emoji"]
                },
                "DocumentVersion": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "version_number": { "type": "integer" },
                        "lines_added": { "type": "integer" },
                        "lines_removed": { "type": "integer" },
                        "created_by": { "type": "string", "format": "uuid" },
                        "author_name": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "version_number", "lines_added", "lines_removed", "created_by", "author_name", "created_at"]
                },
                "RestoreDocumentVersionResponse": {
                    "type": "object",
                    "properties": {
                        "version_id": { "type": "string", "format": "uuid" },
                        "version_number": { "type": "integer" }
                    },
                    "required": ["version_id", "version_number"]
                },
                "DocumentAttachment": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "filename": { "type": "string" },
                        "original_name": { "type": "string" },
                        "mime_type": { "type": "string" },
                        "size_bytes": { "type": "integer", "minimum": 0 },
                        "url": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "filename", "original_name", "mime_type", "size_bytes", "url", "created_at"]
                },
                "Document": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "folder_id": { "type": "string", "format": "uuid", "nullable": true },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                        "title": { "type": "string" },
                        "icon": { "type": "string", "nullable": true },
                        "cover_image": { "type": "string", "nullable": true },
                        "locked": { "type": "boolean" },
                        "locked_by": { "type": "string", "format": "uuid", "nullable": true },
                        "public": { "type": "boolean" },
                        "public_slug": { "type": "string", "nullable": true },
                        "created_by": { "type": "string", "format": "uuid" },
                        "last_updated_by": { "type": "string", "format": "uuid" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" },
                        "workflow_template_id": { "type": "string", "format": "uuid", "nullable": true },
                        "workflow_template_name": { "type": "string", "nullable": true },
                        "workflow_stage_id": { "type": "string", "nullable": true },
                        "workflow_stage_name": { "type": "string", "nullable": true },
                        "workflow_status": { "type": "string", "enum": ["active", "completed", "cancelled"], "nullable": true }
                    },
                    "required": ["id", "space_id", "folder_id", "parent_id", "title", "icon", "cover_image", "locked", "locked_by", "public", "public_slug", "created_by", "last_updated_by", "created_at", "updated_at", "workflow_template_id", "workflow_template_name", "workflow_stage_id", "workflow_stage_name", "workflow_status"]
                },
                "DocumentCreateRequest": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "maxLength": 500 },
                        "folder_id": { "type": "string", "format": "uuid", "nullable": true },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "content": { "type": "string", "maxLength": 5242880, "nullable": true },
                        "template_id": { "type": "string", "format": "uuid", "nullable": true, "description": "Optional same-space template whose matching tag and property metadata is applied atomically." },
                        "source_id": { "type": "string", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "updated_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["title"]
                },
                "DocumentUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "maxLength": 500, "nullable": true },
                        "folder_id": { "type": "string", "format": "uuid", "nullable": true },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "cover_image": { "type": "string", "nullable": true }
                    },
                    "minProperties": 1
                },
                "DocumentMoveRequest": {
                    "type": "object",
                    "properties": {
                        "target_space_id": { "type": "string", "format": "uuid" },
                        "folder_id": { "type": "string", "format": "uuid", "nullable": true },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true }
                    },
                    "required": ["target_space_id"]
                },
                "DocumentContent": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "title": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["id", "title", "content"]
                },
                "DocumentContentUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "maxLength": 5242880 }
                    },
                    "required": ["content"]
                },
                "StarredDocument": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "space_name": { "type": "string" },
                        "title": { "type": "string" },
                        "icon": { "type": "string", "nullable": true },
                        "updated_at": { "type": "string", "format": "date-time" },
                        "starred_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "space_id", "space_name", "title", "icon", "updated_at", "starred_at"]
                },
                "DocumentTemplate": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "description": { "type": "string", "nullable": true },
                        "content": { "type": "string" },
                        "properties": { "type": ["object", "array", "string", "number", "boolean", "null"] },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "created_by": { "type": "string", "format": "uuid" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "space_id", "name", "description", "content", "properties", "tags", "created_by", "created_at", "updated_at"]
                },
                "DocumentTemplateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "description": { "type": "string", "maxLength": 1000, "nullable": true },
                        "content": { "type": "string" },
                        "properties": { "type": ["object", "array", "string", "number", "boolean", "null"] },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "source_id": { "type": "string", "format": "uuid", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "updated_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name", "content"]
                },
                "PublishDocumentResponse": {
                    "type": "object",
                    "properties": {
                        "public_slug": {
                            "type": "string",
                            "minLength": 16,
                            "maxLength": 16,
                            "description": "Generated public page slug for /p/{slug}."
                        }
                    },
                    "required": ["public_slug"]
                },
                "PublicPage": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "icon": { "type": "string", "nullable": true },
                        "cover_image": { "type": "string", "nullable": true },
                        "content": { "type": "string" },
                        "space_name": { "type": "string" },
                        "space_accent_color": { "type": "string", "nullable": true },
                        "author": { "type": "string" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["title", "icon", "cover_image", "content", "space_name", "space_accent_color", "author", "updated_at"]
                },
                "WorkflowTemplateStage": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "sort_order": { "type": "integer" }
                    },
                    "required": ["id", "name", "sort_order"]
                },
                "WorkflowTemplateApprover": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "approver_type": { "type": "string", "enum": ["user", "role"] },
                        "approver_id": { "type": "string" },
                        "stage_id": { "type": "string", "nullable": true }
                    },
                    "required": ["id", "approver_type", "approver_id", "stage_id"]
                },
                "WorkflowTemplate": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "description": { "type": "string", "nullable": true },
                        "stages": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/WorkflowTemplateStage" }
                        },
                        "approvers": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/WorkflowTemplateApprover" }
                        },
                        "created_by": { "type": "string", "format": "uuid" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "space_id", "name", "description", "stages", "approvers", "created_by", "created_at", "updated_at"]
                },
                "WorkflowTemplateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "description": { "type": "string", "nullable": true },
                        "stages": {
                            "type": "array",
                            "minItems": 2,
                            "maxItems": 12,
                            "items": { "$ref": "#/components/schemas/WorkflowTemplateStage" }
                        },
                        "approvers": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/WorkflowTemplateApprover" }
                        },
                        "source_id": { "type": "string", "format": "uuid", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "updated_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["name", "stages"]
                },
                "DocumentWorkflow": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "document_id": { "type": "string", "format": "uuid" },
                        "workflow_template_id": { "type": "string", "format": "uuid" },
                        "workflow_template_name": { "type": "string" },
                        "current_stage_id": { "type": "string" },
                        "current_stage_name": { "type": "string" },
                        "status": { "type": "string", "enum": ["active", "completed", "cancelled"] },
                        "stages": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/WorkflowTemplateStage" }
                        },
                        "approvers": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/WorkflowTemplateApprover" }
                        },
                        "created_by": { "type": "string", "format": "uuid" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "document_id", "workflow_template_id", "workflow_template_name", "current_stage_id", "current_stage_name", "status", "stages", "approvers", "created_by", "created_at", "updated_at"]
                },
                "DocumentWorkflowEnableRequest": {
                    "type": "object",
                    "properties": {
                        "template_id": { "type": "string", "format": "uuid" },
                        "stage_id": { "type": "string", "nullable": true },
                        "source_id": { "type": "string", "nullable": true },
                        "event_source_id": { "type": "string", "nullable": true },
                        "event_created_by": { "type": "string", "format": "uuid", "nullable": true },
                        "event_created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "updated_at": { "type": "string", "format": "date-time", "nullable": true }
                    },
                    "required": ["template_id"]
                },
                "DocumentWorkflowTransitionRequest": {
                    "type": "object",
                    "properties": {
                        "stage_id": { "type": "string" },
                        "note": { "type": "string", "nullable": true }
                    },
                    "required": ["stage_id"]
                },
                "DocumentWorkflowEvent": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "document_workflow_id": { "type": "string", "format": "uuid" },
                        "document_id": { "type": "string", "format": "uuid" },
                        "from_stage_id": { "type": "string", "nullable": true },
                        "to_stage_id": { "type": "string" },
                        "action": { "type": "string", "enum": ["enable", "disable", "transition", "delegate", "escalate"] },
                        "note": { "type": "string", "nullable": true },
                        "created_by": { "type": "string", "format": "uuid" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "document_workflow_id", "document_id", "from_stage_id", "to_stage_id", "action", "note", "created_by", "created_at"]
                },
                "DocumentWorkflowEventRequest": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["delegate", "escalate", "transition", "enable", "disable"] },
                        "note": { "type": "string", "nullable": true },
                        "source_id": { "type": "string", "nullable": true },
                        "created_by": { "type": "string", "format": "uuid", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time", "nullable": true },
                        "from_stage_id": { "type": "string", "nullable": true },
                        "to_stage_id": { "type": "string", "nullable": true }
                    },
                    "required": ["action"]
                },
                "WorkflowDeadlineEscalationCandidate": {
                    "type": "object",
                    "properties": {
                        "event_id": { "type": "string", "format": "uuid" },
                        "document_workflow_id": { "type": "string", "format": "uuid" },
                        "document_id": { "type": "string", "format": "uuid" },
                        "document_title": { "type": "string" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "workflow_template_name": { "type": "string" },
                        "current_stage_id": { "type": "string" },
                        "target": { "type": "string" },
                        "due_date": { "type": "string", "format": "date" },
                        "note": { "type": "string" },
                        "delegated_by": { "type": "string", "format": "uuid" },
                        "delegated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["event_id", "document_workflow_id", "document_id", "document_title", "space_id", "workflow_template_name", "current_stage_id", "target", "due_date", "note", "delegated_by", "delegated_at"]
                },
                "WorkflowDeadlineEscalationRunResponse": {
                    "type": "object",
                    "properties": {
                        "scanned": { "type": "integer", "minimum": 0 },
                        "escalated": { "type": "integer", "minimum": 0 },
                        "dry_run": { "type": "boolean" },
                        "target": { "type": "string" },
                        "candidates": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/WorkflowDeadlineEscalationCandidate" }
                        }
                    },
                    "required": ["scanned", "escalated", "dry_run", "target", "candidates"]
                },
                "SearchResult": {
                    "type": "object",
                    "properties": {
                        "document_id": { "type": "string", "format": "uuid" },
                        "title": { "type": "string" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "space_name": { "type": "string" },
                        "excerpt": { "type": "string" },
                        "updated_at": { "type": "string", "format": "date-time" },
                        "author": { "type": "string" }
                    },
                    "required": ["document_id", "title", "space_id", "space_name", "excerpt", "updated_at", "author"]
                },
                "GraphNode": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "title": { "type": "string" },
                        "folder_id": { "type": "string", "format": "uuid", "nullable": true },
                        "connection_count": { "type": "integer", "minimum": 0 },
                        "orphan": { "type": "boolean" }
                    },
                    "required": ["id", "title", "folder_id", "connection_count", "orphan"]
                },
                "GraphEdge": {
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "format": "uuid" },
                        "target": { "type": "string", "format": "uuid" },
                        "relation": { "type": "string", "description": "A plus-delimited relation label such as wiki, mention, or wiki+mention." }
                    },
                    "required": ["source", "target", "relation"]
                },
                "GraphResponse": {
                    "type": "object",
                    "properties": {
                        "nodes": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/GraphNode" }
                        },
                        "edges": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/GraphEdge" }
                        }
                    },
                    "required": ["nodes", "edges"]
                },
                "AiConfig": {
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean" },
                        "default_provider": {
                            "type": "string",
                            "enum": ["ollama", "openai", "anthropic", "xai"]
                        },
                        "providers": {
                            "type": "array",
                            "items": {
                                "type": "string",
                                "enum": ["ollama", "openai", "anthropic", "xai"]
                            }
                        }
                    },
                    "required": ["enabled", "default_provider", "providers"]
                },
                "AiChatMessage": {
                    "type": "object",
                    "properties": {
                        "role": { "type": "string", "enum": ["system", "user", "assistant"] },
                        "content": { "type": "string" }
                    },
                    "required": ["role", "content"]
                },
                "AiChatRequest": {
                    "type": "object",
                    "properties": {
                        "messages": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/AiChatMessage" }
                        },
                        "document_context": { "type": "string", "nullable": true }
                    },
                    "required": ["messages"]
                },
                "AiChatResponse": {
                    "type": "object",
                    "properties": {
                        "message": { "$ref": "#/components/schemas/AiChatMessage" }
                    },
                    "required": ["message"]
                },
                "AiCompleteRequest": {
                    "type": "object",
                    "properties": {
                        "prompt": { "type": "string", "minLength": 1 },
                        "context": { "type": "string", "nullable": true }
                    },
                    "required": ["prompt"]
                },
                "AiCompleteResponse": {
                    "type": "object",
                    "properties": {
                        "completion": { "type": "string" }
                    },
                    "required": ["completion"]
                },
                "AiDocumentActionRequest": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["summarize", "translate", "improve", "extract_actions", "custom"],
                            "description": "Built-in document action or custom action using custom_prompt."
                        },
                        "content": { "type": "string" },
                        "custom_prompt": { "type": "string", "nullable": true },
                        "language": { "type": "string", "nullable": true }
                    },
                    "required": ["action", "content"]
                },
                "AiDocumentActionResponse": {
                    "type": "object",
                    "properties": {
                        "result": { "type": "string" }
                    },
                    "required": ["result"]
                },
                "SyncReplayManifest": {
                    "type": "object",
                    "properties": {
                        "schema_version": { "type": "integer", "const": 1 },
                        "provider": { "type": "string", "minLength": 1 },
                        "replay_contract": { "type": "object" },
                        "operation_count": { "type": "integer", "minimum": 0 },
                        "recovery_checkpoint_count": { "type": "integer", "minimum": 0 },
                        "recovery_checkpoints": { "type": "array", "items": {} },
                        "operations": { "type": "array", "items": { "type": "object" } }
                    },
                    "required": ["schema_version", "provider", "replay_contract", "operation_count", "operations"]
                },
                "SyncReplayPreflightResponse": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string" },
                        "provider": { "type": "string" },
                        "ready": { "type": "boolean" },
                        "operation_count": { "type": "integer", "minimum": 0 },
                        "ready_operation_count": { "type": "integer", "minimum": 0 },
                        "blocked_operation_count": { "type": "integer", "minimum": 0 },
                        "recovery_checkpoint_count": { "type": "integer", "minimum": 0 },
                        "target_backends": { "type": "array", "items": { "type": "string" } },
                        "blockers": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["status", "provider", "ready", "operation_count", "ready_operation_count", "blocked_operation_count", "recovery_checkpoint_count", "target_backends", "blockers"]
                },
                "SyncReplayOperationResult": {
                    "type": "object",
                    "properties": {
                        "operation_id": { "type": "string" },
                        "entity_type": { "type": "string" },
                        "action": { "type": "string" },
                        "status": { "type": "string" },
                        "message": { "type": "string" }
                    },
                    "required": ["operation_id", "entity_type", "action", "status", "message"]
                },
                "SyncReplayExecutionResponse": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["completed", "partial"] },
                        "attempted_operation_count": { "type": "integer", "minimum": 0 },
                        "applied_operation_count": { "type": "integer", "minimum": 0 },
                        "skipped_operation_count": { "type": "integer", "minimum": 0 },
                        "failed_operation_count": { "type": "integer", "minimum": 0 },
                        "unsupported_operation_count": { "type": "integer", "minimum": 0 },
                        "results": { "type": "array", "items": { "$ref": "#/components/schemas/SyncReplayOperationResult" } }
                    },
                    "required": ["status", "attempted_operation_count", "applied_operation_count", "skipped_operation_count", "failed_operation_count", "unsupported_operation_count", "results"]
                },
                "SyncReplayHistoryCheckpoint": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "space_id": { "type": "string", "format": "uuid", "nullable": true },
                        "source_server_url": { "type": "string", "format": "uri", "nullable": true },
                        "downloaded_by_user_id": { "type": "string", "format": "uuid", "nullable": true },
                        "checkpoint": {},
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "space_id", "source_server_url", "downloaded_by_user_id", "checkpoint", "created_at"]
                },
                "SyncReplayHistoryManifest": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "provider": { "type": "string" },
                        "status": { "type": "string" },
                        "attempted_by": { "type": "string", "format": "uuid" },
                        "operation_count": { "type": "integer", "minimum": 0 },
                        "attempted_operation_count": { "type": "integer", "minimum": 0 },
                        "applied_operation_count": { "type": "integer", "minimum": 0 },
                        "skipped_operation_count": { "type": "integer", "minimum": 0 },
                        "failed_operation_count": { "type": "integer", "minimum": 0 },
                        "unsupported_operation_count": { "type": "integer", "minimum": 0 },
                        "recovery_checkpoint_count": { "type": "integer", "minimum": 0 },
                        "created_at": { "type": "string", "format": "date-time" },
                        "checkpoints": { "type": "array", "items": { "$ref": "#/components/schemas/SyncReplayHistoryCheckpoint" } }
                    },
                    "required": ["id", "provider", "status", "attempted_by", "operation_count", "attempted_operation_count", "applied_operation_count", "skipped_operation_count", "failed_operation_count", "unsupported_operation_count", "recovery_checkpoint_count", "created_at", "checkpoints"]
                },
                "SyncReplayHistoryResponse": {
                    "type": "object",
                    "properties": { "manifests": { "type": "array", "items": { "$ref": "#/components/schemas/SyncReplayHistoryManifest" } } },
                    "required": ["manifests"]
                },
                "WebhookCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1 },
                        "url": { "type": "string", "format": "uri" },
                        "secret": { "type": "string", "nullable": true },
                        "events": { "type": "array", "items": { "type": "string" } },
                        "space_id": { "type": "string", "format": "uuid", "nullable": true }
                    },
                    "required": ["name", "url", "events"]
                },
                "Webhook": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "url": { "type": "string", "format": "uri" },
                        "events": { "type": "array", "items": { "type": "string" } },
                        "space_id": { "type": "string", "format": "uuid", "nullable": true },
                        "is_active": { "type": "boolean" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "name", "url", "events", "space_id", "is_active", "created_at"]
                },
                "WebhookDelivery": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "event_type": { "type": "string" },
                        "response_status": { "type": "integer", "nullable": true },
                        "success": { "type": "boolean" },
                        "attempt_number": { "type": "integer" },
                        "delivered_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "event_type", "response_status", "success", "attempt_number", "delivered_at"]
                },
                "Notification": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "notification_type": { "type": "string" },
                        "title": { "type": "string" },
                        "body": { "type": "string", "nullable": true },
                        "source_user": { "type": "string", "nullable": true },
                        "target_type": { "type": "string", "nullable": true },
                        "target_id": { "type": "string", "nullable": true },
                        "read": { "type": "boolean" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "notification_type", "title", "body", "source_user", "target_type", "target_id", "read", "created_at"]
                },
                "NotificationPreference": {
                    "type": "object",
                    "properties": {
                        "notification_type": { "type": "string", "enum": ["workflow_approval", "workflow_handoff", "workflow_delegate", "workflow_escalate", "mention", "comment", "comment_reply", "doc_shared", "space_invite", "doc_subscription", "doc_locked", "doc_unlocked", "system"] },
                        "label": { "type": "string" },
                        "channel": { "type": "string", "enum": ["both", "in_app", "email", "none"] }
                    },
                    "required": ["notification_type", "label", "channel"]
                },
                "NotificationPreferencesUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "preferences": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "notification_type": { "type": "string", "enum": ["workflow_approval", "workflow_handoff", "workflow_delegate", "workflow_escalate", "mention", "comment", "comment_reply", "doc_shared", "space_invite", "doc_subscription", "doc_locked", "doc_unlocked", "system"] },
                                    "channel": { "type": "string", "enum": ["both", "in_app", "email", "none"] }
                                },
                                "required": ["notification_type", "channel"]
                            }
                        }
                    },
                    "required": ["preferences"]
                },
                "McpServerManifest": {
                    "type": "object",
                    "properties": {
                        "schema_version": { "type": "integer" },
                        "transport_status": { "type": "string", "enum": ["manifest_endpoint_only", "read_only_http_tools"] },
                        "scopes": { "type": "array", "items": { "type": "string" } },
                        "summary": { "$ref": "#/components/schemas/McpCatalogSummary" },
                        "tools": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/McpToolManifestEntry" }
                        }
                    },
                    "required": ["schema_version", "transport_status", "scopes", "summary", "tools"]
                },
                "McpCatalogSummary": {
                    "type": "object",
                    "properties": {
                        "tool_count": { "type": "integer", "minimum": 0 },
                        "by_family": { "type": "object", "additionalProperties": { "type": "integer", "minimum": 0 } },
                        "by_scope": { "type": "object", "additionalProperties": { "type": "integer", "minimum": 0 } },
                        "by_confirmation": { "type": "object", "additionalProperties": { "type": "integer", "minimum": 0 } },
                        "by_runtime_mode": { "type": "object", "additionalProperties": { "type": "integer", "minimum": 0 } }
                    },
                    "required": ["tool_count", "by_family", "by_scope", "by_confirmation", "by_runtime_mode"]
                },
                "McpToolManifestEntry": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "family": { "type": "string" },
                        "required_scopes": { "type": "array", "items": { "type": "string" } },
                        "confirmation": { "type": "string" },
                        "description": { "type": "string" },
                        "runtime_modes": { "type": "array", "items": { "type": "string", "enum": ["server", "local_vault"] } },
                        "input_schema": { "type": "object" },
                        "output_schema": { "type": "object" }
                    },
                    "required": ["name", "family", "required_scopes", "confirmation", "description", "runtime_modes", "input_schema", "output_schema"]
                },
                "McpToolExecutionRequest": {
                    "type": "object",
                    "properties": {
                        "input": { "type": "object" }
                    }
                },
                "McpToolExecutionResponse": {
                    "type": "object",
                    "properties": {
                        "tool": { "type": "string" },
                        "read_only": { "type": "boolean" },
                        "result": {}
                    },
                    "required": ["tool", "read_only", "result"]
                },
                "ScimUser": {
                    "type": "object",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string" } },
                        "id": { "type": "string", "format": "uuid" },
                        "userName": { "type": "string", "format": "email" },
                        "externalId": { "type": ["string", "null"] },
                        "displayName": { "type": "string" },
                        "active": { "type": "boolean" },
                        "emails": { "type": "array", "items": { "type": "object" } },
                        "meta": { "type": "object" }
                    },
                    "required": ["schemas", "id", "userName", "externalId", "displayName", "active", "emails", "meta"]
                },
                "ScimGroup": {
                    "type": "object",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string" } },
                        "id": { "type": "string", "format": "uuid" },
                        "displayName": { "type": "string" },
                        "externalId": { "type": ["string", "null"] },
                        "description": { "type": ["string", "null"] },
                        "members": { "type": "array", "items": { "type": "object", "properties": { "value": { "type": "string", "format": "uuid" }, "display": { "type": "string" } }, "required": ["value"] } },
                        "meta": { "type": "object" }
                    },
                    "required": ["schemas", "id", "displayName", "externalId", "description", "members", "meta"]
                },
                "ScimGroupListResponse": {
                    "type": "object",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string" } },
                        "totalResults": { "type": "integer", "minimum": 0 },
                        "startIndex": { "type": "integer", "minimum": 1 },
                        "itemsPerPage": { "type": "integer", "minimum": 0 },
                        "Resources": { "type": "array", "items": { "$ref": "#/components/schemas/ScimGroup" } }
                    },
                    "required": ["schemas", "totalResults", "startIndex", "itemsPerPage", "Resources"]
                },
                "ScimGroupCreateRequest": {
                    "type": "object",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string" } },
                        "displayName": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "externalId": { "type": "string", "maxLength": 255 },
                        "description": { "type": "string", "maxLength": 1000 },
                        "members": { "type": "array", "items": { "type": "object", "properties": { "value": { "type": "string", "maxLength": 128 } }, "required": ["value"] } }
                    },
                    "required": ["displayName"],
                    "additionalProperties": true
                },
                "ScimUserCreateRequest": {
                    "type": "object",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string" } },
                        "userName": { "type": "string", "format": "email" },
                        "externalId": { "type": "string", "maxLength": 255 },
                        "displayName": { "type": "string", "maxLength": 100 },
                        "active": { "type": "boolean", "default": true }
                    },
                    "required": ["userName"],
                    "additionalProperties": true
                },
                "ScimPatchRequest": {
                    "type": "object",
                    "description": "One replace operation is supported for active (boolean), displayName (non-empty string), or userName (email).",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string", "const": "urn:ietf:params:scim:api:messages:2.0:PatchOp" } },
                        "Operations": { "type": "array", "minItems": 1, "maxItems": 1, "items": { "type": "object", "properties": { "op": { "type": "string", "const": "replace" }, "path": { "type": "string", "enum": ["active", "displayName", "userName"] }, "value": {} }, "required": ["op", "path", "value"] } }
                    },
                    "required": ["schemas", "Operations"]
                },
                "ScimGroupMembershipPatchRequest": {
                    "type": "object",
                    "description": "Only one replace operation with path members and an array of member values is supported.",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string", "const": "urn:ietf:params:scim:api:messages:2.0:PatchOp" } },
                        "Operations": { "type": "array", "minItems": 1, "maxItems": 1, "items": { "type": "object", "properties": { "op": { "type": "string", "const": "replace" }, "path": { "type": "string", "const": "members" }, "value": { "type": "array", "items": { "type": "object", "properties": { "value": { "type": "string", "maxLength": 128 } }, "required": ["value"] } } }, "required": ["op", "path", "value"] } }
                    },
                    "required": ["schemas", "Operations"]
                },
                "ScimListResponse": {
                    "type": "object",
                    "properties": {
                        "schemas": { "type": "array", "items": { "type": "string" } },
                        "totalResults": { "type": "integer", "minimum": 0 },
                        "startIndex": { "type": "integer", "minimum": 1 },
                        "itemsPerPage": { "type": "integer", "minimum": 0 },
                        "Resources": { "type": "array", "items": { "$ref": "#/components/schemas/ScimUser" } }
                    },
                    "required": ["schemas", "totalResults", "startIndex", "itemsPerPage", "Resources"]
                }
            }
        },
        "paths": build_paths(),
    })
}

fn build_paths() -> Value {
    let mut paths = serde_json::Map::new();

    paths.insert(
        "/ServiceProviderConfig".into(),
        json!({ "get": scim_operation("Read supported SCIM capabilities") }),
    );
    paths.insert(
        "/ResourceTypes".into(),
        json!({ "get": scim_operation("List supported SCIM resource types") }),
    );
    paths.insert(
        "/ResourceTypes/{resource_type}".into(),
        json!({ "get": scim_operation("Read a supported SCIM resource type") }),
    );
    paths.insert(
        "/Schemas".into(),
        json!({ "get": scim_operation("List supported SCIM schemas") }),
    );
    paths.insert(
        "/Schemas/{schema_id}".into(),
        json!({ "get": scim_operation("Read a supported SCIM schema") }),
    );
    paths.insert(
        "/Users".into(),
        json!({
            "get": scim_operation("List SCIM users by userName or externalId"),
            "post": {
                "tags": ["SCIM"],
                "summary": "Provision a SCIM user",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimUserCreateRequest" } } } },
                "responses": {
                    "201": { "description": "User provisioned", "headers": { "Location": { "schema": { "type": "string" } } }, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimUser" } } } },
                    "400": { "description": "Invalid SCIM User resource" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "409": { "description": "Duplicate email or external ID" }
                }
            }
        }),
    );
    paths.insert(
        "/Users/{user_id}".into(),
        json!({
            "parameters": [
                {
                    "name": "user_id",
                    "in": "path",
                    "required": true,
                    "description": "Starlane Notes user ID for the SCIM User resource.",
                    "schema": { "type": "string", "format": "uuid" }
                }
            ],
            "get": scim_operation("Read a SCIM user"),
            "put": {
                "tags": ["SCIM"],
                "summary": "Replace a SCIM-provisioned user",
                "description": "Replaces userName, displayName, and active. The immutable externalId, when supplied, must match the provisioned user.",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimUserCreateRequest" } } } },
                "responses": {
                    "200": { "description": "SCIM user replaced", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimUser" } } } },
                    "400": { "description": "Invalid SCIM User resource or immutable external ID mismatch" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "404": { "description": "SCIM-provisioned user not found" },
                    "409": { "description": "Duplicate email" }
                }
            },
            "patch": {
                "tags": ["SCIM"],
                "summary": "Replace one SCIM-provisioned user lifecycle or profile field",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimPatchRequest" } } } },
                "responses": {
                    "204": { "description": "User lifecycle or profile field updated" },
                    "400": { "description": "Unsupported SCIM Patch operation" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "404": { "description": "SCIM-provisioned user not found" },
                    "409": { "description": "Duplicate email" }
                }
            },
            "delete": {
                "tags": ["SCIM"],
                "summary": "Deprovision a SCIM user",
                "description": "Deactivation is non-destructive: it disables sign-in while preserving the user's content.",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "responses": {
                    "204": { "description": "SCIM user deprovisioned" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "404": { "description": "SCIM-provisioned user not found" }
                }
            }
        }),
    );
    paths.insert(
        "/Groups".into(),
        json!({
            "get": scim_operation("List SCIM groups and memberships"),
            "post": {
                "tags": ["SCIM"],
                "summary": "Provision a SCIM group with optional existing members",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimGroupCreateRequest" } } } },
                "responses": {
                    "201": { "description": "Group provisioned", "headers": { "Location": { "schema": { "type": "string" } } }, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimGroup" } } } },
                    "400": { "description": "Invalid SCIM Group resource or unknown member" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "409": { "description": "Duplicate group display name" }
                }
            }
        }),
    );
    set_json_response(
        &mut paths,
        "/Users",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/ScimUserListResponse" }),
    );
    set_json_response(
        &mut paths,
        "/Groups",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/ScimGroupListResponse" }),
    );
    if let Some(operation) = paths.get_mut("/Users").and_then(|item| item.get_mut("get")) {
        operation["parameters"] = json!([
            { "name": "filter", "in": "query", "description": "Optional SCIM lookup filter. Supported forms are `userName eq \"person@example.com\"` and `externalId eq \"idp-user-123\"`.", "schema": { "type": "string" } },
            { "name": "startIndex", "in": "query", "schema": { "type": "integer", "minimum": 1, "default": 1 } },
            { "name": "count", "in": "query", "schema": { "type": "integer", "minimum": 0, "maximum": 100, "default": 100 } }
        ]);
    }
    if let Some(operation) = paths
        .get_mut("/Groups")
        .and_then(|item| item.get_mut("get"))
    {
        operation["parameters"] = json!([
            { "name": "startIndex", "in": "query", "schema": { "type": "integer", "minimum": 1, "default": 1 } },
            { "name": "count", "in": "query", "schema": { "type": "integer", "minimum": 0, "maximum": 100, "default": 100 } }
        ]);
    }
    paths.insert(
        "/Groups/{group_id}".into(),
        json!({
            "parameters": [
                {
                    "name": "group_id",
                    "in": "path",
                    "required": true,
                    "description": "Starlane Notes group ID for the SCIM Group resource.",
                    "schema": { "type": "string", "format": "uuid" }
                }
            ],
            "get": scim_operation("Read a SCIM group and membership"),
            "put": {
                "tags": ["SCIM"],
                "summary": "Replace a SCIM-managed group",
                "description": "Atomically replaces displayName, description, and complete membership. The immutable externalId must match when supplied.",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimGroupCreateRequest" } } } },
                "responses": {
                    "200": { "description": "SCIM group replaced", "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimGroup" } } } },
                    "400": { "description": "Invalid SCIM Group resource, member, or immutable external ID" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "404": { "description": "SCIM-managed group not found" },
                    "409": { "description": "Duplicate group display name" }
                }
            },
            "patch": {
                "tags": ["SCIM"],
                "summary": "Replace a SCIM group's complete membership",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/ScimGroupMembershipPatchRequest" } } } },
                "responses": {
                    "204": { "description": "Group membership replaced" },
                    "400": { "description": "Unsupported SCIM Group Patch operation or unknown member" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "404": { "description": "SCIM group not found" }
                }
            },
            "delete": {
                "tags": ["SCIM"],
                "summary": "Delete a SCIM-managed group",
                "description": "Removes the group, its memberships, and linked group permission grants. Locally administered groups cannot be deleted through SCIM.",
                "servers": [{ "url": "/scim/v2" }],
                "security": [{ "scimBearerAuth": [] }],
                "responses": {
                    "204": { "description": "SCIM group deleted" },
                    "401": { "description": "Invalid SCIM bearer token" },
                    "404": { "description": "SCIM-managed group not found" }
                }
            }
        }),
    );
    set_json_response(
        &mut paths,
        "/Groups/{group_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/ScimGroup" }),
    );

    add_path(
        &mut paths,
        "/health",
        [("get", "Health", "Check API health", false)],
    );
    set_json_response(
        &mut paths,
        "/health",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/HealthResponse" }),
    );
    set_json_response(
        &mut paths,
        "/health",
        "get",
        "503",
        json!({ "$ref": "#/components/schemas/HealthUnavailableResponse" }),
    );
    if let Some(response) = paths
        .get_mut("/health")
        .and_then(|item| item.get_mut("get"))
        .and_then(|operation| operation.get_mut("responses"))
        .and_then(|responses| responses.get_mut("503"))
    {
        response["description"] = json!("Database unavailable.");
    }

    add_path(
        &mut paths,
        "/auth/register",
        [(
            "post",
            "Authentication",
            "Register first or standard user",
            false,
        )],
    );
    add_path(
        &mut paths,
        "/auth/login",
        [(
            "post",
            "Authentication",
            "Exchange credentials for tokens",
            false,
        )],
    );
    add_path(
        &mut paths,
        "/auth/refresh",
        [("post", "Authentication", "Refresh access token", false)],
    );
    set_request_body(
        &mut paths,
        "/auth/register",
        "post",
        json!({ "$ref": "#/components/schemas/RegisterRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/register",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/AuthResponse" }),
    );
    set_error_response(
        &mut paths,
        "/auth/register",
        "post",
        "403",
        "Public signups are disabled.",
    );
    set_error_response(
        &mut paths,
        "/auth/register",
        "post",
        "409",
        "Email already registered.",
    );
    set_request_body(
        &mut paths,
        "/auth/login",
        "post",
        json!({ "$ref": "#/components/schemas/LoginRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/login",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AuthResponse" }),
    );
    set_request_body(
        &mut paths,
        "/auth/refresh",
        "post",
        json!({ "$ref": "#/components/schemas/RefreshTokenRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/refresh",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/TokenRefreshResponse" }),
    );
    add_path(
        &mut paths,
        "/auth/logout",
        [(
            "post",
            "Authentication",
            "Revoke the current refresh session",
            true,
        )],
    );
    set_empty_response(
        &mut paths,
        "/auth/logout",
        "post",
        "204",
        "Current refresh session revoked.",
    );
    add_path(
        &mut paths,
        "/auth/password-reset/request",
        [(
            "post",
            "Authentication",
            "Request a password reset link",
            false,
        )],
    );
    add_path(
        &mut paths,
        "/auth/password-reset/confirm",
        [("post", "Authentication", "Confirm password reset", false)],
    );
    set_request_body(
        &mut paths,
        "/auth/password-reset/request",
        "post",
        json!({ "$ref": "#/components/schemas/PasswordResetRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/password-reset/request",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/PasswordResetRequestResponse" }),
    );
    set_request_body(
        &mut paths,
        "/auth/password-reset/confirm",
        "post",
        json!({ "$ref": "#/components/schemas/PasswordResetConfirmRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/auth/password-reset/confirm",
        "post",
        "204",
        "Password reset confirmed.",
    );
    add_path(
        &mut paths,
        "/auth/me",
        [
            ("get", "Authentication", "Read current user", true),
            ("put", "Authentication", "Update current user profile", true),
        ],
    );
    set_json_response(
        &mut paths,
        "/auth/me",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/CurrentUserProfile" }),
    );
    set_request_body(
        &mut paths,
        "/auth/me",
        "put",
        json!({ "$ref": "#/components/schemas/UpdateProfileRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/me",
        "put",
        "200",
        json!({ "$ref": "#/components/schemas/CurrentUserProfile" }),
    );
    set_error_response(
        &mut paths,
        "/auth/me",
        "put",
        "409",
        "Username is already in use.",
    );
    add_path(
        &mut paths,
        "/auth/config",
        [(
            "get",
            "Authentication",
            "Read public auth configuration",
            false,
        )],
    );
    set_json_response(
        &mut paths,
        "/auth/config",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/AuthConfigResponse" }),
    );
    add_path(
        &mut paths,
        "/auth/sessions",
        [("get", "Authentication", "List current user sessions", true)],
    );
    add_path(
        &mut paths,
        "/auth/sessions/{session_id}",
        [(
            "delete",
            "Authentication",
            "Revoke a current user session",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/auth/sessions/{session_id}",
        [("session_id", "Current-user session ID to revoke.")],
    );
    set_empty_response(
        &mut paths,
        "/auth/sessions/{session_id}",
        "delete",
        "204",
        "Session revoked.",
    );
    add_path(
        &mut paths,
        "/auth/api-keys",
        [
            ("get", "Authentication", "List current user API keys", true),
            ("post", "Authentication", "Create a personal API key", true),
        ],
    );
    add_path(
        &mut paths,
        "/auth/api-keys/{key_id}",
        [(
            "delete",
            "Authentication",
            "Revoke a personal API key",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/auth/api-keys/{key_id}",
        [("key_id", "Current-user API key ID to revoke.")],
    );
    set_empty_response(
        &mut paths,
        "/auth/api-keys/{key_id}",
        "delete",
        "204",
        "API key revoked.",
    );
    set_json_response(
        &mut paths,
        "/auth/sessions",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/UserSession" } }),
    );
    set_json_response(
        &mut paths,
        "/auth/api-keys",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/ApiKey" } }),
    );
    set_request_body(
        &mut paths,
        "/auth/api-keys",
        "post",
        json!({ "$ref": "#/components/schemas/ApiKeyCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/api-keys",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/ApiKeyCreateResponse" }),
    );
    add_path(
        &mut paths,
        "/admin/api-keys",
        [
            (
                "get",
                "Administration",
                "List service account API keys",
                true,
            ),
            (
                "post",
                "Administration",
                "Create a service account API key",
                true,
            ),
        ],
    );
    add_path(
        &mut paths,
        "/admin/api-keys/{key_id}",
        [(
            "delete",
            "Administration",
            "Revoke a service account API key",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/api-keys/{key_id}",
        [("key_id", "Service account API key ID to revoke.")],
    );
    set_json_response(
        &mut paths,
        "/admin/api-keys",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/ApiKey" } }),
    );
    set_request_body(
        &mut paths,
        "/admin/api-keys",
        "post",
        json!({ "$ref": "#/components/schemas/ServiceApiKeyCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/admin/api-keys",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/ApiKeyCreateResponse" }),
    );
    set_empty_response(
        &mut paths,
        "/admin/api-keys/{key_id}",
        "delete",
        "204",
        "Service account API key revoked.",
    );
    add_path(
        &mut paths,
        "/auth/2fa/totp/setup",
        [("post", "Authentication", "Start TOTP setup", true)],
    );
    set_json_response(
        &mut paths,
        "/auth/2fa/totp/setup",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/TotpSetupResponse" }),
    );
    set_error_response(
        &mut paths,
        "/auth/2fa/totp/setup",
        "post",
        "409",
        "TOTP is already enabled.",
    );
    add_path(
        &mut paths,
        "/auth/2fa/totp/verify",
        [("post", "Authentication", "Verify TOTP code", true)],
    );
    set_request_body(
        &mut paths,
        "/auth/2fa/totp/verify",
        "post",
        json!({ "$ref": "#/components/schemas/TotpVerifyRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/2fa/totp/verify",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/TotpStatusResponse" }),
    );
    for path in [
        "/auth/register",
        "/auth/login",
        "/auth/refresh",
        "/auth/password-reset/request",
        "/auth/password-reset/confirm",
        "/auth/2fa/totp/verify",
    ] {
        set_error_response(&mut paths, path, "post", "429", "Rate limit exceeded.");
    }
    add_path(
        &mut paths,
        "/auth/2fa/totp/disable",
        [("post", "Authentication", "Disable TOTP", true)],
    );
    set_request_body(
        &mut paths,
        "/auth/2fa/totp/disable",
        "post",
        json!({ "$ref": "#/components/schemas/TotpVerifyRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/2fa/totp/disable",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/TotpStatusResponse" }),
    );
    set_error_response(
        &mut paths,
        "/auth/2fa/totp/disable",
        "post",
        "403",
        "Two-factor authentication is required by this server.",
    );
    add_path(
        &mut paths,
        "/auth/passkeys",
        [("get", "Authentication", "List passkeys", true)],
    );
    set_json_response(
        &mut paths,
        "/auth/passkeys",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Passkey" } }),
    );
    add_path(
        &mut paths,
        "/auth/passkeys/register/start",
        [("post", "Authentication", "Start passkey registration", true)],
    );
    set_request_body(
        &mut paths,
        "/auth/passkeys/register/start",
        "post",
        json!({ "$ref": "#/components/schemas/PasskeyRegisterStartRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/passkeys/register/start",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/PasskeyChallengeResponse" }),
    );
    add_path(
        &mut paths,
        "/auth/passkeys/register/finish",
        [(
            "post",
            "Authentication",
            "Finish passkey registration",
            true,
        )],
    );
    set_request_body(
        &mut paths,
        "/auth/passkeys/register/finish",
        "post",
        json!({ "$ref": "#/components/schemas/PasskeyRegisterFinishRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/passkeys/register/finish",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/Passkey" }),
    );
    set_error_response(
        &mut paths,
        "/auth/passkeys/register/finish",
        "post",
        "409",
        "Passkey already registered.",
    );
    add_path(
        &mut paths,
        "/auth/passkeys/{passkey_id}",
        [("delete", "Authentication", "Delete passkey", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/auth/passkeys/{passkey_id}",
        [("passkey_id", "Passkey ID to delete.")],
    );
    set_empty_response(
        &mut paths,
        "/auth/passkeys/{passkey_id}",
        "delete",
        "204",
        "Passkey deleted.",
    );
    add_path(
        &mut paths,
        "/auth/passkeys/login/start",
        [("post", "Authentication", "Start passkey login", false)],
    );
    set_request_body(
        &mut paths,
        "/auth/passkeys/login/start",
        "post",
        json!({ "$ref": "#/components/schemas/PasskeyLoginStartRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/passkeys/login/start",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/PasskeyChallengeResponse" }),
    );
    add_path(
        &mut paths,
        "/auth/passkeys/login/finish",
        [("post", "Authentication", "Finish passkey login", false)],
    );
    set_request_body(
        &mut paths,
        "/auth/passkeys/login/finish",
        "post",
        json!({ "$ref": "#/components/schemas/PasskeyLoginFinishRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/passkeys/login/finish",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AuthResponse" }),
    );
    for path in ["/auth/passkeys/login/start", "/auth/passkeys/login/finish"] {
        set_error_response(&mut paths, path, "post", "429", "Rate limit exceeded.");
    }
    for (path, request, response, secured) in [
        ("/auth/native-handoff/start", "NativeHandoffStartRequest", "NativeHandoffStartResponse", false),
        ("/auth/native-handoff/complete", "NativeHandoffCompleteRequest", "NativeHandoffCompleteResponse", true),
        ("/auth/native-handoff/exchange", "NativeHandoffExchangeRequest", "AuthResponse", false),
    ] {
        add_path(&mut paths, path, [("post", "Authentication", "Native browser authentication handoff", secured)]);
        set_request_body(&mut paths, path, "post", json!({ "$ref": format!("#/components/schemas/{request}") }));
        set_json_response(&mut paths, path, "post", "200", json!({ "$ref": format!("#/components/schemas/{response}") }));
    }
    for path in ["/auth/native-handoff/start", "/auth/native-handoff/exchange"] {
        set_error_response(&mut paths, path, "post", "429", "Rate limit exceeded.");
    }
    add_path(
        &mut paths,
        "/auth/oauth/{provider}",
        [("get", "Authentication", "Start OAuth login", false)],
    );
    set_string_path_parameters(
        &mut paths,
        "/auth/oauth/{provider}",
        [("provider", "OAuth provider identifier.")],
    );
    set_redirect_response(
        &mut paths,
        "/auth/oauth/{provider}",
        "get",
        "307",
        "Redirect to the OAuth provider authorization URL.",
    );
    add_path(
        &mut paths,
        "/auth/oauth/{provider}/callback",
        [("get", "Authentication", "Complete OAuth login", false)],
    );
    set_string_path_parameters(
        &mut paths,
        "/auth/oauth/{provider}/callback",
        [("provider", "OAuth provider identifier.")],
    );
    set_redirect_response(
        &mut paths,
        "/auth/oauth/{provider}/callback",
        "get",
        "307",
        "Redirect to the Starlane app with access and refresh tokens in the URL fragment.",
    );
    add_path(
        &mut paths,
        "/auth/oidc",
        [("get", "Authentication", "Start OIDC login", false)],
    );
    set_redirect_response(
        &mut paths,
        "/auth/oidc",
        "get",
        "307",
        "Redirect to the OIDC provider authorization URL.",
    );
    add_path(
        &mut paths,
        "/auth/oidc/callback",
        [("get", "Authentication", "Complete OIDC login", false)],
    );
    set_redirect_response(
        &mut paths,
        "/auth/oidc/callback",
        "get",
        "307",
        "Redirect to the Starlane app with access and refresh tokens in the URL fragment.",
    );

    add_path(
        &mut paths,
        "/users/directory",
        [(
            "get",
            "Users and Groups",
            "List active users for pickers",
            true,
        )],
    );
    set_json_response(
        &mut paths,
        "/users/directory",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/UserDirectoryEntry" } }),
    );
    add_path(
        &mut paths,
        "/admin/users",
        [
            ("get", "Administration", "List users", true),
            (
                "post",
                "Administration",
                "Create an admin-managed user",
                true,
            ),
        ],
    );
    add_path(
        &mut paths,
        "/admin/users/{user_id}",
        [(
            "put",
            "Administration",
            "Update user role or lifecycle state",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/users/{user_id}",
        [("user_id", "Admin-managed user ID to update.")],
    );
    add_path(
        &mut paths,
        "/admin/users/bulk-lifecycle",
        [(
            "post",
            "Administration",
            "Bulk activate or deactivate users",
            true,
        )],
    );
    add_path(
        &mut paths,
        "/admin/users/{user_id}/password-reset",
        [("post", "Administration", "Reset user password", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/users/{user_id}/password-reset",
        [("user_id", "Admin-managed user ID to reset.")],
    );
    add_path(
        &mut paths,
        "/admin/users/{user_id}/sessions",
        [("get", "Administration", "List user sessions", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/users/{user_id}/sessions",
        [(
            "user_id",
            "Admin-managed user ID whose sessions are listed.",
        )],
    );
    add_path(
        &mut paths,
        "/admin/users/{user_id}/sessions/{session_id}",
        [("delete", "Administration", "Revoke user session", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/users/{user_id}/sessions/{session_id}",
        [
            ("user_id", "Admin-managed user ID that owns the session."),
            ("session_id", "User session ID to revoke."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/admin/users/{user_id}/sessions/{session_id}",
        "delete",
        "204",
        "User session revoked.",
    );
    add_path(
        &mut paths,
        "/admin/users/import/template",
        [(
            "get",
            "Administration",
            "Download CSV import template",
            true,
        )],
    );
    set_csv_response(
        &mut paths,
        "/admin/users/import/template",
        "get",
        "200",
        "CSV template with email, display_name, role, and spaces columns.",
    );
    add_path(
        &mut paths,
        "/admin/users/import/preview",
        [("post", "Administration", "Preview CSV user import", true)],
    );
    add_path(
        &mut paths,
        "/admin/users/import",
        [("post", "Administration", "Import CSV users", true)],
    );
    set_json_response(
        &mut paths,
        "/admin/users",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/User" } }),
    );
    set_request_body(
        &mut paths,
        "/admin/users",
        "post",
        json!({ "$ref": "#/components/schemas/AdminUserCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/admin/users",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/AdminUserCreateResponse" }),
    );
    set_request_body(
        &mut paths,
        "/admin/users/{user_id}",
        "put",
        json!({ "$ref": "#/components/schemas/AdminUserUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/admin/users/{user_id}",
        "put",
        "204",
        "User lifecycle updated.",
    );
    set_request_body(
        &mut paths,
        "/admin/users/bulk-lifecycle",
        "post",
        json!({ "$ref": "#/components/schemas/AdminUserBulkLifecycleRequest" }),
    );
    set_json_response(
        &mut paths,
        "/admin/users/bulk-lifecycle",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AdminUserBulkLifecycleResponse" }),
    );
    set_json_response(
        &mut paths,
        "/admin/users/{user_id}/password-reset",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AdminPasswordResetResponse" }),
    );
    set_json_response(
        &mut paths,
        "/admin/users/{user_id}/sessions",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/UserSession" } }),
    );
    set_multipart_file_request_body(
        &mut paths,
        "/admin/users/import/preview",
        "post",
        "CSV file using the email, display_name, role, spaces header format.",
    );
    set_json_response(
        &mut paths,
        "/admin/users/import/preview",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AdminUserImportPreview" }),
    );
    set_multipart_file_request_body(
        &mut paths,
        "/admin/users/import",
        "post",
        "CSV file using the email, display_name, role, spaces header format.",
    );
    set_json_response(
        &mut paths,
        "/admin/users/import",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/AdminUserImportResult" }),
    );
    add_path(
        &mut paths,
        "/admin/config",
        [
            (
                "get",
                "Administration",
                "List admin configuration entries",
                true,
            ),
            (
                "post",
                "Administration",
                "Create or update an admin configuration entry",
                true,
            ),
        ],
    );
    set_json_response(
        &mut paths,
        "/admin/config",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/AdminConfigEntry" } }),
    );
    set_request_body(
        &mut paths,
        "/admin/config",
        "post",
        json!({ "$ref": "#/components/schemas/AdminConfigUpsertRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/admin/config",
        "post",
        "204",
        "Admin configuration saved.",
    );
    add_path(
        &mut paths,
        "/admin/config/{key}",
        [(
            "delete",
            "Administration",
            "Delete a database-backed configuration entry",
            true,
        )],
    );
    if let Some(item) = paths
        .get_mut("/admin/config/{key}")
        .and_then(Value::as_object_mut)
    {
        item.insert(
            "parameters".into(),
            json!([
                {
                    "name": "key",
                    "in": "path",
                    "required": true,
                    "description": "Database-backed admin configuration key to delete.",
                    "schema": { "type": "string" }
                }
            ]),
        );
    }
    set_empty_response(
        &mut paths,
        "/admin/config/{key}",
        "delete",
        "204",
        "Admin configuration entry deleted.",
    );
    add_path(
        &mut paths,
        "/admin/audit-log",
        [("get", "Administration", "List audit log entries", true)],
    );
    set_json_response(
        &mut paths,
        "/admin/audit-log",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/AuditLogEntry" } }),
    );
    if let Some(operation) = paths
        .get_mut("/admin/audit-log")
        .and_then(|item| item.get_mut("get"))
    {
        operation["parameters"] = json!([
            { "name": "action", "in": "query", "description": "Filter entries by audit action name.", "schema": { "type": "string" } },
            { "name": "user_id", "in": "query", "description": "Filter entries by acting user ID.", "schema": { "type": "string", "format": "uuid" } },
            { "name": "target_type", "in": "query", "description": "Filter entries by target resource type.", "schema": { "type": "string" } },
            { "name": "limit", "in": "query", "description": "Maximum entries to return.", "schema": { "type": "integer", "minimum": 0, "maximum": 200, "default": 50 } },
            { "name": "offset", "in": "query", "description": "Entry offset for pagination.", "schema": { "type": "integer", "minimum": 0, "default": 0 } }
        ]);
    }
    add_path(
        &mut paths,
        "/admin/audit-log/export",
        [("get", "Administration", "Export audit log CSV", true)],
    );
    set_csv_response(
        &mut paths,
        "/admin/audit-log/export",
        "get",
        "200",
        "Audit log CSV export.",
    );
    add_path(
        &mut paths,
        "/admin/plugin-runtime-audit",
        [(
            "post",
            "Administration",
            "Record a blocked plugin runtime audit event",
            true,
        )],
    );
    set_request_body(
        &mut paths,
        "/admin/plugin-runtime-audit",
        "post",
        json!({ "$ref": "#/components/schemas/PluginRuntimeAuditRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/admin/plugin-runtime-audit",
        "post",
        "204",
        "Plugin runtime audit event recorded.",
    );
    add_path(
        &mut paths,
        "/admin/search/reindex",
        [("post", "Administration", "Rebuild the search index", true)],
    );
    set_json_response(
        &mut paths,
        "/admin/search/reindex",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/SearchReindexResponse" }),
    );
    add_path(
        &mut paths,
        "/admin/groups",
        [
            ("get", "Users and Groups", "List groups", true),
            ("post", "Users and Groups", "Create group", true),
        ],
    );
    set_json_response(
        &mut paths,
        "/admin/groups",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Group" } }),
    );
    set_request_body(
        &mut paths,
        "/admin/groups",
        "post",
        json!({ "$ref": "#/components/schemas/GroupCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/admin/groups",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/Group" }),
    );
    add_path(
        &mut paths,
        "/admin/groups/{group_id}/members",
        [
            ("get", "Users and Groups", "List group members", true),
            ("post", "Users and Groups", "Add group member", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/groups/{group_id}/members",
        [("group_id", "Group ID whose members are listed or updated.")],
    );
    set_json_response(
        &mut paths,
        "/admin/groups/{group_id}/members",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/GroupMember" } }),
    );
    set_request_body(
        &mut paths,
        "/admin/groups/{group_id}/members",
        "post",
        json!({ "$ref": "#/components/schemas/GroupMemberAddRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/admin/groups/{group_id}/members",
        "post",
        "201",
        "Group member added.",
    );
    add_path(
        &mut paths,
        "/admin/groups/{group_id}/members/{user_id}",
        [("delete", "Users and Groups", "Remove group member", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/groups/{group_id}/members/{user_id}",
        [
            ("group_id", "Group ID that owns the member."),
            ("user_id", "User ID to remove from the group."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/admin/groups/{group_id}/members/{user_id}",
        "delete",
        "204",
        "Group member removed.",
    );

    add_path(
        &mut paths,
        "/spaces",
        [
            ("get", "Spaces", "List accessible spaces", true),
            ("post", "Spaces", "Create a space", true),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Space" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces",
        "post",
        json!({ "$ref": "#/components/schemas/SpaceCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/Space" }),
    );
    set_error_response(
        &mut paths,
        "/spaces",
        "post",
        "409",
        "Space name or import ID already exists.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}",
        [
            ("get", "Spaces", "Read a space", true),
            ("put", "Spaces", "Update a space", true),
            ("delete", "Spaces", "Delete a space", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}",
        [("space_id", "Space ID to read, update, or delete.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/Space" }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}",
        "put",
        json!({ "$ref": "#/components/schemas/SpaceUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}",
        "put",
        "204",
        "Space updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}",
        "delete",
        "204",
        "Space deleted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/members",
        [
            ("get", "Spaces", "List space members", true),
            ("post", "Spaces", "Add space member", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/members",
        [("space_id", "Space ID whose members are listed or updated.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/members",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/SpaceMember" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/members",
        "post",
        json!({ "$ref": "#/components/schemas/SpaceMemberRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/members",
        "post",
        "201",
        "Space member added.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/members/{user_id}",
        [
            ("put", "Spaces", "Update space member role", true),
            ("delete", "Spaces", "Remove space member", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/members/{user_id}",
        [
            ("space_id", "Space ID that owns the membership."),
            (
                "user_id",
                "User ID whose space membership is updated or removed.",
            ),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/members/{user_id}",
        "put",
        json!({ "$ref": "#/components/schemas/SpaceMemberRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/members/{user_id}",
        "put",
        "204",
        "Space member role updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/members/{user_id}",
        "delete",
        "204",
        "Space member removed.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/groups",
        [
            ("get", "Spaces", "List group grants for a space", true),
            ("post", "Spaces", "Grant group access to a space", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/groups",
        [(
            "space_id",
            "Space ID whose group grants are listed or updated.",
        )],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/groups",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/SpaceGroupGrant" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/groups",
        "post",
        json!({ "$ref": "#/components/schemas/SpaceGroupGrantRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/groups",
        "post",
        "201",
        "Space group access granted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}",
        [
            (
                "put",
                "Spaces",
                "Update group access role for a space",
                true,
            ),
            ("delete", "Spaces", "Remove group access from a space", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}",
        [
            ("space_id", "Space ID that owns the group grant."),
            (
                "group_id",
                "Group ID whose space grant is updated or removed.",
            ),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}",
        "put",
        json!({ "$ref": "#/components/schemas/SpaceGroupGrantRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}",
        "put",
        "204",
        "Space group access updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}",
        "delete",
        "204",
        "Space group access removed.",
    );

    add_path(
        &mut paths,
        "/spaces/{space_id}/folders",
        [
            ("get", "Documents", "List folders", true),
            ("post", "Documents", "Create folder", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/folders",
        [("space_id", "Space ID that owns the folders.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/folders",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Folder" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/folders",
        "post",
        json!({ "$ref": "#/components/schemas/FolderCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/folders",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/Folder" }),
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/folders",
        "post",
        "409",
        "Folder import ID already exists.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}",
        [
            ("get", "Documents", "Read folder", true),
            ("put", "Documents", "Update folder", true),
            ("delete", "Documents", "Delete folder", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}",
        [
            ("space_id", "Space ID that owns the folder."),
            ("folder_id", "Folder ID to read, update, or delete."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/Folder" }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}",
        "put",
        json!({ "$ref": "#/components/schemas/FolderUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}",
        "put",
        "204",
        "Folder updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}",
        "delete",
        "204",
        "Folder deleted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/move",
        [("post", "Documents", "Move folder", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/move",
        [
            ("space_id", "Source space ID that owns the folder."),
            ("folder_id", "Folder ID to move."),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/move",
        "post",
        json!({ "$ref": "#/components/schemas/FolderMoveRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/move",
        "post",
        "204",
        "Folder moved.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/reorder",
        [("put", "Documents", "Reorder folder", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/reorder",
        [
            ("space_id", "Space ID that owns the folder."),
            ("folder_id", "Folder ID to reorder."),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/reorder",
        "put",
        json!({ "$ref": "#/components/schemas/FolderReorderRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/reorder",
        "put",
        "204",
        "Folder reordered.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents",
        [
            ("get", "Documents", "List documents", true),
            ("post", "Documents", "Create document", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents",
        [("space_id", "Space ID that owns the documents.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Document" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/Document" }),
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/documents",
        "post",
        "409",
        "Document import ID already exists.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}",
        [
            ("get", "Documents", "Read document", true),
            ("put", "Documents", "Update document", true),
            ("delete", "Documents", "Delete document", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID to read, update, or delete."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/Document" }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}",
        "put",
        "204",
        "Document updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}",
        "delete",
        "204",
        "Document deleted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/move",
        [("post", "Documents", "Move document", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/move",
        [
            ("space_id", "Source space ID that owns the document."),
            ("doc_id", "Document ID to move."),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/move",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentMoveRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/move",
        "post",
        "204",
        "Document moved.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/content",
        [
            ("get", "Documents", "Read document content", true),
            ("put", "Documents", "Update document content", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/content",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID whose content is read or updated."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/content",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/DocumentContent" }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/content",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentContentUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/content",
        "put",
        "204",
        "Document content updated.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/star",
        [
            ("post", "Documents", "Star document", true),
            ("delete", "Documents", "Unstar document", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/star",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID to star or unstar."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/star",
        "post",
        "204",
        "Document starred.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/star",
        "delete",
        "204",
        "Document unstarred.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/starred",
        [("get", "Documents", "List starred documents", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/starred",
        [("space_id", "Space ID whose starred documents are listed.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/starred",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/StarredDocument" } }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/lock",
        [("post", "Documents", "Lock document", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/lock",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID to lock."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/lock",
        "post",
        "204",
        "Document locked.",
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/lock",
        "post",
        "409",
        "Document is locked by another user.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unlock",
        [("post", "Documents", "Unlock document", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unlock",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID to unlock."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unlock",
        "post",
        "204",
        "Document unlocked.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/publish",
        [("post", "Documents", "Publish document", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/publish",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID to publish."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/publish",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/PublishDocumentResponse" }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unpublish",
        [("post", "Documents", "Unpublish document", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unpublish",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID to unpublish."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unpublish",
        "post",
        "204",
        "Document unpublished.",
    );
    add_path(
        &mut paths,
        "/p/{slug}",
        [("get", "Documents", "Read public page by slug", false)],
    );
    set_string_path_parameters(
        &mut paths,
        "/p/{slug}",
        [("slug", "Published public page slug.")],
    );
    set_json_response(
        &mut paths,
        "/p/{slug}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/PublicPage" }),
    );

    add_path(
        &mut paths,
        "/spaces/{space_id}/templates",
        [
            ("get", "Templates", "List document templates", true),
            ("post", "Templates", "Create document template", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/templates",
        [("space_id", "Space ID that owns the document templates.")],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/templates/{template_id}",
        [
            ("get", "Templates", "Read document template", true),
            ("put", "Templates", "Update document template", true),
            ("delete", "Templates", "Delete document template", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/templates/{template_id}",
        [
            ("space_id", "Space ID that owns the document template."),
            ("template_id", "Document template ID."),
        ],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/workflow-templates",
        [
            ("get", "Workflows", "List workflow templates", true),
            ("post", "Workflows", "Create workflow template", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/workflow-templates",
        [("space_id", "Space ID that owns the workflow templates.")],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/workflow-templates/{template_id}",
        [
            ("get", "Workflows", "Read workflow template", true),
            ("put", "Workflows", "Update workflow template", true),
            ("delete", "Workflows", "Delete workflow template", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/workflow-templates/{template_id}",
        [
            ("space_id", "Space ID that owns the workflow template."),
            ("template_id", "Workflow template ID."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/templates",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentTemplate" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/templates",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentTemplateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/templates",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/IdResponse" }),
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/templates",
        "post",
        "409",
        "Document template import ID already exists.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/templates/{template_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/DocumentTemplate" }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/templates/{template_id}",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentTemplateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/templates/{template_id}",
        "put",
        "204",
        "Document template updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/templates/{template_id}",
        "delete",
        "204",
        "Document template deleted.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/workflow-templates",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/WorkflowTemplate" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/workflow-templates",
        "post",
        json!({ "$ref": "#/components/schemas/WorkflowTemplateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/workflow-templates",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/IdResponse" }),
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/workflow-templates",
        "post",
        "409",
        "Workflow template import ID already exists.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/workflow-templates/{template_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/WorkflowTemplate" }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/workflow-templates/{template_id}",
        "put",
        json!({ "$ref": "#/components/schemas/WorkflowTemplateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/workflow-templates/{template_id}",
        "put",
        "204",
        "Workflow template updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/workflow-templates/{template_id}",
        "delete",
        "204",
        "Workflow template deleted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow",
        [
            ("get", "Workflows", "Read active document workflow", true),
            ("put", "Workflows", "Activate document workflow", true),
            (
                "delete",
                "Workflows",
                "Clear active document workflow",
                true,
            ),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "document_id",
                "Document ID whose active workflow is managed.",
            ),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow",
        "get",
        "200",
        json!({
            "oneOf": [
                { "$ref": "#/components/schemas/DocumentWorkflow" },
                { "type": "null" }
            ]
        }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentWorkflowEnableRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow",
        "put",
        "200",
        json!({ "$ref": "#/components/schemas/DocumentWorkflow" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow",
        "delete",
        "204",
        "Active document workflow cleared.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/transitions",
        [(
            "post",
            "Workflows",
            "Transition active document workflow",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/transitions",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "document_id",
                "Document ID whose active workflow is transitioned.",
            ),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/transitions",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentWorkflowTransitionRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/transitions",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/DocumentWorkflow" }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/events",
        [
            ("get", "Workflows", "List workflow events", true),
            ("post", "Workflows", "Record workflow event", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/events",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "document_id",
                "Document ID whose workflow events are listed or recorded.",
            ),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/events",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentWorkflowEvent" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/events",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentWorkflowEventRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{document_id}/workflow/events",
        "post",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentWorkflowEvent" } }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/workflow-queue",
        [("get", "Workflows", "List workflow queue documents", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/workflow-queue",
        [("space_id", "Space ID whose workflow queue is listed.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/workflow-queue",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Document" } }),
    );
    add_path(
        &mut paths,
        "/admin/workflows/deadline-escalations",
        [
            (
                "get",
                "Workflows",
                "List overdue delegated workflow candidates",
                true,
            ),
            (
                "post",
                "Workflows",
                "Run overdue delegated workflow escalations",
                true,
            ),
        ],
    );
    set_json_response(
        &mut paths,
        "/admin/workflows/deadline-escalations",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/WorkflowDeadlineEscalationCandidate" } }),
    );
    set_json_response(
        &mut paths,
        "/admin/workflows/deadline-escalations",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/WorkflowDeadlineEscalationRunResponse" }),
    );

    add_path(
        &mut paths,
        "/spaces/{space_id}/tags",
        [
            ("get", "Metadata", "List tags", true),
            ("post", "Metadata", "Create tag", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/tags",
        [("space_id", "Space ID that owns the tags.")],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/tags/{tag_id}",
        [
            ("put", "Metadata", "Update tag", true),
            ("delete", "Metadata", "Delete tag", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/tags/{tag_id}",
        [
            ("space_id", "Space ID that owns the tag."),
            ("tag_id", "Tag ID to update or delete."),
        ],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags",
        [
            ("get", "Metadata", "List document tags", true),
            ("post", "Metadata", "Attach document tag", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID whose tags are listed or updated."),
        ],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags/{tag_id}",
        [("delete", "Metadata", "Remove document tag", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags/{tag_id}",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID whose tag is removed."),
            ("tag_id", "Tag ID to remove from the document."),
        ],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/properties",
        [
            ("get", "Metadata", "List property schemas", true),
            ("post", "Metadata", "Create property schema", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/properties",
        [("space_id", "Space ID that owns the property schemas.")],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/properties/{property_id}",
        [
            ("put", "Metadata", "Update property schema", true),
            ("delete", "Metadata", "Delete property schema", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/properties/{property_id}",
        [
            ("space_id", "Space ID that owns the property schema."),
            ("property_id", "Property schema ID to update or delete."),
        ],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/properties/reorder",
        [("put", "Metadata", "Reorder property schemas", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/properties/reorder",
        [("space_id", "Space ID whose property schemas are reordered.")],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/properties",
        [
            ("get", "Metadata", "List document properties", true),
            ("put", "Metadata", "Replace document properties", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/properties",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "doc_id",
                "Document ID whose property values are read or replaced.",
            ),
        ],
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/backlinks",
        [(
            "get",
            "Metadata",
            "List same-space document backlinks",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/backlinks",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID whose backlinks are listed."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/tags",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Tag" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/tags",
        "post",
        json!({ "$ref": "#/components/schemas/TagCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/tags",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/Tag" }),
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/tags",
        "post",
        "409",
        "Tag import ID already exists.",
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/tags/{tag_id}",
        "put",
        json!({ "$ref": "#/components/schemas/TagCreateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/tags/{tag_id}",
        "put",
        "204",
        "Tag updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/tags/{tag_id}",
        "delete",
        "204",
        "Tag deleted.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Tag" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags",
        "post",
        json!({
            "type": "object",
            "properties": { "tag_id": { "type": "string", "format": "uuid" } },
            "required": ["tag_id"]
        }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags",
        "post",
        "201",
        "Document tag attached.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/tags/{tag_id}",
        "delete",
        "204",
        "Document tag removed.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/properties",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/PropertySchema" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/properties",
        "post",
        json!({ "$ref": "#/components/schemas/PropertySchemaCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/properties",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/PropertySchema" }),
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/properties",
        "post",
        "409",
        "Property schema import ID already exists.",
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/properties/{property_id}",
        "put",
        json!({ "$ref": "#/components/schemas/PropertySchemaCreateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/properties/{property_id}",
        "put",
        "204",
        "Property schema updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/properties/{property_id}",
        "delete",
        "204",
        "Property schema deleted.",
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/properties/reorder",
        "put",
        json!({ "$ref": "#/components/schemas/PropertySchemaReorderRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/properties/reorder",
        "put",
        "204",
        "Property schemas reordered.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/properties",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentPropertyValue" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/properties",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentPropertyValuesUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/properties",
        "put",
        "204",
        "Document properties replaced.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/backlinks",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Backlink" } }),
    );

    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments",
        [
            ("get", "Documents", "List document comments", true),
            ("post", "Documents", "Create document comment", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "doc_id",
                "Document ID whose comments are listed or created.",
            ),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentComment" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentCommentCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/IdResponse" }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}",
        [
            ("put", "Documents", "Update document comment", true),
            ("delete", "Documents", "Delete document comment", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the comment."),
            ("comment_id", "Document comment ID to update or delete."),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentCommentUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}",
        "put",
        "204",
        "Document comment updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}",
        "delete",
        "204",
        "Document comment deleted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve",
        [("post", "Documents", "Resolve document comment", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the comment."),
            ("comment_id", "Document comment ID to resolve."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve",
        "post",
        "204",
        "Document comment resolved.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/reactions",
        [
            ("get", "Documents", "List document reactions", true),
            ("post", "Documents", "Toggle document reaction", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/reactions",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "doc_id",
                "Document ID whose reactions are listed or toggled.",
            ),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/reactions",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentReaction" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/reactions",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentReactionToggleRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/reactions",
        "post",
        "204",
        "Document reaction toggled.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions",
        [("get", "Documents", "List document versions", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID whose versions are listed."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentVersion" } }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}",
        [("get", "Documents", "Read document version", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the version."),
            ("version_id", "Document version ID to read."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/DocumentVersion" }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore",
        [("post", "Documents", "Restore document version", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the version."),
            ("version_id", "Document version ID to restore."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/RestoreDocumentVersionResponse" }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments",
        [
            ("get", "Documents", "List document attachments", true),
            ("post", "Documents", "Upload document attachment", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "doc_id",
                "Document ID whose attachments are listed or uploaded.",
            ),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentAttachment" } }),
    );
    set_multipart_file_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments",
        "post",
        "Document attachment file. The multipart field must be named `file`, include a non-blank filename no longer than 255 bytes, and be within the configured upload size.",
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/DocumentAttachment" }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}",
        [("delete", "Documents", "Delete document attachment", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the attachment."),
            ("attachment_id", "Document attachment ID to delete."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}",
        "delete",
        "204",
        "Document attachment deleted.",
    );

    add_path(
        &mut paths,
        "/search",
        [("get", "Search", "Search documents", true)],
    );
    set_json_response(
        &mut paths,
        "/search",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/SearchResult" } }),
    );
    if let Some(operation) = paths
        .get_mut("/search")
        .and_then(|item| item.get_mut("get"))
    {
        operation["parameters"] = json!([
            { "name": "q", "in": "query", "description": "Full-text query; omit for metadata-only filtering.", "schema": { "type": "string", "maxLength": 500 } },
            { "name": "title", "in": "query", "description": "Case-insensitive document title substring filter.", "schema": { "type": "string" } },
            { "name": "space_id", "in": "query", "schema": { "type": "string" } },
            { "name": "folder", "in": "query", "description": "Exact folder name or ID.", "schema": { "type": "string" } },
            { "name": "tag", "in": "query", "schema": { "type": "string" } },
            { "name": "property_name", "in": "query", "schema": { "type": "string" } },
            { "name": "property_value", "in": "query", "schema": { "type": "string" } },
            { "name": "has_attachment", "in": "query", "schema": { "type": "boolean" } },
            { "name": "has_cover", "in": "query", "schema": { "type": "boolean" } },
            { "name": "has_workflow", "in": "query", "description": "Whether a document has a workflow instance.", "schema": { "type": "boolean" } },
            { "name": "has_tag", "in": "query", "description": "Whether a document has at least one tag.", "schema": { "type": "boolean" } },
            { "name": "has_property", "in": "query", "description": "Whether a document has at least one stored property value.", "schema": { "type": "boolean" } },
            { "name": "is_locked", "in": "query", "description": "Whether a document is currently locked.", "schema": { "type": "boolean" } },
            { "name": "created_on", "in": "query", "description": "Exact document creation date in YYYY-MM-DD format.", "schema": { "type": "string", "format": "date" } },
            { "name": "updated_on", "in": "query", "description": "Exact document update date in YYYY-MM-DD format.", "schema": { "type": "string", "format": "date" } },
            { "name": "limit", "in": "query", "schema": { "type": "integer", "minimum": 0, "maximum": 100, "default": 20 } }
        ]);
    }
    add_path(
        &mut paths,
        "/dataview",
        [("get", "Search", "Run a Dataview query", true)],
    );
    set_json_response(
        &mut paths,
        "/dataview",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/SearchResult" } }),
    );
    if let Some(operation) = paths
        .get_mut("/dataview")
        .and_then(|item| item.get_mut("get"))
    {
        operation["parameters"] = json!([
            { "name": "q", "in": "query", "description": "Dataview query expression.", "schema": { "type": "string", "maxLength": 500 } },
            { "name": "limit", "in": "query", "description": "Maximum results to return.", "schema": { "type": "integer", "minimum": 0, "maximum": 100, "default": 50 } }
        ]);
    }
    add_path(
        &mut paths,
        "/spaces/{space_id}/graph",
        [("get", "Graph", "Read space graph", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/graph",
        [("space_id", "Space ID whose graph is read.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/graph",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/GraphResponse" }),
    );
    add_path(
        &mut paths,
        "/mcp/manifest",
        [("get", "MCP", "Read the MCP tool manifest", true)],
    );
    set_json_response(
        &mut paths,
        "/mcp/manifest",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/McpServerManifest" }),
    );
    add_path(
        &mut paths,
        "/mcp/tools/{tool_name}",
        [("post", "MCP", "Execute a read-only MCP tool", true)],
    );
    set_request_body(
        &mut paths,
        "/mcp/tools/{tool_name}",
        "post",
        json!({ "$ref": "#/components/schemas/McpToolExecutionRequest" }),
    );
    set_json_response(
        &mut paths,
        "/mcp/tools/{tool_name}",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/McpToolExecutionResponse" }),
    );
    if let Some(operation) = paths
        .get_mut("/mcp/tools/{tool_name}")
        .and_then(|item| item.get_mut("post"))
    {
        operation["parameters"] = json!([
            {
                "name": "tool_name",
                "in": "path",
                "required": true,
                "description": "Namespaced MCP tool name from the manifest, for example `starlane.documents.get`.",
                "schema": { "type": "string" }
            }
        ]);
    }
    add_path(
        &mut paths,
        "/sync/replay/preflight",
        [(
            "post",
            "Sync",
            "Preflight a queued sync replay manifest",
            true,
        )],
    );
    set_request_body(
        &mut paths,
        "/sync/replay/preflight",
        "post",
        json!({ "$ref": "#/components/schemas/SyncReplayManifest" }),
    );
    set_json_response(
        &mut paths,
        "/sync/replay/preflight",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/SyncReplayPreflightResponse" }),
    );
    add_path(
        &mut paths,
        "/sync/replay",
        [(
            "post",
            "Sync",
            "Execute supported queued sync replay operations",
            true,
        )],
    );
    set_request_body(
        &mut paths,
        "/sync/replay",
        "post",
        json!({ "$ref": "#/components/schemas/SyncReplayManifest" }),
    );
    set_json_response(
        &mut paths,
        "/sync/replay",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/SyncReplayExecutionResponse" }),
    );
    add_path(
        &mut paths,
        "/sync/replay/history",
        [(
            "get",
            "Sync",
            "List the current user's recent replay history",
            true,
        )],
    );
    set_json_response(
        &mut paths,
        "/sync/replay/history",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/SyncReplayHistoryResponse" }),
    );

    add_path(
        &mut paths,
        "/webhooks",
        [
            ("get", "Webhooks", "List webhooks", true),
            ("post", "Webhooks", "Create webhook", true),
        ],
    );
    add_path(
        &mut paths,
        "/webhooks/{webhook_id}",
        [
            ("get", "Webhooks", "Read webhook", true),
            ("put", "Webhooks", "Update webhook", true),
            ("delete", "Webhooks", "Delete webhook", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/webhooks/{webhook_id}",
        [("webhook_id", "Webhook ID to read, update, or delete.")],
    );
    add_path(
        &mut paths,
        "/webhooks/{webhook_id}/deliveries",
        [("get", "Webhooks", "List webhook deliveries", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/webhooks/{webhook_id}/deliveries",
        [("webhook_id", "Webhook ID whose deliveries are listed.")],
    );
    add_path(
        &mut paths,
        "/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver",
        [("post", "Webhooks", "Redeliver webhook delivery", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver",
        [
            ("webhook_id", "Webhook ID that owns the delivery."),
            ("delivery_id", "Webhook delivery ID to redeliver."),
        ],
    );
    set_json_response(
        &mut paths,
        "/webhooks",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Webhook" } }),
    );
    set_request_body(
        &mut paths,
        "/webhooks",
        "post",
        json!({ "$ref": "#/components/schemas/WebhookCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/webhooks",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/IdResponse" }),
    );
    set_json_response(
        &mut paths,
        "/webhooks/{webhook_id}",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/Webhook" }),
    );
    set_request_body(
        &mut paths,
        "/webhooks/{webhook_id}",
        "put",
        json!({ "$ref": "#/components/schemas/WebhookCreateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/webhooks/{webhook_id}",
        "put",
        "204",
        "Webhook updated.",
    );
    set_empty_response(
        &mut paths,
        "/webhooks/{webhook_id}",
        "delete",
        "204",
        "Webhook deleted.",
    );
    set_json_response(
        &mut paths,
        "/webhooks/{webhook_id}/deliveries",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/WebhookDelivery" } }),
    );
    set_empty_response(
        &mut paths,
        "/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver",
        "post",
        "202",
        "Webhook delivery redelivery queued.",
    );

    add_path(
        &mut paths,
        "/notifications",
        [("get", "Notifications", "List notifications", true)],
    );
    add_path(
        &mut paths,
        "/notifications/preferences",
        [
            (
                "get",
                "Notifications",
                "List notification preferences",
                true,
            ),
            (
                "put",
                "Notifications",
                "Update notification preferences",
                true,
            ),
        ],
    );
    add_path(
        &mut paths,
        "/notifications/read-all",
        [("post", "Notifications", "Mark all notifications read", true)],
    );
    add_path(
        &mut paths,
        "/notifications/system",
        [("post", "Notifications", "Record authenticated system activity", true)],
    );
    add_path(
        &mut paths,
        "/notifications/{notification_id}/read",
        [("post", "Notifications", "Mark notification read", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/notifications/{notification_id}/read",
        [("notification_id", "Notification ID to mark as read.")],
    );
    set_json_response(
        &mut paths,
        "/notifications",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/Notification" } }),
    );
    if let Some(operation) = paths
        .get_mut("/notifications")
        .and_then(|item| item.get_mut("get"))
    {
        operation["description"] = json!("Lists the current user's notifications in descending creation order. Use the final item's `created_at|id` cursor to retrieve the next page.");
        operation["parameters"] = json!([
            { "name": "limit", "in": "query", "description": "Page size.", "schema": { "type": "integer", "minimum": 1, "maximum": 100, "default": 50 } },
            { "name": "before", "in": "query", "description": "Opaque cursor from the final item of the preceding page (`created_at|id`).", "schema": { "type": "string" } },
            { "name": "unread_only", "in": "query", "description": "Return only unread notifications.", "schema": { "type": "boolean", "default": false } }
        ]);
    }
    set_json_response(
        &mut paths,
        "/notifications/preferences",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/NotificationPreference" } }),
    );
    set_empty_response(
        &mut paths,
        "/notifications/read-all",
        "post",
        "204",
        "All current-user notifications marked read.",
    );
    set_json_response(
        &mut paths,
        "/notifications/system",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/Notification" }),
    );
    set_request_body(
        &mut paths,
        "/notifications/system",
        "post",
        json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "minLength": 1, "maxLength": 160 },
                "body": { "type": "string", "maxLength": 1000 }
            },
            "required": ["title"]
        }),
    );
    set_empty_response(
        &mut paths,
        "/notifications/{notification_id}/read",
        "post",
        "204",
        "Notification marked read.",
    );
    set_request_body(
        &mut paths,
        "/notifications/preferences",
        "put",
        json!({ "$ref": "#/components/schemas/NotificationPreferencesUpdateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/notifications/preferences",
        "put",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/NotificationPreference" } }),
    );

    add_path(
        &mut paths,
        "/ai/config",
        [("get", "AI", "Read AI configuration", true)],
    );
    add_path(
        &mut paths,
        "/ai/chat",
        [("post", "AI", "Chat with configured AI provider", true)],
    );
    add_path(
        &mut paths,
        "/ai/complete",
        [("post", "AI", "Complete selected document text", true)],
    );
    add_path(
        &mut paths,
        "/ai/action",
        [("post", "AI", "Run AI document action", true)],
    );
    set_json_response(
        &mut paths,
        "/ai/config",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/AiConfig" }),
    );
    set_request_body(
        &mut paths,
        "/ai/chat",
        "post",
        json!({ "$ref": "#/components/schemas/AiChatRequest" }),
    );
    set_json_response(
        &mut paths,
        "/ai/chat",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AiChatResponse" }),
    );
    set_request_body(
        &mut paths,
        "/ai/complete",
        "post",
        json!({ "$ref": "#/components/schemas/AiCompleteRequest" }),
    );
    set_json_response(
        &mut paths,
        "/ai/complete",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AiCompleteResponse" }),
    );
    set_request_body(
        &mut paths,
        "/ai/action",
        "post",
        json!({ "$ref": "#/components/schemas/AiDocumentActionRequest" }),
    );
    set_json_response(
        &mut paths,
        "/ai/action",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AiDocumentActionResponse" }),
    );

    Value::Object(paths)
}

fn add_path<const N: usize>(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    operations: [(&str, &str, &str, bool); N],
) {
    let mut item = serde_json::Map::new();
    for (method, tag, summary, secured) in operations {
        item.insert(method.to_string(), operation(tag, summary, secured));
    }
    paths.insert(path.to_string(), Value::Object(item));
}

fn set_uuid_path_parameters<const N: usize>(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    parameters: [(&str, &str); N],
) {
    if let Some(item) = paths.get_mut(path).and_then(Value::as_object_mut) {
        item.insert(
            "parameters".into(),
            Value::Array(
                parameters
                    .into_iter()
                    .map(|(name, description)| {
                        json!({
                            "name": name,
                            "in": "path",
                            "required": true,
                            "description": description,
                            "schema": { "type": "string", "format": "uuid" }
                        })
                    })
                    .collect(),
            ),
        );
    }
}

fn set_string_path_parameters<const N: usize>(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    parameters: [(&str, &str); N],
) {
    if let Some(item) = paths.get_mut(path).and_then(Value::as_object_mut) {
        item.insert(
            "parameters".into(),
            Value::Array(
                parameters
                    .into_iter()
                    .map(|(name, description)| {
                        json!({
                            "name": name,
                            "in": "path",
                            "required": true,
                            "description": description,
                            "schema": { "type": "string" }
                        })
                    })
                    .collect(),
            ),
        );
    }
}

fn set_request_body(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    schema: Value,
) {
    if let Some(operation) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(Value::as_object_mut)
    {
        operation.insert(
            "requestBody".into(),
            json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            }),
        );
    }
}

fn set_multipart_file_request_body(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    file_description: &str,
) {
    if let Some(operation) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(Value::as_object_mut)
    {
        operation.insert(
            "requestBody".into(),
            json!({
                "required": true,
                "content": {
                    "multipart/form-data": {
                        "schema": {
                            "type": "object",
                            "properties": {
                                "file": {
                                    "type": "string",
                                    "format": "binary",
                                    "description": file_description
                                }
                            },
                            "required": ["file"]
                        }
                    }
                }
            }),
        );
    }
}

fn set_json_response(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    status: &str,
    schema: Value,
) {
    if let Some(responses) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(|operation| operation.get_mut("responses"))
        .and_then(Value::as_object_mut)
    {
        responses.insert(
            status.into(),
            json!({
                "description": "Success",
                "content": {
                    "application/json": {
                        "schema": schema
                    }
                }
            }),
        );
    }
}

fn set_empty_response(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    status: &str,
    description: &str,
) {
    if let Some(responses) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(|operation| operation.get_mut("responses"))
        .and_then(Value::as_object_mut)
    {
        responses.insert(status.into(), json!({ "description": description }));
        if status != "200" {
            responses.remove("200");
        }
    }
}

fn set_error_response(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    status: &str,
    description: &str,
) {
    if let Some(responses) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(|operation| operation.get_mut("responses"))
        .and_then(Value::as_object_mut)
    {
        responses.insert(
            status.into(),
            json!({ "description": description, "content": error_content() }),
        );
    }
}

fn set_csv_response(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    status: &str,
    description: &str,
) {
    if let Some(responses) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(|operation| operation.get_mut("responses"))
        .and_then(Value::as_object_mut)
    {
        responses.insert(
            status.into(),
            json!({
                "description": description,
                "content": {
                    "text/csv": {
                        "schema": {
                            "type": "string"
                        }
                    }
                }
            }),
        );
    }
}

fn set_redirect_response(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
    status: &str,
    description: &str,
) {
    if let Some(responses) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(|operation| operation.get_mut("responses"))
        .and_then(Value::as_object_mut)
    {
        responses.insert(
            status.into(),
            json!({
                "description": description,
                "headers": {
                    "Location": {
                        "description": "Redirect target.",
                        "schema": {
                            "type": "string",
                            "format": "uri"
                        }
                    }
                }
            }),
        );
        if status != "200" {
            responses.remove("200");
        }
    }
}

fn operation(tag: &str, summary: &str, secured: bool) -> Value {
    let mut op = json!({
        "tags": [tag],
        "summary": summary,
        "responses": {
            "200": { "description": "Success" },
            "201": { "description": "Created" },
            "202": { "description": "Accepted" },
            "204": { "description": "No content" },
            "400": { "description": "Validation error", "content": error_content() },
            "401": { "description": "Authentication required", "content": error_content() },
            "403": { "description": "Forbidden", "content": error_content() },
            "404": { "description": "Not found", "content": error_content() }
        }
    });

    if secured {
        op["security"] = json!([{ "bearerAuth": [] }]);
    }

    op
}

fn scim_operation(summary: &str) -> Value {
    json!({
        "tags": ["SCIM"],
        "summary": summary,
        "servers": [{ "url": "/scim/v2" }],
        "security": [{ "scimBearerAuth": [] }],
        "responses": {
            "200": { "description": "Success" },
            "401": { "description": "Invalid SCIM bearer token" },
            "404": { "description": "SCIM resource not found" }
        }
    })
}

fn error_content() -> Value {
    json!({
        "application/json": {
            "schema": { "$ref": "#/components/schemas/Error" }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::openapi_spec;
    use serde_json::json;

    #[test]
    fn spec_exposes_current_rest_route_surface() {
        let spec = openapi_spec();
        let paths = spec["paths"].as_object().expect("paths object");
        for path in [
            "/Groups",
            "/Groups/{group_id}",
            "/ResourceTypes",
            "/ResourceTypes/{resource_type}",
            "/Schemas",
            "/Schemas/{schema_id}",
            "/ServiceProviderConfig",
            "/Users",
            "/Users/{user_id}",
            "/admin/api-keys",
            "/admin/api-keys/{key_id}",
            "/admin/audit-log",
            "/admin/audit-log/export",
            "/admin/config",
            "/admin/config/{key}",
            "/admin/groups",
            "/admin/groups/{group_id}/members",
            "/admin/groups/{group_id}/members/{user_id}",
            "/admin/plugin-runtime-audit",
            "/admin/search/reindex",
            "/admin/users",
            "/admin/users/bulk-lifecycle",
            "/admin/users/import",
            "/admin/users/import/preview",
            "/admin/users/import/template",
            "/admin/users/{user_id}",
            "/admin/users/{user_id}/password-reset",
            "/admin/users/{user_id}/sessions",
            "/admin/users/{user_id}/sessions/{session_id}",
            "/admin/workflows/deadline-escalations",
            "/ai/action",
            "/ai/chat",
            "/ai/complete",
            "/ai/config",
            "/auth/2fa/totp/disable",
            "/auth/2fa/totp/setup",
            "/auth/2fa/totp/verify",
            "/auth/api-keys",
            "/auth/api-keys/{key_id}",
            "/auth/config",
            "/auth/login",
            "/auth/logout",
            "/auth/me",
            "/auth/native-handoff/complete",
            "/auth/native-handoff/exchange",
            "/auth/native-handoff/start",
            "/auth/oauth/{provider}",
            "/auth/oauth/{provider}/callback",
            "/auth/oidc",
            "/auth/oidc/callback",
            "/auth/passkeys",
            "/auth/passkeys/login/finish",
            "/auth/passkeys/login/start",
            "/auth/passkeys/register/finish",
            "/auth/passkeys/register/start",
            "/auth/passkeys/{passkey_id}",
            "/auth/password-reset/confirm",
            "/auth/password-reset/request",
            "/auth/refresh",
            "/auth/register",
            "/auth/sessions",
            "/auth/sessions/{session_id}",
            "/dataview",
            "/health",
            "/mcp/manifest",
            "/mcp/tools/{tool_name}",
            "/notifications",
            "/notifications/preferences",
            "/notifications/read-all",
            "/notifications/{notification_id}/read",
            "/p/{slug}",
            "/search",
            "/spaces",
            "/spaces/{space_id}",
            "/spaces/{space_id}/documents",
            "/spaces/{space_id}/documents/{doc_id}",
            "/spaces/{space_id}/documents/{doc_id}/attachments",
            "/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}",
            "/spaces/{space_id}/documents/{doc_id}/backlinks",
            "/spaces/{space_id}/documents/{doc_id}/comments",
            "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}",
            "/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve",
            "/spaces/{space_id}/documents/{doc_id}/content",
            "/spaces/{space_id}/documents/{doc_id}/lock",
            "/spaces/{space_id}/documents/{doc_id}/move",
            "/spaces/{space_id}/documents/{doc_id}/properties",
            "/spaces/{space_id}/documents/{doc_id}/publish",
            "/spaces/{space_id}/documents/{doc_id}/reactions",
            "/spaces/{space_id}/documents/{doc_id}/star",
            "/spaces/{space_id}/documents/{doc_id}/tags",
            "/spaces/{space_id}/documents/{doc_id}/tags/{tag_id}",
            "/spaces/{space_id}/documents/{doc_id}/unlock",
            "/spaces/{space_id}/documents/{doc_id}/unpublish",
            "/spaces/{space_id}/documents/{doc_id}/versions",
            "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}",
            "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore",
            "/spaces/{space_id}/documents/{document_id}/workflow",
            "/spaces/{space_id}/documents/{document_id}/workflow/events",
            "/spaces/{space_id}/documents/{document_id}/workflow/transitions",
            "/spaces/{space_id}/documents/starred",
            "/spaces/{space_id}/folders",
            "/spaces/{space_id}/folders/{folder_id}",
            "/spaces/{space_id}/folders/{folder_id}/move",
            "/spaces/{space_id}/folders/{folder_id}/reorder",
            "/spaces/{space_id}/graph",
            "/spaces/{space_id}/groups",
            "/spaces/{space_id}/groups/{group_id}",
            "/spaces/{space_id}/members",
            "/spaces/{space_id}/members/{user_id}",
            "/spaces/{space_id}/properties",
            "/spaces/{space_id}/properties/reorder",
            "/spaces/{space_id}/properties/{property_id}",
            "/spaces/{space_id}/tags",
            "/spaces/{space_id}/tags/{tag_id}",
            "/spaces/{space_id}/templates",
            "/spaces/{space_id}/templates/{template_id}",
            "/spaces/{space_id}/workflow-queue",
            "/spaces/{space_id}/workflow-templates",
            "/spaces/{space_id}/workflow-templates/{template_id}",
            "/sync/replay",
            "/sync/replay/history",
            "/sync/replay/preflight",
            "/users/directory",
            "/webhooks",
            "/webhooks/{webhook_id}",
            "/webhooks/{webhook_id}/deliveries",
            "/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver",
        ] {
            assert!(paths.contains_key(path), "missing OpenAPI path {path}");
        }
    }

    #[test]
    fn secured_operations_include_bearer_auth() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces"]["get"]["security"][0]["bearerAuth"]
                .as_array()
                .expect("bearer security")
                .len(),
            0
        );
        assert!(spec["paths"]["/auth/login"]["post"]["security"].is_null());
    }

    #[test]
    fn health_route_includes_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/health"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/HealthResponse"
        );
        assert_eq!(
            spec["paths"]["/health"]["get"]["responses"]["503"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/HealthUnavailableResponse"
        );
        assert_eq!(
            spec["paths"]["/health"]["get"]["responses"]["503"]["description"],
            "Database unavailable."
        );
    }

    #[test]
    fn starred_and_workflow_routes_match_implemented_verbs() {
        let spec = openapi_spec();
        let paths = spec["paths"].as_object().expect("paths object");
        assert!(paths["/spaces/{space_id}/documents/{doc_id}/star"]["delete"].is_object());
        assert!(paths["/spaces/{space_id}/documents/{document_id}/workflow"]["put"].is_object());
        assert!(
            paths["/spaces/{space_id}/documents/{document_id}/workflow/transitions"]["post"]
                .is_object()
        );
        assert!(paths["/admin/workflows/deadline-escalations"]["post"].is_object());
        assert!(
            !paths.contains_key("/spaces/{space_id}/documents/{document_id}/workflow/transition")
        );
        assert!(paths["/spaces/{space_id}/documents/{doc_id}/move"]["post"].is_object());
        assert!(paths["/spaces/{space_id}/folders/{folder_id}/move"]["post"].is_object());
    }

    #[test]
    fn document_publish_route_includes_json_schema() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/publish"]["post"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/PublishDocumentResponse"
        );
    }

    #[test]
    fn space_and_folder_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["items"]["$ref"],
            "#/components/schemas/Space"
        );
        assert_eq!(
            spec["components"]["schemas"]["Space"]["properties"]["owner_display_name"]["type"],
            "string"
        );
        assert_eq!(
            spec["components"]["schemas"]["Space"]["properties"]["owner_username"]["format"],
            "email"
        );
        assert!(spec["components"]["schemas"]["Space"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("created_by"))));
        assert!(spec["components"]["schemas"]["Space"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("description"))
                    && required.contains(&serde_json::json!("icon"))
                    && required.contains(&serde_json::json!("accent_color"))
                    && required.contains(&serde_json::json!("member_role"))
            ));
        assert_eq!(
            spec["components"]["schemas"]["Space"]["properties"]["description"]["nullable"],
            true
        );
        assert_eq!(
            spec["paths"]["/spaces"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/SpaceCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces"]["post"]["responses"]["409"]["description"],
            "Space name or import ID already exists."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SpaceUpdateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}"]["put"]["responses"]["204"]["description"],
            "Space updated."
        );
        assert!(spec["paths"]["/spaces/{space_id}"]["put"]["responses"]["200"].is_null());
        assert_eq!(
            spec["paths"]["/spaces/{space_id}"]["delete"]["responses"]["204"]["description"],
            "Space deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Folder"
        );
        assert!(spec["components"]["schemas"]["Folder"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("parent_id"))
                    && required.contains(&serde_json::json!("icon"))
            ));
        assert_eq!(
            spec["components"]["schemas"]["Folder"]["properties"]["parent_id"]["nullable"],
            true
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/FolderCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders"]["post"]["responses"]["409"]["description"],
            "Folder import ID already exists."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}"]["put"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/FolderUpdateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}"]["parameters"][1]["name"],
            "folder_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}"]["put"]["responses"]["204"]
                ["description"],
            "Folder updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}"]["delete"]["responses"]["204"]
                ["description"],
            "Folder deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}/move"]["post"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/FolderMoveRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["FolderMoveRequest"]["properties"]["target_space_id"]
                ["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}/reorder"]["put"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/FolderReorderRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}/reorder"]["parameters"][1]
                ["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/folders/{folder_id}/reorder"]["put"]["responses"]
                ["204"]["description"],
            "Folder reordered."
        );
    }

    #[test]
    fn user_group_and_membership_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/users/directory"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/UserDirectoryEntry"
        );
        assert!(
            spec["components"]["schemas"]["UserDirectoryEntry"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("avatar_url"))
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/SpaceMember"
        );
        assert!(spec["components"]["schemas"]["SpaceMember"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("avatar_url")));
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SpaceMemberRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members"]["post"]["responses"]["201"]["description"],
            "Space member added."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members/{user_id}"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SpaceMemberRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members/{user_id}"]["parameters"][1]["name"],
            "user_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members/{user_id}"]["put"]["responses"]["204"]
                ["description"],
            "Space member role updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/members/{user_id}"]["delete"]["responses"]["204"]
                ["description"],
            "Space member removed."
        );
        assert_eq!(
            spec["paths"]["/admin/groups"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Group"
        );
        assert!(spec["components"]["schemas"]["Group"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("description")));
        assert_eq!(
            spec["paths"]["/admin/groups"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/GroupCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}/members"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/GroupMember"
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}/members"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/GroupMemberAddRequest"
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}/members"]["parameters"][0]["name"],
            "group_id"
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}/members"]["post"]["responses"]["201"]
                ["description"],
            "Group member added."
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}/members/{user_id}"]["delete"]["responses"]
                ["204"]["description"],
            "Group member removed."
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}/members/{user_id}"]["parameters"][1]["name"],
            "user_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/SpaceGroupGrant"
        );
        assert!(spec["components"]["schemas"]["SpaceGroupGrant"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("description")));
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SpaceGroupGrantRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups"]["post"]["responses"]["201"]["description"],
            "Space group access granted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups/{group_id}"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SpaceGroupGrantRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups/{group_id}"]["parameters"][1]["name"],
            "group_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups/{group_id}"]["put"]["responses"]["204"]
                ["description"],
            "Space group access updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/groups/{group_id}"]["delete"]["responses"]["204"]
                ["description"],
            "Space group access removed."
        );
    }

    #[test]
    fn core_document_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Document"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents"]["post"]["responses"]["201"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/Document"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents"]["post"]["responses"]["409"]
                ["description"],
            "Document import ID already exists."
        );
        assert_eq!(
            spec["components"]["schemas"]["DocumentCreateRequest"]["properties"]["parent_id"]
                ["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}"]["get"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/Document"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentUpdateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}"]["parameters"][1]["name"],
            "doc_id"
        );
        assert_eq!(
            spec["components"]["schemas"]["DocumentUpdateRequest"]["properties"]["parent_id"]
                ["format"],
            "uuid"
        );
        assert_eq!(
            spec["components"]["schemas"]["Document"]["properties"]["parent_id"]["format"],
            "uuid"
        );
        assert!(spec["components"]["schemas"]["Document"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("folder_id"))
                    && required.contains(&serde_json::json!("parent_id"))
                    && required.contains(&serde_json::json!("icon"))
                    && required.contains(&serde_json::json!("cover_image"))
                    && required.contains(&serde_json::json!("locked_by"))
                    && required.contains(&serde_json::json!("public_slug"))
                    && required.contains(&serde_json::json!("workflow_template_id"))
                    && required.contains(&serde_json::json!("workflow_template_name"))
                    && required.contains(&serde_json::json!("workflow_stage_id"))
                    && required.contains(&serde_json::json!("workflow_stage_name"))
                    && required.contains(&serde_json::json!("workflow_status"))
            ));
        assert_eq!(
            spec["components"]["schemas"]["Document"]["properties"]["workflow_status"]["nullable"],
            true
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}"]["put"]["responses"]["204"]
                ["description"],
            "Document updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}"]["delete"]["responses"]["204"]
                ["description"],
            "Document deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/move"]["post"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentMoveRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["DocumentMoveRequest"]["properties"]["target_space_id"]
                ["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/content"]["get"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentContent"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/content"]["put"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentContentUpdateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/content"]["parameters"][1]
                ["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/content"]["put"]["responses"]
                ["204"]["description"],
            "Document content updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/starred"]["get"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/StarredDocument"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/starred"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["components"]["schemas"]["StarredDocument"]["required"][4],
            "icon"
        );
        assert_eq!(
            spec["components"]["schemas"]["StarredDocument"]["properties"]["icon"]["nullable"],
            true
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/star"]["post"]["responses"]["204"]
                ["description"],
            "Document starred."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/star"]["parameters"][1]["name"],
            "doc_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/star"]["delete"]["responses"]
                ["204"]["description"],
            "Document unstarred."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/lock"]["post"]["responses"]["204"]
                ["description"],
            "Document locked."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/lock"]["post"]["responses"]["409"]
                ["description"],
            "Document is locked by another user."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/lock"]["parameters"][1]["schema"]
                ["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/unlock"]["post"]["responses"]
                ["204"]["description"],
            "Document unlocked."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/unlock"]["parameters"][1]["name"],
            "doc_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/publish"]["parameters"][1]
                ["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/unpublish"]["post"]["responses"]
                ["204"]["description"],
            "Document unpublished."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/unpublish"]["parameters"][1]
                ["name"],
            "doc_id"
        );
        assert_eq!(
            spec["paths"]["/p/{slug}"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/PublicPage"
        );
        assert_eq!(spec["paths"]["/p/{slug}"]["parameters"][0]["name"], "slug");
        assert_eq!(
            spec["paths"]["/p/{slug}"]["parameters"][0]["schema"]["type"],
            "string"
        );
        assert!(spec["components"]["schemas"]["PublicPage"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("icon"))
                && required.contains(&serde_json::json!("cover_image"))
                && required.contains(&serde_json::json!("space_accent_color"))));
        assert_eq!(
            spec["components"]["schemas"]["PublicPage"]["properties"]["space_accent_color"]
                ["nullable"],
            true
        );
    }

    #[test]
    fn workflow_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow"]["get"]
                ["responses"]["200"]["content"]["application/json"]["schema"]["oneOf"][0]["$ref"],
            "#/components/schemas/DocumentWorkflow"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow"]["put"]
                ["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentWorkflowEnableRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow"]["delete"]
                ["responses"]["204"]["description"],
            "Active document workflow cleared."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow"]["parameters"][1]
                ["name"],
            "document_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow/transitions"]
                ["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentWorkflowTransitionRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow/transitions"]
                ["parameters"][1]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow/transitions"]
                ["post"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentWorkflow"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow/events"]["get"]
                ["responses"]["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/DocumentWorkflowEvent"
        );
        assert!(
            spec["components"]["schemas"]["DocumentWorkflowEvent"]["required"]
                .as_array()
                .is_some_and(
                    |required| required.contains(&serde_json::json!("from_stage_id"))
                        && required.contains(&serde_json::json!("note"))
                )
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow/events"]["post"]
                ["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentWorkflowEventRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{document_id}/workflow/events"]
                ["parameters"][1]["name"],
            "document_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-queue"]["get"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Document"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-queue"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/admin/workflows/deadline-escalations"]["get"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/WorkflowDeadlineEscalationCandidate"
        );
        assert_eq!(
            spec["paths"]["/admin/workflows/deadline-escalations"]["post"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/WorkflowDeadlineEscalationRunResponse"
        );
    }

    #[test]
    fn comment_and_reaction_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments"]["get"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/DocumentComment"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments"]["post"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentCommentCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments"]["parameters"][1]
                ["name"],
            "doc_id"
        );
        assert!(spec["components"]["schemas"]["DocumentComment"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("parent_id"))
                    && required.contains(&serde_json::json!("selection_start"))
                    && required.contains(&serde_json::json!("selection_end"))
                    && required.contains(&serde_json::json!("selection_text"))
                    && required.contains(&serde_json::json!("resolved_by"))
            ));
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}"]["put"]
                ["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentCommentUpdateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}"]
                ["parameters"][2]["name"],
            "comment_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}"]["put"]
                ["responses"]["204"]["description"],
            "Document comment updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}"]["delete"]
                ["responses"]["204"]["description"],
            "Document comment deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve"]
                ["post"]["responses"]["204"]["description"],
            "Document comment resolved."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/comments/{comment_id}/resolve"]
                ["parameters"][2]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/reactions"]["get"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/DocumentReaction"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/reactions"]["parameters"][1]
                ["name"],
            "doc_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/reactions"]["post"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentReactionToggleRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/reactions"]["post"]["responses"]
                ["204"]["description"],
            "Document reaction toggled."
        );
    }

    #[test]
    fn version_and_attachment_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/versions"]["get"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/DocumentVersion"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/versions"]["parameters"][1]
                ["name"],
            "doc_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/versions/{version_id}"]
                ["parameters"][2]["name"],
            "version_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore"]
                ["post"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/RestoreDocumentVersionResponse"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore"]
                ["parameters"][2]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments"]["get"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/DocumentAttachment"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments"]["parameters"][1]
                ["name"],
            "doc_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments"]["post"]
                ["requestBody"]["content"]["multipart/form-data"]["schema"]["properties"]["file"]
                ["format"],
            "binary"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments"]["post"]["responses"]
                ["201"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentAttachment"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}"]
                ["delete"]["responses"]["204"]["description"],
            "Document attachment deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}"]
                ["parameters"][2]["name"],
            "attachment_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}"]
                ["parameters"][2]["schema"]["format"],
            "uuid"
        );
        assert!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}"]
                ["get"]
                .is_null()
        );
    }

    #[test]
    fn admin_user_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert!(spec["components"]["schemas"]["User"].is_object());
        assert!(spec["components"]["schemas"]["User"]["properties"]["avatar_url"].is_object());
        assert_eq!(
            spec["components"]["schemas"]["User"]["properties"]["updated_at"]["format"],
            "date-time"
        );
        assert!(spec["components"]["schemas"]["User"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("avatar_url"))));
        assert_eq!(
            spec["paths"]["/admin/users"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["items"]["$ref"],
            "#/components/schemas/User"
        );
        assert_eq!(
            spec["paths"]["/admin/users"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AdminUserCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/admin/users"]["post"]["responses"]["201"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AdminUserCreateResponse"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AdminUserUpdateRequest"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}"]["put"]["responses"]["204"]["description"],
            "User lifecycle updated."
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}"]["parameters"][0]["name"],
            "user_id"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert!(spec["paths"]["/admin/users/{user_id}"]["put"]["responses"]["200"].is_null());
        assert_eq!(
            spec["paths"]["/admin/users/bulk-lifecycle"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AdminUserBulkLifecycleRequest"
        );
        assert_eq!(
            spec["paths"]["/admin/users/bulk-lifecycle"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AdminUserBulkLifecycleResponse"
        );
        assert_eq!(
            spec["components"]["schemas"]["AdminUserBulkLifecycleSkippedUser"]["properties"]
                ["reason"]["enum"][6],
            "last_active_admin_required"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}/password-reset"]["post"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/AdminPasswordResetResponse"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}/password-reset"]["parameters"][0]["name"],
            "user_id"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}/sessions"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/UserSession"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}/sessions"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}/sessions/{session_id}"]["delete"]["responses"]
                ["204"]["description"],
            "User session revoked."
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}/sessions/{session_id}"]["parameters"][0]["name"],
            "user_id"
        );
        assert_eq!(
            spec["paths"]["/admin/users/{user_id}/sessions/{session_id}"]["parameters"][1]["name"],
            "session_id"
        );
    }

    #[test]
    fn admin_user_import_routes_include_multipart_and_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/admin/users/import/preview"]["post"]["requestBody"]["content"]
                ["multipart/form-data"]["schema"]["properties"]["file"]["format"],
            "binary"
        );
        assert_eq!(
            spec["paths"]["/admin/users/import/preview"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AdminUserImportPreview"
        );
        assert_eq!(
            spec["paths"]["/admin/users/import"]["post"]["requestBody"]["content"]
                ["multipart/form-data"]["schema"]["required"][0],
            "file"
        );
        assert_eq!(
            spec["paths"]["/admin/users/import"]["post"]["responses"]["201"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AdminUserImportResult"
        );
    }

    #[test]
    fn audit_and_csv_routes_include_response_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/admin/audit-log"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/AuditLogEntry"
        );
        assert!(spec["paths"]["/admin/audit-log"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "action")));
        assert!(spec["paths"]["/admin/audit-log"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "user_id")));
        assert!(spec["paths"]["/admin/audit-log"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "target_type")));
        assert_eq!(
            spec["paths"]["/admin/audit-log"]["get"]["parameters"][3]["schema"]["maximum"],
            200
        );
        assert_eq!(
            spec["paths"]["/admin/audit-log"]["get"]["parameters"][3]["schema"]["minimum"],
            0
        );
        assert_eq!(
            spec["paths"]["/admin/audit-log"]["get"]["parameters"][3]["schema"]["default"],
            50
        );
        assert_eq!(
            spec["paths"]["/admin/audit-log"]["get"]["parameters"][4]["name"],
            "offset"
        );
        assert_eq!(
            spec["paths"]["/admin/audit-log"]["get"]["parameters"][4]["schema"]["minimum"],
            0
        );
        assert_eq!(
            spec["paths"]["/admin/audit-log"]["get"]["parameters"][4]["schema"]["default"],
            0
        );
        assert_eq!(
            spec["paths"]["/admin/audit-log/export"]["get"]["responses"]["200"]["content"]
                ["text/csv"]["schema"]["type"],
            "string"
        );
        assert!(spec["components"]["schemas"]["AuditLogEntry"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("user_id"))
                && required.contains(&serde_json::json!("user_email"))
                && required.contains(&serde_json::json!("ip_address"))
                && required.contains(&serde_json::json!("target_type"))
                && required.contains(&serde_json::json!("target_id"))));
        assert_eq!(
            spec["paths"]["/admin/plugin-runtime-audit"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PluginRuntimeAuditRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["PluginRuntimeAuditRequest"]["properties"]["plugin_id"]
                ["pattern"],
            "\\S"
        );
        assert_eq!(
            spec["paths"]["/admin/plugin-runtime-audit"]["post"]["responses"]["400"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/Error"
        );
        assert_eq!(
            spec["paths"]["/admin/plugin-runtime-audit"]["post"]["responses"]["204"]["description"],
            "Plugin runtime audit event recorded."
        );
        assert_eq!(
            spec["paths"]["/admin/users/import/template"]["get"]["responses"]["200"]["content"]
                ["text/csv"]["schema"]["type"],
            "string"
        );
    }

    #[test]
    fn admin_config_and_search_reindex_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/admin/config"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/AdminConfigEntry"
        );
        assert_eq!(
            spec["paths"]["/admin/config"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AdminConfigUpsertRequest"
        );
        assert_eq!(
            spec["paths"]["/admin/config"]["post"]["responses"]["204"]["description"],
            "Admin configuration saved."
        );
        assert_eq!(
            spec["paths"]["/admin/config/{key}"]["delete"]["responses"]["204"]["description"],
            "Admin configuration entry deleted."
        );
        assert_eq!(
            spec["paths"]["/admin/config/{key}"]["parameters"][0]["name"],
            "key"
        );
        assert_eq!(
            spec["paths"]["/admin/config/{key}"]["parameters"][0]["schema"]["type"],
            "string"
        );
        assert_eq!(
            spec["paths"]["/admin/search/reindex"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SearchReindexResponse"
        );
        assert_eq!(
            spec["components"]["schemas"]["AdminConfigEntry"]["properties"]["source"]["enum"][0],
            "env"
        );
        assert!(
            spec["components"]["schemas"]["AdminConfigEntry"]["required"]
                .as_array()
                .is_some_and(|required| required.contains(&serde_json::json!("env_var")))
        );
    }

    #[test]
    fn webhook_and_notification_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/webhooks"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["items"]["$ref"],
            "#/components/schemas/Webhook"
        );
        assert_eq!(
            spec["paths"]["/webhooks"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/WebhookCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}/deliveries"]["get"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/WebhookDelivery"
        );
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}/deliveries"]["parameters"][0]["name"],
            "webhook_id"
        );
        assert!(spec["components"]["schemas"]["Webhook"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("space_id"))));
        assert!(spec["components"]["schemas"]["WebhookDelivery"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("response_status"))));
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver"]["post"]
                ["responses"]["202"]["description"],
            "Webhook delivery redelivery queued."
        );
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver"]
                ["parameters"][0]["name"],
            "webhook_id"
        );
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}/deliveries/{delivery_id}/redeliver"]
                ["parameters"][1]["name"],
            "delivery_id"
        );
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}"]["put"]["responses"]["204"]["description"],
            "Webhook updated."
        );
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}"]["delete"]["responses"]["204"]["description"],
            "Webhook deleted."
        );
        assert_eq!(
            spec["paths"]["/webhooks/{webhook_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/notifications"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Notification"
        );
        assert_eq!(
            spec["paths"]["/notifications"]["get"]["parameters"][0]["name"],
            "limit"
        );
        assert_eq!(
            spec["paths"]["/notifications"]["get"]["parameters"][0]["schema"]["default"],
            50
        );
        assert_eq!(
            spec["paths"]["/notifications"]["get"]["parameters"][0]["schema"]["maximum"],
            100
        );
        assert_eq!(
            spec["paths"]["/notifications"]["get"]["parameters"][1]["name"],
            "before"
        );
        assert_eq!(
            spec["paths"]["/notifications"]["get"]["parameters"][2]["name"],
            "unread_only"
        );
        assert_eq!(
            spec["paths"]["/notifications"]["get"]["parameters"][2]["schema"]["default"],
            false
        );
        assert!(spec["components"]["schemas"]["Notification"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("body"))
                && required.contains(&serde_json::json!("source_user"))
                && required.contains(&serde_json::json!("target_type"))
                && required.contains(&serde_json::json!("target_id"))));
        assert_eq!(
            spec["paths"]["/notifications/preferences"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/NotificationPreferencesUpdateRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["NotificationPreference"]["properties"]
                ["notification_type"]["enum"][0],
            "workflow_approval"
        );
        assert_eq!(
            spec["components"]["schemas"]["NotificationPreferencesUpdateRequest"]["properties"]
                ["preferences"]["items"]["properties"]["notification_type"]["enum"][12],
            "system"
        );
        assert_eq!(
            spec["paths"]["/notifications/preferences"]["put"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/NotificationPreference"
        );
        assert_eq!(
            spec["paths"]["/notifications/{notification_id}/read"]["post"]["responses"]["204"]
                ["description"],
            "Notification marked read."
        );
        assert_eq!(
            spec["paths"]["/notifications/{notification_id}/read"]["parameters"][0]["name"],
            "notification_id"
        );
        assert_eq!(
            spec["paths"]["/notifications/{notification_id}/read"]["parameters"][0]["schema"]
                ["format"],
            "uuid"
        );
    }

    #[test]
    fn auth_session_and_api_key_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/auth/logout"]["post"]["responses"]["204"]["description"],
            "Current refresh session revoked."
        );
        assert_eq!(
            spec["paths"]["/auth/register"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/RegisterRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/register"]["post"]["responses"]["201"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AuthResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/register"]["post"]["responses"]["403"]["description"],
            "Public signups are disabled."
        );
        assert_eq!(
            spec["paths"]["/auth/register"]["post"]["responses"]["409"]["description"],
            "Email already registered."
        );
        assert_eq!(
            spec["paths"]["/auth/login"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/LoginRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/login"]["post"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AuthResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/refresh"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/RefreshTokenRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/refresh"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/TokenRefreshResponse"
        );
        assert_eq!(
            spec["components"]["schemas"]["AuthResponse"]["properties"]["user"]["$ref"],
            "#/components/schemas/CurrentUserProfile"
        );
        assert_eq!(
            spec["paths"]["/auth/config"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AuthConfigResponse"
        );
        assert_eq!(
            spec["components"]["schemas"]["AuthConfigResponse"]["properties"]
                ["allow_public_signups"]["type"],
            "boolean"
        );
        assert_eq!(
            spec["components"]["schemas"]["AuthConfigResponse"]["properties"]["max_upload_size_mb"]
                ["minimum"],
            1
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/setup"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/TotpSetupResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/setup"]["post"]["responses"]["409"]["description"],
            "TOTP is already enabled."
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/verify"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/TotpVerifyRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/verify"]["post"]["security"][0]["bearerAuth"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/verify"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/TotpStatusResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/verify"]["post"]["responses"]["429"]["description"],
            "Rate limit exceeded."
        );
        assert_eq!(
            spec["paths"]["/auth/login"]["post"]["responses"]["429"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/Error"
        );
        assert_eq!(
            spec["components"]["schemas"]["Error"]["required"],
            json!(["error", "message"])
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/disable"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/TotpVerifyRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/disable"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/TotpStatusResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/2fa/totp/disable"]["post"]["responses"]["403"]["description"],
            "Two-factor authentication is required by this server."
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Passkey"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/register/start"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasskeyRegisterStartRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/register/start"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasskeyChallengeResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/register/finish"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasskeyRegisterFinishRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/register/finish"]["post"]["responses"]["201"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/Passkey"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/register/finish"]["post"]["responses"]["409"]
                ["description"],
            "Passkey already registered."
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/{passkey_id}"]["parameters"][0]["name"],
            "passkey_id"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/{passkey_id}"]["delete"]["responses"]["204"]
                ["description"],
            "Passkey deleted."
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/login/start"]["post"]["security"],
            json!(null)
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/login/start"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasskeyLoginStartRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/login/start"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasskeyChallengeResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/login/finish"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasskeyLoginFinishRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/login/finish"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AuthResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/passkeys/login/start"]["post"]["responses"]["429"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/Error"
        );
        assert_eq!(
            spec["paths"]["/auth/me"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/CurrentUserProfile"
        );
        assert_eq!(
            spec["paths"]["/auth/me"]["put"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/UpdateProfileRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/me"]["put"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/CurrentUserProfile"
        );
        assert_eq!(
            spec["paths"]["/auth/me"]["put"]["responses"]["409"]["description"],
            "Username is already in use."
        );
        assert_eq!(
            spec["components"]["schemas"]["CurrentUserProfile"]["properties"]["username"]
                ["pattern"],
            "^[A-Za-z0-9_-]{1,64}$"
        );
        assert!(
            spec["components"]["schemas"]["CurrentUserProfile"]["required"]
                .as_array()
                .is_some_and(|required| required.contains(&serde_json::json!("username"))
                    && required.contains(&serde_json::json!("avatar_url")))
        );
        assert_eq!(
            spec["paths"]["/auth/password-reset/request"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasswordResetRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/password-reset/request"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasswordResetRequestResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/password-reset/confirm"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PasswordResetConfirmRequest"
        );
        assert_eq!(
            spec["paths"]["/auth/password-reset/confirm"]["post"]["responses"]["204"]
                ["description"],
            "Password reset confirmed."
        );
        assert_eq!(
            spec["paths"]["/auth/password-reset/confirm"]["post"]["responses"]["429"]
                ["description"],
            "Rate limit exceeded."
        );
        assert_eq!(
            spec["paths"]["/auth/sessions"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/UserSession"
        );
        assert_eq!(
            spec["paths"]["/auth/sessions/{session_id}"]["delete"]["responses"]["204"]
                ["description"],
            "Session revoked."
        );
        assert_eq!(
            spec["paths"]["/auth/sessions/{session_id}"]["parameters"][0]["name"],
            "session_id"
        );
        assert_eq!(
            spec["paths"]["/auth/sessions/{session_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert!(spec["components"]["schemas"]["UserSession"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("device_name"))
                    && required.contains(&serde_json::json!("ip_address"))
                    && required.contains(&serde_json::json!("user_agent"))
            ));
        assert_eq!(
            spec["paths"]["/auth/api-keys"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/ApiKey"
        );
        assert_eq!(
            spec["paths"]["/auth/api-keys"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ApiKeyCreateRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["ApiKey"]["properties"]["scopes"]["items"]["enum"][2],
            "admin"
        );
        assert!(spec["components"]["schemas"]["ApiKey"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("last_used_at"))
                    && required.contains(&serde_json::json!("expires_at"))
            ));
        assert_eq!(
            spec["components"]["schemas"]["ApiKeyCreateRequest"]["properties"]["scopes"]["default"]
                [1],
            "write"
        );
        assert_eq!(
            spec["paths"]["/auth/api-keys"]["post"]["responses"]["201"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/ApiKeyCreateResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/api-keys/{key_id}"]["delete"]["responses"]["204"]["description"],
            "API key revoked."
        );
        assert_eq!(
            spec["paths"]["/auth/api-keys/{key_id}"]["parameters"][0]["name"],
            "key_id"
        );
        assert_eq!(
            spec["paths"]["/auth/api-keys/{key_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/admin/api-keys"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/ApiKey"
        );
        assert_eq!(
            spec["paths"]["/admin/api-keys"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ServiceApiKeyCreateRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["ServiceApiKeyCreateRequest"]["properties"]["scopes"]
                ["default"][0],
            "admin"
        );
        assert_eq!(
            spec["paths"]["/admin/api-keys/{key_id}"]["delete"]["responses"]["204"]["description"],
            "Service account API key revoked."
        );
        assert_eq!(
            spec["paths"]["/admin/api-keys/{key_id}"]["parameters"][0]["name"],
            "key_id"
        );
        assert_eq!(
            spec["paths"]["/admin/api-keys/{key_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
    }

    #[test]
    fn auth_redirect_routes_include_location_headers() {
        let spec = openapi_spec();
        let redirect_routes = [
            (
                "/auth/oauth/{provider}",
                "Redirect to the OAuth provider authorization URL.",
            ),
            (
                "/auth/oauth/{provider}/callback",
                "Redirect to the Starlane app with access and refresh tokens in the URL fragment.",
            ),
            (
                "/auth/oidc",
                "Redirect to the OIDC provider authorization URL.",
            ),
            (
                "/auth/oidc/callback",
                "Redirect to the Starlane app with access and refresh tokens in the URL fragment.",
            ),
        ];

        for (path, description) in redirect_routes {
            assert_eq!(
                spec["paths"][path]["get"]["responses"]["307"]["description"],
                description
            );
            assert_eq!(
                spec["paths"][path]["get"]["responses"]["307"]["headers"]["Location"]["schema"]
                    ["format"],
                "uri"
            );
            assert!(spec["paths"][path]["get"]["responses"]["200"].is_null());
        }
        assert_eq!(
            spec["paths"]["/auth/oauth/{provider}"]["parameters"][0]["name"],
            "provider"
        );
        assert_eq!(
            spec["paths"]["/auth/oauth/{provider}/callback"]["parameters"][0]["schema"]["type"],
            "string"
        );
    }

    #[test]
    fn scim_routes_describe_the_implemented_provisioning_contract() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/Users"]["post"]["servers"][0]["url"],
            "/scim/v2"
        );
        assert_eq!(
            spec["paths"]["/Users"]["post"]["security"][0]["scimBearerAuth"],
            serde_json::json!([])
        );
        assert_eq!(
            spec["paths"]["/Users"]["post"]["requestBody"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/ScimUserCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/Users"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ScimUserListResponse"
        );
        assert_eq!(
            spec["paths"]["/Users"]["get"]["parameters"][0]["name"],
            "filter"
        );
        assert_eq!(
            spec["paths"]["/Users"]["get"]["parameters"][0]["description"],
            "Optional SCIM lookup filter. Supported forms are `userName eq \"person@example.com\"` and `externalId eq \"idp-user-123\"`."
        );
        assert_eq!(
            spec["paths"]["/Users/{user_id}"]["patch"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/ScimPatchRequest"
        );
        assert_eq!(
            spec["paths"]["/Users/{user_id}"]["patch"]["responses"]["409"]["description"],
            "Duplicate email"
        );
        assert_eq!(
            spec["paths"]["/Users/{user_id}"]["parameters"][0]["name"],
            "user_id"
        );
        assert_eq!(
            spec["paths"]["/Users/{user_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/Users/{user_id}"]["put"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/ScimUser"
        );
        assert_eq!(
            spec["paths"]["/Users/{user_id}"]["delete"]["responses"]["204"]["description"],
            "SCIM user deprovisioned"
        );
        assert_eq!(
            spec["components"]["schemas"]["ScimPatchRequest"]["properties"]["Operations"]
                ["maxItems"],
            1
        );
        assert!(spec["components"]["schemas"]["ScimUser"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("externalId"))));
        assert_eq!(
            spec["paths"]["/Groups"]["get"]["servers"][0]["url"],
            "/scim/v2"
        );
        assert_eq!(
            spec["paths"]["/Groups"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ScimGroupCreateRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["ScimGroup"]["properties"]["members"]["type"],
            "array"
        );
        assert!(spec["components"]["schemas"]["ScimGroup"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("externalId"))
                    && required.contains(&serde_json::json!("description"))
            ));
        assert_eq!(
            spec["paths"]["/Groups/{group_id}"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/ScimGroup"
        );
        assert_eq!(
            spec["paths"]["/Groups/{group_id}"]["parameters"][0]["name"],
            "group_id"
        );
        assert_eq!(
            spec["paths"]["/Groups/{group_id}"]["parameters"][0]["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/Groups/{group_id}"]["patch"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/ScimGroupMembershipPatchRequest"
        );
        assert_eq!(
            spec["paths"]["/Groups/{group_id}"]["put"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/ScimGroup"
        );
        assert_eq!(
            spec["paths"]["/Groups/{group_id}"]["delete"]["responses"]["204"]["description"],
            "SCIM group deleted"
        );
        assert_eq!(
            spec["paths"]["/Groups"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ScimGroupListResponse"
        );
    }

    #[test]
    fn metadata_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/tags"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Tag"
        );
        assert!(spec["components"]["schemas"]["Tag"]["required"]
            .as_array()
            .is_some_and(
                |required| required.contains(&serde_json::json!("created_at"))
                    && required.contains(&serde_json::json!("doc_count"))
            ));
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/tags"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/TagCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/tags"]["post"]["responses"]["409"]["description"],
            "Tag import ID already exists."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/tags"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/tags/{tag_id}"]["put"]["responses"]["204"]
                ["description"],
            "Tag updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/tags/{tag_id}"]["parameters"][1]["name"],
            "tag_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/tags/{tag_id}"]["delete"]["responses"]["204"]
                ["description"],
            "Tag deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/tags"]["post"]["responses"]["201"]
                ["description"],
            "Document tag attached."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/tags"]["parameters"][1]["name"],
            "doc_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/tags/{tag_id}"]["delete"]
                ["responses"]["204"]["description"],
            "Document tag removed."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/tags/{tag_id}"]["parameters"][2]
                ["schema"]["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/PropertySchema"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PropertySchemaCreateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties"]["post"]["responses"]["409"]
                ["description"],
            "Property schema import ID already exists."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties"]["parameters"][0]["name"],
            "space_id"
        );
        assert!(spec["components"]["schemas"]["PropertySchema"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("options"))));
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties/reorder"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/PropertySchemaReorderRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties/reorder"]["parameters"][0]["schema"]
                ["format"],
            "uuid"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties/{property_id}"]["put"]["responses"]["204"]
                ["description"],
            "Property schema updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties/{property_id}"]["parameters"][1]["name"],
            "property_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties/{property_id}"]["delete"]["responses"]
                ["204"]["description"],
            "Property schema deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/properties/reorder"]["put"]["responses"]["204"]
                ["description"],
            "Property schemas reordered."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/properties"]["get"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/DocumentPropertyValue"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/properties"]["parameters"][1]
                ["name"],
            "doc_id"
        );
        assert!(
            spec["components"]["schemas"]["DocumentPropertyValue"]["required"]
                .as_array()
                .is_some_and(|required| required.contains(&serde_json::json!("options"))
                    && required.contains(&serde_json::json!("value")))
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/properties"]["put"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentPropertyValuesUpdateRequest"
        );
        assert!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/properties"]["put"]["responses"]
                ["200"]
                .is_null()
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/properties"]["put"]["responses"]
                ["204"]["description"],
            "Document properties replaced."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/backlinks"]["get"]["responses"]
                ["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/Backlink"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/backlinks"]["parameters"][1]
                ["schema"]["format"],
            "uuid"
        );
    }

    #[test]
    fn ai_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/ai/config"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AiConfig"
        );
        assert_eq!(
            spec["paths"]["/ai/chat"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AiChatRequest"
        );
        assert_eq!(
            spec["paths"]["/ai/chat"]["post"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AiChatResponse"
        );
        assert_eq!(
            spec["paths"]["/ai/complete"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AiCompleteRequest"
        );
        assert_eq!(
            spec["paths"]["/ai/complete"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/AiCompleteResponse"
        );
        assert_eq!(
            spec["paths"]["/ai/action"]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AiDocumentActionRequest"
        );
        assert_eq!(
            spec["components"]["schemas"]["AiDocumentActionRequest"]["properties"]["action"]
                ["enum"][3],
            "extract_actions"
        );
        assert_eq!(
            spec["paths"]["/ai/action"]["post"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/AiDocumentActionResponse"
        );
    }

    #[test]
    fn sync_replay_routes_include_typed_schemas_and_history() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/sync/replay/preflight"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SyncReplayManifest"
        );
        assert_eq!(
            spec["paths"]["/sync/replay"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SyncReplayExecutionResponse"
        );
        assert_eq!(
            spec["paths"]["/sync/replay/history"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/SyncReplayHistoryResponse"
        );
        assert!(
            spec["components"]["schemas"]["SyncReplayHistoryCheckpoint"]["required"]
                .as_array()
                .is_some_and(|required| required.contains(&serde_json::json!("space_id"))
                    && required.contains(&serde_json::json!("source_server_url"))
                    && required.contains(&serde_json::json!("downloaded_by_user_id")))
        );
    }

    #[test]
    fn template_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/DocumentTemplate"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentTemplateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates"]["post"]["responses"]["409"]
                ["description"],
            "Document template import ID already exists."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates"]["parameters"][0]["name"],
            "space_id"
        );
        assert!(
            spec["components"]["schemas"]["DocumentTemplate"]["required"]
                .as_array()
                .is_some_and(
                    |required| required.contains(&serde_json::json!("description"))
                        && required.contains(&serde_json::json!("properties"))
                )
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates/{template_id}"]["get"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/DocumentTemplate"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates/{template_id}"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates/{template_id}"]["parameters"][1]["name"],
            "template_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates/{template_id}"]["put"]["responses"]["204"]
                ["description"],
            "Document template updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/templates/{template_id}"]["delete"]["responses"]
                ["204"]["description"],
            "Document template deleted."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates"]["get"]["responses"]["200"]
                ["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/WorkflowTemplate"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates"]["post"]["requestBody"]
                ["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/WorkflowTemplateRequest"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates"]["post"]["responses"]["409"]
                ["description"],
            "Workflow template import ID already exists."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates"]["parameters"][0]["schema"]
                ["format"],
            "uuid"
        );
        assert!(
            spec["components"]["schemas"]["WorkflowTemplate"]["required"]
                .as_array()
                .is_some_and(|required| required.contains(&serde_json::json!("description")))
        );
        assert!(
            spec["components"]["schemas"]["WorkflowTemplateApprover"]["required"]
                .as_array()
                .is_some_and(|required| required.contains(&serde_json::json!("stage_id")))
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates/{template_id}"]["get"]
                ["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/WorkflowTemplate"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates/{template_id}"]["parameters"][0]
                ["name"],
            "space_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates/{template_id}"]["parameters"][1]
                ["name"],
            "template_id"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates/{template_id}"]["put"]
                ["responses"]["204"]["description"],
            "Workflow template updated."
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/workflow-templates/{template_id}"]["delete"]
                ["responses"]["204"]["description"],
            "Workflow template deleted."
        );
    }

    #[test]
    fn search_dataview_and_graph_routes_include_json_schemas() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/search"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["items"]["$ref"],
            "#/components/schemas/SearchResult"
        );
        assert!(spec["paths"]["/search"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "title")));
        assert!(spec["paths"]["/search"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "has_workflow")));
        assert!(spec["paths"]["/search"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "is_locked")));
        assert!(spec["paths"]["/search"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "has_tag")));
        assert!(spec["paths"]["/search"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "created_on")));
        assert!(spec["paths"]["/search"]["get"]["parameters"]
            .as_array()
            .is_some_and(|parameters| parameters
                .iter()
                .any(|parameter| parameter["name"] == "updated_on")));
        assert_eq!(
            spec["paths"]["/dataview"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["items"]["$ref"],
            "#/components/schemas/SearchResult"
        );
        assert_eq!(
            spec["paths"]["/dataview"]["get"]["parameters"][0]["name"],
            "q"
        );
        assert_eq!(
            spec["paths"]["/dataview"]["get"]["parameters"][0]["schema"]["maxLength"],
            500
        );
        assert_eq!(
            spec["paths"]["/dataview"]["get"]["parameters"][1]["schema"]["default"],
            50
        );
        assert_eq!(
            spec["paths"]["/dataview"]["get"]["parameters"][1]["schema"]["maximum"],
            100
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/graph"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/GraphResponse"
        );
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/graph"]["parameters"][0]["name"],
            "space_id"
        );
        assert_eq!(
            spec["components"]["schemas"]["GraphResponse"]["properties"]["nodes"]["items"]["$ref"],
            "#/components/schemas/GraphNode"
        );
        assert!(spec["components"]["schemas"]["GraphNode"]["required"]
            .as_array()
            .is_some_and(|required| required.contains(&serde_json::json!("folder_id"))));
    }

    #[test]
    fn mcp_manifest_route_includes_json_schema() {
        let spec = openapi_spec();
        assert_eq!(
            spec["paths"]["/mcp/manifest"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/McpServerManifest"
        );
        assert_eq!(
            spec["components"]["schemas"]["McpServerManifest"]["properties"]["transport_status"]
                ["enum"][1],
            "read_only_http_tools"
        );
        assert_eq!(
            spec["components"]["schemas"]["McpServerManifest"]["properties"]["summary"]["$ref"],
            "#/components/schemas/McpCatalogSummary"
        );
        assert_eq!(
            spec["components"]["schemas"]["McpCatalogSummary"]["required"][0],
            "tool_count"
        );
        assert_eq!(
            spec["components"]["schemas"]["McpCatalogSummary"]["required"][4],
            "by_runtime_mode"
        );
        assert_eq!(
            spec["components"]["schemas"]["McpToolManifestEntry"]["properties"]["runtime_modes"]
                ["items"]["enum"][0],
            "server"
        );
        assert!(
            spec["components"]["schemas"]["McpToolManifestEntry"]["required"]
                .as_array()
                .expect("required fields")
                .iter()
                .any(|field| field == "runtime_modes")
        );
        assert_eq!(
            spec["paths"]["/mcp/tools/{tool_name}"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/McpToolExecutionRequest"
        );
        assert_eq!(
            spec["paths"]["/mcp/tools/{tool_name}"]["post"]["parameters"][0]["name"],
            "tool_name"
        );
        assert_eq!(
            spec["paths"]["/mcp/tools/{tool_name}"]["post"]["parameters"][0]["in"],
            "path"
        );
        assert_eq!(
            spec["paths"]["/mcp/tools/{tool_name}"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/McpToolExecutionResponse"
        );
    }
}
