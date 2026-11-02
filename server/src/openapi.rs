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
                        "document_id": { "type": "string", "format": "uuid" },
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
                "SpaceRetentionResponse": {
                    "type": "object",
                    "properties": {
                        "trash_retention_days": { "type": "integer", "minimum": 30, "maximum": 120 }
                    },
                    "required": ["trash_retention_days"]
                },
                "SpaceRetentionUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "trash_retention_days": { "type": "integer", "minimum": 30, "maximum": 120 }
                    },
                    "required": ["trash_retention_days"],
                    "additionalProperties": false
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
                        "role": {
                            "type": "string",
                            "description": "Built-in role ID or a space-specific custom role ID."
                        },
                        "joined_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["user_id", "email", "display_name", "avatar_url", "role", "joined_at"]
                },
                "SpaceMemberRequest": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" },
                        "role": {
                            "type": "string",
                            "nullable": true,
                            "description": "Built-in role ID or a space-specific custom role ID."
                        }
                    },
                    "required": ["user_id"]
                },
                "SpaceRoleDefinition": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "description": { "type": "string" },
                        "permissions": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["view", "comment", "edit_content", "manage_members", "manage_space"] }
                        },
                        "built_in": { "type": "boolean" }
                    },
                    "required": ["id", "name", "description", "permissions", "built_in"]
                },
                "SpaceRoleRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 80 },
                        "description": { "type": "string", "maxLength": 240 },
                        "permissions": {
                            "type": "array",
                            "minItems": 1,
                            "items": { "type": "string", "enum": ["view", "comment", "edit_content", "manage_members", "manage_space"] }
                        }
                    },
                    "required": ["name", "permissions"]
                },
                "Group": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "description": { "type": "string", "nullable": true },
                        "member_count": { "type": "integer", "minimum": 0 },
                        "space_grant_count": { "type": "integer", "minimum": 0 },
                        "scim_managed": { "type": "boolean" },
                        "managed_space_id": { "type": "string", "format": "uuid", "nullable": true },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "name", "description", "member_count", "space_grant_count", "scim_managed", "managed_space_id", "created_at", "updated_at"]
                },
                "GroupCreateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "description": { "type": "string", "nullable": true }
                    },
                    "required": ["name"]
                },
                "GroupUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "description": { "type": "string", "nullable": true }
                    },
                    "required": ["name"]
                },
                "GroupSpaceGrant": {
                    "type": "object",
                    "properties": {
                        "space_id": { "type": "string", "format": "uuid" },
                        "name": { "type": "string" },
                        "slug": { "type": "string" },
                        "role": {
                            "type": "string",
                            "description": "Built-in role ID or a space-specific custom role ID."
                        }
                    },
                    "required": ["space_id", "name", "slug", "role"]
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
                        "role": {
                            "type": "string",
                            "description": "Built-in role ID or a space-specific custom role ID."
                        },
                        "member_count": { "type": "integer", "minimum": 0 },
                        "granted_at": { "type": "string", "format": "date-time" },
                        "managed": { "type": "boolean" }
                    },
                    "required": ["group_id", "name", "description", "role", "member_count", "granted_at", "managed"]
                },
                "SpaceGroupGrantRequest": {
                    "type": "object",
                    "properties": {
                        "group_id": { "type": "string", "format": "uuid" },
                        "role": {
                            "type": "string",
                            "nullable": true,
                            "description": "Built-in role ID or a space-specific custom role ID."
                        }
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
                        "role": { "type": "string", "enum": ["admin", "member", "viewer"] },
                        "account_kind": { "type": "string", "enum": ["member", "guest"], "description": "Current account classification; guest authority does not inherit the stored role." },
                        "is_active": { "type": "boolean" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "email", "display_name", "avatar_url", "role", "account_kind", "is_active", "created_at", "updated_at"]
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
                        "avatar_url": {
                            "type": "string",
                            "maxLength": 1048576,
                            "nullable": true,
                            "description": "External avatar URL (up to 500 characters) or a validated JPEG, PNG, or WebP image data URL from the profile cropper (under 1 MB)."
                        },
                        "role": { "type": "string", "enum": ["admin", "member", "viewer"] },
                        "account_kind": { "type": "string", "enum": ["member", "guest"], "description": "Current account classification; guest authority does not inherit the stored role." },
                        "totp_enabled": { "type": "boolean" }
                    },
                    "required": ["id", "email", "username", "display_name", "avatar_url", "role", "account_kind", "totp_enabled"]
                },
                "UpdateProfileRequest": {
                    "type": "object",
                    "properties": {
                        "display_name": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "avatar_url": {
                            "type": "string",
                            "maxLength": 1048576,
                            "nullable": true,
                            "description": "External avatar URL (up to 500 characters) or a validated JPEG, PNG, or WebP image data URL from the profile cropper (under 1 MB)."
                        },
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
                "NativeHandoffCompleteResponse": { "type": "object", "properties": { "handoff_id": { "type": "string", "format": "uuid" }, "status": { "type": "string", "enum": ["approved"] } }, "required": ["handoff_id", "status"] },
                "NativeHandoffExchangeRequest": { "type": "object", "properties": { "handoff_id": { "type": "string", "format": "uuid" }, "code_verifier": { "type": "string", "minLength": 43, "maxLength": 128 } }, "required": ["handoff_id", "code_verifier"], "additionalProperties": false },
                "NativeHandoffPendingResponse": { "type": "object", "properties": { "handoff_id": { "type": "string", "format": "uuid" }, "status": { "type": "string", "enum": ["pending"] } }, "required": ["handoff_id", "status"] },
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
                        "created_at": { "type": "string", "format": "date-time" },
                        "actor_kind": { "type": "string", "nullable": true },
                        "actor_id": { "type": "string", "nullable": true }
                    },
                    "required": ["id", "user_id", "user_email", "ip_address", "action", "target_type", "target_id", "created_at"]
                },
                "StorageIntegrityIssue": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["unsafe_path", "misplaced_file", "unsafe_symlink", "invalid_file_type", "missing_file", "unreadable_file", "orphan_file"] },
                        "record_type": { "type": "string", "enum": ["document", "draft", "attachment"] },
                        "record_id": { "type": "string", "nullable": true },
                        "path": { "type": "string" },
                        "detail": { "type": "string" }
                    },
                    "required": ["kind", "record_type", "record_id", "path", "detail"]
                },
                "StorageHealthReport": {
                    "type": "object",
                    "properties": {
                        "observed_at": {
                            "type": "string",
                            "format": "date-time"
                        },
                        "database": {
                            "type": "object",
                            "properties": {
                                "engine": {
                                    "type": "string",
                                    "enum": [
                                        "sqlite",
                                        "postgresql",
                                        "unknown"
                                    ]
                                },
                                "connection_status": {
                                    "type": "string",
                                    "enum": [
                                        "available",
                                        "unavailable"
                                    ]
                                },
                                "migration_history": {
                                    "type": "object",
                                    "properties": {
                                        "status": {
                                            "type": "string",
                                            "enum": [
                                                "matching",
                                                "mismatch",
                                                "unavailable"
                                            ]
                                        },
                                        "expected_count": {
                                            "type": "integer",
                                            "minimum": 0
                                        },
                                        "applied_count": {
                                            "type": "integer",
                                            "minimum": 0,
                                            "nullable": true
                                        },
                                        "scope": {
                                            "type": "string",
                                            "enum": [
                                                "applied_history_only"
                                            ]
                                        }
                                    },
                                    "required": [
                                        "status",
                                        "expected_count",
                                        "applied_count",
                                        "scope"
                                    ]
                                }
                            },
                            "required": [
                                "engine",
                                "connection_status",
                                "migration_history"
                            ]
                        },
                        "files": {
                            "type": "object",
                            "properties": {
                                "backend": {
                                    "type": "string",
                                    "enum": [
                                        "local"
                                    ]
                                },
                                "location": {
                                    "type": "string"
                                },
                                "read_status": {
                                    "type": "string",
                                    "enum": [
                                        "available",
                                        "unavailable"
                                    ]
                                },
                                "integrity_status": {
                                    "type": "string",
                                    "enum": [
                                        "not_checked"
                                    ]
                                }
                            },
                            "required": [
                                "backend",
                                "location",
                                "read_status",
                                "integrity_status"
                            ]
                        },
                        "recorded_attachment_usage": {
                            "type": "object",
                            "properties": {
                                "status": {
                                    "type": "string",
                                    "enum": [
                                        "available",
                                        "unavailable"
                                    ]
                                },
                                "attachments": {
                                    "type": "string",
                                    "nullable": true,
                                    "pattern": "^[0-9]+$"
                                },
                                "bytes": {
                                    "type": "string",
                                    "nullable": true,
                                    "pattern": "^[0-9]+$"
                                },
                                "scope": {
                                    "type": "string",
                                    "enum": [
                                        "attachment_database_records_only"
                                    ]
                                }
                            },
                            "required": [
                                "status",
                                "attachments",
                                "bytes",
                                "scope"
                            ]
                        },
                        "attachment_capacity": {
                            "type": "object",
                            "description": "Validated conservative attachment charge and reservation maxima; not raw filesystem usage or free space. Unavailable evidence never releases retained charge.",
                            "properties": {
                                "quota_status": {"type":"string","enum":["configured","unlimited","unavailable"]},
                                "quota_bytes": {"type":"string","nullable":true,"pattern":"^(0|[1-9][0-9]*)$"},
                                "accounting_status": {"type":"string","enum":["available","unavailable"]},
                                "logical_retained_bytes": {"type":"string","nullable":true,"pattern":"^(0|[1-9][0-9]*)$"},
                                "charged_retained_bytes": {"type":"string","nullable":true,"pattern":"^(0|[1-9][0-9]*)$"},
                                "reserved_maximum_bytes": {"type":"string","nullable":true,"pattern":"^(0|[1-9][0-9]*)$"},
                                "charged_total_bytes": {"type":"string","nullable":true,"pattern":"^(0|[1-9][0-9]*)$"},
                                "scope": {"type":"string","enum":["validated_attachment_charge_and_reservations"]}
                            },
                            "required": ["quota_status","quota_bytes","accounting_status","logical_retained_bytes","charged_retained_bytes","reserved_maximum_bytes","charged_total_bytes","scope"]
                        },
                        "limits": {
                            "type": "object",
                            "properties": {
                                "max_upload_bytes": {
                                    "type": "string",
                                    "nullable": true,
                                    "pattern": "^[0-9]+$"
                                },
                                "quota_status": {
                                    "type": "string",
                                    "enum": [
                                        "configured", "unlimited", "unavailable", "not_available"
                                    ]
                                }
                            },
                            "required": [
                                "max_upload_bytes",
                                "quota_status"
                            ]
                        },
                        "backup": {
                            "type": "object",
                            "properties": {
                                "status": {
                                    "type": "string",
                                    "enum": [
                                        "unknown",
                                        "recorded_completed",
                                        "unavailable"
                                    ]
                                },
                                "completed_at": {
                                    "type": "string",
                                    "format": "date-time",
                                    "nullable": true
                                }
                            },
                            "required": [
                                "status",
                                "completed_at"
                            ]
                        },
                        "application_restore_history": {
                            "type": "object",
                            "description": "Historical completed isolated application test at its recorded time on the original supported source; ordinary edits do not make it fresh and this never promotes verified_restore.",
                            "properties": {
                                "status": { "type": "string", "enum": ["unavailable", "completed_isolated_test"] },
                                "completed_at": { "type": "string", "format": "date-time", "nullable": true },
                                "scope": { "type": "string", "nullable": true, "enum": ["restored-pg-ledger1-ordinary-application-v1", "restored-sqlite-ledger1-ordinary-application-v1", null] },
                                "operation": { "type": "string", "format": "uuid", "nullable": true },
                                "pages": { "type": "integer", "minimum": 0, "nullable": true },
                                "drafts": { "type": "integer", "minimum": 0, "nullable": true },
                                "registered_assets": { "type": "integer", "minimum": 0, "nullable": true },
                                "legacy_assets": { "type": "integer", "minimum": 0, "nullable": true },
                                "exact_binary_bytes": { "type": "string", "pattern": "^(0|[1-9][0-9]*)$", "nullable": true },
                                "sampled_humans": { "type": "integer", "minimum": 0, "nullable": true },
                                "semantics": { "type": "string", "enum": ["historical_completed_isolated_application_test_at_recorded_time"] },
                                "full_supported_restore_verified": { "type": "boolean", "enum": [false] }
                            },
                            "required": ["status", "completed_at", "scope", "operation", "pages", "drafts", "registered_assets", "legacy_assets", "exact_binary_bytes", "sampled_humans", "semantics", "full_supported_restore_verified"]
                        },
                        "verified_restore": {
                            "type": "object",
                            "properties": {
                                "status": {
                                    "type": "string",
                                    "enum": [
                                        "unknown"
                                    ]
                                },
                                "completed_at": {
                                    "type": "string",
                                    "format": "date-time",
                                    "nullable": true
                                }
                            },
                            "required": [
                                "status",
                                "completed_at"
                            ]
                        }
                    },
                    "required": [
                        "observed_at",
                        "database",
                        "files",
                        "recorded_attachment_usage",
                        "limits",
                        "backup",
                        "verified_restore",
                        "application_restore_history"
                    ]
                },
                "StorageIntegrityReport": {
                    "type": "object",
                    "properties": {
                        "healthy": { "type": "boolean" },
                        "documents": { "type": "integer", "minimum": 0 },
                        "drafts": { "type": "integer", "minimum": 0 },
                        "attachments": { "type": "integer", "minimum": 0 },
                        "issues": { "type": "array", "items": { "$ref": "#/components/schemas/StorageIntegrityIssue" } }
                    },
                    "required": ["healthy", "documents", "drafts", "attachments", "issues"]
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
                        "device_name": { "type": "string" },
                        "ip_address": { "type": "string", "nullable": true },
                        "user_agent": { "type": "string", "nullable": true },
                        "session_kind": { "type": "string", "enum": ["browser", "app"] },
                        "client_platform": { "type": "string", "enum": ["web", "mobile_app", "desktop_app"] },
                        "is_current": { "type": "boolean" },
                        "last_seen_at": { "type": "string", "format": "date-time" },
                        "expires_at": { "type": "string", "format": "date-time" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "device_name", "ip_address", "user_agent", "session_kind", "client_platform", "is_current", "last_seen_at", "expires_at", "created_at"]
                },
                "SessionUpdateRequest": {
                    "type": "object",
                    "properties": { "device_name": { "type": "string", "minLength": 1, "maxLength": 80 } },
                    "required": ["device_name"]
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
                        "published_at": { "type": "string", "format": "date-time", "nullable": true },
                        "draft_id": { "type": "string", "format": "uuid", "nullable": true, "description": "Initial private draft created with a new document. Omitted from ordinary document reads." },
                        "published_at": { "type": "string", "format": "date-time", "nullable": true },
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
                "DraftDiscussionComment": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "draft_id": { "type": "string", "format": "uuid" },
                        "source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "parent_id": { "type": ["string", "null"], "format": "uuid" },
                        "user_id": { "type": "string", "format": "uuid" },
                        "user_name": { "type": "string" },
                        "content": { "type": "string", "minLength": 1, "maxLength": 10000 },
                        "selection_start": { "type": ["integer", "null"], "minimum": 0 },
                        "selection_end": { "type": ["integer", "null"], "minimum": 1 },
                        "selection_text": { "type": ["string", "null"] },
                        "anchor_outdated": { "type": "boolean" },
                        "resolved": { "type": "boolean" },
                        "resolved_by": { "type": ["string", "null"], "format": "uuid" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" },
                        "replies": { "type": "array", "items": { "$ref": "#/components/schemas/DraftDiscussionComment" } }
                    },
                    "required": ["id", "draft_id", "source_digest", "parent_id", "user_id", "user_name", "content", "selection_start", "selection_end", "selection_text", "anchor_outdated", "resolved", "resolved_by", "created_at", "updated_at", "replies"]
                },
                "DraftDiscussion": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "discussion_id": { "type": ["string", "null"], "format": "uuid" },
                        "draft_id": { "type": "string", "format": "uuid" },
                        "source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "state": { "type": "string", "enum": ["active", "closed"] },
                        "mention_candidates": { "type": "array", "maxItems": 100, "items": { "$ref": "#/components/schemas/DraftDiscussionMentionCandidate" } },
                        "comments": { "type": "array", "items": { "$ref": "#/components/schemas/DraftDiscussionComment" } }
                    },
                    "required": ["discussion_id", "draft_id", "source_digest", "state", "mention_candidates", "comments"]
                },
                "DraftDiscussionMentionCandidate": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" },
                        "display_name": { "type": "string" },
                        "email": { "type": "string", "format": "email" }
                    },
                    "required": ["user_id", "display_name", "email"]
                },
                "ClosedDraftDiscussionSummary": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "draft_id": { "type": "string", "format": "uuid" },
                        "publication_operation_id": { "type": "string", "format": "uuid" },
                        "closed_at": { "type": "string", "format": "date-time" },
                        "comment_count": { "type": "integer", "minimum": 0 },
                        "unresolved_count": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["id", "draft_id", "publication_operation_id", "closed_at", "comment_count", "unresolved_count"]
                },
                "ClosedDraftDiscussionList": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "schema_version": { "type": "integer", "enum": [1] },
                        "space_id": { "type": "string", "format": "uuid" },
                        "document_id": { "type": "string", "format": "uuid" },
                        "discussions": { "type": "array", "maxItems": 100, "items": { "$ref": "#/components/schemas/ClosedDraftDiscussionSummary" } }
                    },
                    "required": ["schema_version", "space_id", "document_id", "discussions"]
                },
                "DraftDiscussionCreateRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "content": { "type": "string", "minLength": 1, "maxLength": 10000 },
                        "parent_id": { "type": ["string", "null"], "format": "uuid" },
                        "selection_start": { "type": ["integer", "null"], "minimum": 0 },
                        "selection_end": { "type": ["integer", "null"], "minimum": 1 },
                        "selection_text": { "type": ["string", "null"] }
                    },
                    "required": ["source_digest", "content"]
                },
                "DraftDiscussionUpdateRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "content": { "type": "string", "minLength": 1, "maxLength": 10000 }
                    },
                    "required": ["source_digest", "content"]
                },
                "DraftDiscussionCreated": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "discussion_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["id", "discussion_id"]
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
                "PublicationSnapshotSummary": publication_snapshot_schema(false),
                "PublicationSnapshotDetail": publication_snapshot_schema(true),
                "PublicationSnapshotList": {"type":"object","additionalProperties":false,"required":["schema_version","space_id","document_id","snapshots"],"properties":{"schema_version":{"type":"integer","const":1},"space_id":{"type":"string","description":"Current route space, independently authorized; not captured-space provenance."},"document_id":{"type":"string"},"snapshots":{"type":"array","items":{"$ref":"#/components/schemas/PublicationSnapshotSummary"}}}},
                "PublicationSnapshotGet": {"type":"object","additionalProperties":false,"required":["schema_version","space_id","document_id","snapshot"],"properties":{"schema_version":{"type":"integer","const":1},"space_id":{"type":"string"},"document_id":{"type":"string"},"snapshot":{"$ref":"#/components/schemas/PublicationSnapshotDetail"}}},
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
                        "document_id": {"type":"string"},
                        "logical_attachment_id": {"type":"string"},
                        "audience": {"type":"object","additionalProperties":false,"required":["schema_version","draft_id","draft_created_at"],"properties":{"schema_version":{"const":2},"draft_id":{"type":"string","format":"uuid"},"draft_created_at":{"type":"string","minLength":1}}},
                        "id": { "type": "string", "format": "uuid" },
                        "filename": { "type": "string" },
                        "original_name": { "type": "string" },
                        "mime_type": { "type": "string" },
                        "size_bytes": { "type": "integer", "minimum": 0 },
                        "url": { "type": "string" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["document_id", "id", "filename", "original_name", "mime_type", "size_bytes", "url", "created_at"]
                },
                "OriginalTrashedDeleteTarget": {
                    "type":"object","additionalProperties":false,
                    "properties":{"created_at":{"type":"string","minLength":1,"maxLength":4096,"description":"Exact raw original page incarnation string; do not normalize."},"deleted_at":{"type":"string","minLength":1,"maxLength":4096,"description":"Exact raw original Trash-cycle string from authorized listing; do not normalize."}},
                    "required":["created_at","deleted_at"]
                },
                "OriginalPermanentDeleteRequest": {"type":"object","additionalProperties":false,"properties":{"original_target":{"$ref":"#/components/schemas/OriginalTrashedDeleteTarget"}},"required":["original_target"]},
                "OriginalCleanupTarget": {"type":"object","additionalProperties":false,"properties":{"created_at":{"type":"string","minLength":1,"maxLength":4096},"deleted_at":{"type":"string","nullable":true,"minLength":1,"maxLength":4096,"description":"Required exact original value, including null only where the existing original plan records null; never infer from absence."}},"required":["created_at","deleted_at"]},
                "OriginalAttachmentCleanupRequest": {"type":"object","additionalProperties":false,"properties":{"original_target":{"$ref":"#/components/schemas/OriginalCleanupTarget"}},"required":["original_target"]},
                "OriginalAttachmentCleanupOutcome": {"type":"object","properties":{"operation_id":{"type":"string","format":"uuid"},"metadata_deleted":{"type":"boolean"},"outcome":{"type":"string","enum":["pending","retained_review","physically_completed"]},"reason":{"type":"string","nullable":true},"assets":{"type":"array","items":{"type":"object","properties":{"asset_id":{"type":"string"},"outcome":{"type":"string","enum":["pending","retained_review","physically_completed"]},"reason":{"type":"string","nullable":true}},"required":["asset_id","outcome","reason"]}}},"required":["operation_id","metadata_deleted","outcome","reason","assets"]},
                "Document": {
                    "type": "object",
                    "properties": {
                        "page_access_source": {"type":"string","enum":["direct_page_grant"],"description":"Optional GET-only server-proved direct published page viewer provenance. Omitted for ordinary inherited access. Placement, private draft and participant/editor catalog fields are suppressed in this branch; it grants no space authority."},
                        "page_edit_allowed": {"type":"boolean","description":"Optional scoped-GET projection of current ordinary Page Edit with original human write identity, restrictions and lock-owner policy. A lock grants no authority. Direct published-page viewers receive false; list/create responses omit this field. Cached values are not current authority. Mutations recheck current access."},
                        "maintenance_edit_allowed": {"type":"boolean","description":"Optional GET-only original credential PageEdit hint; mutation rechecks current permission."},
                        "identifier_edit_allowed": { "type": "boolean", "description": "Optional current original-credential PageEdit hint, present only on GET document. Mutation authorization is rechecked." },
                        "maintenance_user_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."},
                        "maintenance_group_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."},
                        "human_identifier": { "type": "string", "nullable": true, "description": "Optional readable reference; trimmed ASCII letters, digits, dots, underscores or hyphens, starting alphanumeric, 1–64 normalized characters. Case-preserving and reserved per space across lifecycle states." },
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
                        "created_by": {"oneOf":[{"type":"string","format":"uuid"},{"type":"string","enum":[""]}],"description":"Original actor ID for ordinary responses; empty in the direct-page-only metadata projection."},
                        "last_updated_by": {"oneOf":[{"type":"string","format":"uuid"},{"type":"string","enum":[""]}],"description":"Original actor ID for ordinary responses; empty in the direct-page-only metadata projection."},
                        "created_at": { "type": "string", "format": "date-time" },
                        "deleted_at": { "type": "string", "format": "date-time", "description": "Optional exact raw original Trash-cycle timestamp on authorized trashed rows. Preserve byte-for-byte with original created_at for deletion witness; this is not deletion authority. Active rows omit it." },
                        "updated_at": { "type": "string", "format": "date-time" },
                        "published_at": { "type": "string", "format": "date-time", "nullable": true },
                        "draft_source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$", "description": "Original full raw initial private draft source returned only with document creation. Omitted from ordinary reads; carry unchanged into the first draft save." },
                        "draft_id": { "type": "string", "format": "uuid", "nullable": true, "description": "Newest private draft accessible to the signed-in user when the document is unpublished." },
                        "workflow_template_id": { "type": "string", "format": "uuid", "nullable": true },
                        "workflow_template_name": { "type": "string", "nullable": true },
                        "workflow_stage_id": { "type": "string", "nullable": true },
                        "workflow_stage_name": { "type": "string", "nullable": true },
                        "workflow_status": { "type": "string", "enum": ["active", "completed", "cancelled"], "nullable": true }
                    },
                    "required": ["id", "space_id", "folder_id", "parent_id", "title", "icon", "cover_image", "locked", "locked_by", "public", "public_slug", "created_by", "last_updated_by", "created_at", "updated_at", "published_at", "workflow_template_id", "workflow_template_name", "workflow_stage_id", "workflow_stage_name", "workflow_status"]
                },
                "MovePreservedIndex": {
                    "type": "object", "additionalProperties": false,
                    "properties": {
                        "schema_version": {"type": "integer", "enum": [1]},
                        "space_id": {"type": "string", "minLength": 1},
                        "document_id": {"type": "string", "minLength": 1},
                        "checked_at": {"type": "string", "format": "date-time"},
                        "operations": {"type": "array", "maxItems": 20, "items": {
                            "type": "object", "additionalProperties": false,
                            "properties": {
                                "operation_id": {"type": "string", "minLength": 1},
                                "created_at": {"type": "string", "format": "date-time"},
                                "kind": {"type": "string", "enum": ["immutable_metadata_evidence"]}
                            },
                            "required": ["operation_id", "created_at", "kind"]
                        }},
                        "next_cursor": {"type": "string", "nullable": true, "description": "Last already disclosed bound operation ID when more bound rows exist; null at the end."}
                    },
                    "required": ["schema_version", "space_id", "document_id", "checked_at", "operations", "next_cursor"]
                },
                "MovePreservedEvidence": {
                    "type": "object", "additionalProperties": false,
                    "properties": {
                        "operation_id": {"type": "string", "minLength": 1},
                        "space_id": {"type": "string", "minLength": 1},
                        "document_id": {"type": "string", "minLength": 1},
                        "kind": {"type": "string", "enum": ["immutable_metadata_evidence"]},
                        "editable": {"type": "boolean", "enum": [false]},
                        "restoration_available": {"type": "boolean", "enum": [false]},
                        "entries": {"type": "array", "items": {
                            "type": "object", "additionalProperties": false,
                            "properties": {
                                "family": {"type": "string", "enum": ["tag", "property", "workflow", "incident_relation", "identifier", "maintenance"]},
                                "source_id": {"type": "string"},
                                "disposition": {"type": "string", "enum": ["map", "create", "preserve", "remove", "rename", "clear"]},
                                "original": {"description": "Inactive sealed historical metadata; private bodies and grants are excluded."},
                                "expected_active": {"description": "Sealed expected active metadata from the operation, not current page state."}
                            },
                            "required": ["family", "source_id", "disposition", "original", "expected_active"]
                        }}
                    },
                    "required": ["operation_id", "space_id", "document_id", "kind", "editable", "restoration_available", "entries"]
                },
                "PageAccessInspection": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "page_permissions": {"type":"array","uniqueItems":true,"items":{"type":"string","enum":["view","comment","create_content","edit_content","move_content","duplicate_content","manage_folders","manage_templates","archive_content","delete_content","manage_members","manage_space"]},"description":"Current effective page permissions; direct viewer grants do not change space_permission_sources."},
                        "page_permission_sources": {"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"kind":{"type":"string","enum":["direct_page_viewer"]},"grant_id":{"type":"string"},"revision":{"type":"string"},"permissions":{"type":"array","uniqueItems":true,"items":{"type":"string","enum":["view"]}}},"required":["kind","grant_id","revision","permissions"]}},

                        "schema_version": {
                            "type": "integer",
                            "enum": [
                                1
                            ]
                        },
                        "space_id": {
                            "type": "string"
                        },
                        "page_id": {
                            "type": "string"
                        },
                        "checked_at": {
                            "type": "string",
                            "format": "date-time"
                        },
                        "capabilities": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "view": {
                                    "type": "boolean"
                                },
                                "comment": {
                                    "type": "boolean"
                                },
                                "edit": {
                                    "type": "boolean"
                                },
                                "move": {
                                    "type": "boolean"
                                },
                                "manage_lock": {
                                    "type": "boolean"
                                },
                                "enable_public_link": {
                                    "type": "boolean"
                                },
                                "disable_public_link": {
                                    "type": "boolean"
                                }
                            },
                            "required": [
                                "view",
                                "comment",
                                "edit",
                                "move",
                                "manage_lock",
                                "enable_public_link",
                                "disable_public_link"
                            ]
                        },
                        "space_permission_sources": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {
                                    "kind": {
                                        "type": "string",
                                        "enum": [
                                            "direct_space_role",
                                            "space_group_role",
                                            "global_group_role",
                                            "internal_space_fallback",
                                            "public_space_fallback",
                                            "instance_admin"
                                        ]
                                    },
                                    "permissions": {
                                        "type": "array",
                                        "uniqueItems": true,
                                        "items": {
                                            "type": "string",
                                            "enum": [
                                                "view",
                                                "comment",
                                                "create_content",
                                                "edit_content",
                                                "move_content",
                                                "duplicate_content",
                                                "manage_folders",
                                                "manage_templates",
                                                "archive_content",
                                                "delete_content",
                                                "manage_members",
                                                "manage_space"
                                            ]
                                        }
                                    }
                                },
                                "required": [
                                    "kind",
                                    "permissions"
                                ]
                            }
                        },
                        "restrictions": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "local": {
                                    "type": "string",
                                    "enum": [
                                        "inherited",
                                        "restricted"
                                    ]
                                },
                                "ancestor_restricted": {
                                    "type": "boolean"
                                }
                            },
                            "required": [
                                "local",
                                "ancestor_restricted"
                            ]
                        },
                        "public_link": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "configured": {
                                    "type": "boolean"
                                },
                                "policy_allows_anonymous": {
                                    "type": "boolean"
                                },
                                "reason": {
                                    "type": [
                                        "string",
                                        "null"
                                    ],
                                    "enum": [
                                        "not_enabled",
                                        "deployment_disabled",
                                        "unpublished",
                                        "restricted",
                                        "missing_link",
                                        null
                                    ]
                                }
                            },
                            "required": [
                                "configured",
                                "policy_allows_anonymous",
                                "reason"
                            ]
                        },
                        "selected_private_draft": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "scope": {
                                    "type": "string",
                                    "enum": [
                                        "none",
                                        "owned",
                                        "shared_viewer",
                                        "shared_editor"
                                    ]
                                },
                                "view": {
                                    "type": "boolean"
                                },
                                "edit": {
                                    "type": "boolean"
                                }
                            },
                            "required": [
                                "scope",
                                "view",
                                "edit"
                            ]
                        },
                        "sharing": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "people_grants_available": {
                                    "type": "boolean",
                                    "description": "Feature availability for the currently eligible active published page; not the caller's management capability. Current named management and original Write remain required to change grants."
                                },
                                "guests_available": {
                                    "type": "boolean",
                                    "description": "Guest viewer-sharing availability for the currently eligible published page; not an invitation or management capability."
                                }
                            },
                            "required": [
                                "people_grants_available",
                                "guests_available"
                            ]
                        }
                    },
                    "required": [
                        "schema_version",
                        "space_id",
                        "page_id",
                        "checked_at",
                        "capabilities",
                        "space_permission_sources",
                        "page_permissions",
                        "page_permission_sources",
                        "restrictions",
                        "public_link",
                        "selected_private_draft",
                        "sharing"
                    ]
                },
                "PageViewerGrant": {"type":"object","additionalProperties":false,"properties":{"id":{"type":"string"},"document_id":{"type":"string"},"space_id":{"type":"string"},"user_id":{"type":"string"},"role":{"type":"string","enum":["viewer"]},"scope":{"type":"string","enum":["page"]},"revision":{"type":"string"},"created_by":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"}},"required":["id","document_id","space_id","user_id","role","scope","revision","created_by","created_at","updated_at"]},
                "PageViewerGrantProposal": {"type":"object","additionalProperties":false,"properties":{"user_id":{"type":"string"},"action":{"type":"string","enum":["grant","revoke"]}},"required":["user_id","action"]},
                "PageViewerGrantInspection": {"type":"object","additionalProperties":false,"properties":{"schema_version":{"type":"integer","enum":[1]},"space_id":{"type":"string"},"document_id":{"type":"string"},"grants":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"user_id":{"type":"string"},"revision":{"type":"string"},"role":{"type":"string","enum":["viewer"]},"scope":{"type":"string","enum":["page"]},"display_name":{"type":["string","null"]},"available":{"type":"boolean"}},"required":["user_id","revision","role","scope","display_name","available"]}},"scope_notice":{"type":"string"}},"required":["schema_version","space_id","document_id","grants","scope_notice"]},
                "PageViewerGrantReview": {"type":"object","additionalProperties":false,"properties":{"schema_version":{"type":"integer","enum":[1]},"space_id":{"type":"string"},"document_id":{"type":"string"},"proposal":{"$ref":"#/components/schemas/PageViewerGrantProposal"},"source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},"recipient":{"type":"object","additionalProperties":false,"properties":{"id":{"type":"string"},"display_name":{"type":["string","null"]},"email":{"type":["string","null"]}},"required":["id","display_name","email"]},"existing_grant":{"oneOf":[{"$ref":"#/components/schemas/PageViewerGrant"},{"type":"null"}]}},"required":["schema_version","space_id","document_id","proposal","source_digest","recipient","existing_grant"]},
                "PageViewerGrantConfirmation": {"type":"object","additionalProperties":false,"properties":{"proposal":{"$ref":"#/components/schemas/PageViewerGrantProposal"},"source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},"confirm":{"type":"boolean","enum":[true]}},"required":["proposal","source_digest","confirm"]},
                "PageViewerGrantOutcome": {"type":"object","additionalProperties":false,"properties":{"schema_version":{"type":"integer","enum":[1]},"space_id":{"type":"string"},"document_id":{"type":"string"},"user_id":{"type":"string"},"action":{"type":"string","enum":["grant","revoke"]},"grant":{"oneOf":[{"$ref":"#/components/schemas/PageViewerGrant"},{"type":"null"}]},"confirmed":{"type":"boolean","enum":[true]},"source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"required":["schema_version","space_id","document_id","user_id","action","grant","confirmed","source_digest"]},
                "SharedPageInspectionRequest": {"type":"object","additionalProperties":false,"properties":{"cursor":{"type":["string","null"],"maxLength":8192,"description":"Opaque actor/original-credential-bound keyset cursor, expiring after ten minutes. Paging evidence supplies no permission. Omit or use null for the first page."}},"required":[]},
                "SharedPageInspection": {"type":"object","additionalProperties":false,"properties":{"pages":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"id":{"type":"string"},"space_id":{"type":"string"},"title":{"type":"string"},"published_at":{"type":"string"},"updated_at":{"type":"string"},"human_identifier":{"type":["string","null"]},"icon":{"type":["string","null"]}},"required":["id","space_id","title","published_at","updated_at","human_identifier","icon"]},"maxItems":20},"next_cursor":{"type":["string","null"],"description":"Cursor based only on the last returned visible page ID; no skipped inaccessible IDs or hidden totals. Null at the end."}},"required":["pages","next_cursor"]},
                "DocumentMaintenanceChoice": {"oneOf": [{"type": "object", "additionalProperties": false, "properties": {"kind": {"type": "string", "enum": ["unassigned"]}}, "required": ["kind"]}, {"type": "object", "additionalProperties": false, "properties": {"kind": {"type": "string", "enum": ["user"]}, "user_id": {"type": "string", "minLength": 1}}, "required": ["kind", "user_id"]}, {"type": "object", "additionalProperties": false, "properties": {"kind": {"type": "string", "enum": ["group"]}, "group_id": {"type": "string", "minLength": 1}}, "required": ["kind", "group_id"]}]},
                "DocumentMaintenanceSource": {"type": "object", "additionalProperties": false, "properties": {"page_updated_at": {"type": "string", "format": "date-time"}, "published_at": {"type": "string", "format": "date-time", "nullable": true}, "maintenance_user_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."}, "maintenance_group_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."}}, "required": ["page_updated_at", "published_at", "maintenance_user_id", "maintenance_group_id"]},
                "DocumentMaintenanceRequest": {"type": "object", "additionalProperties": false, "properties": {"provider": {"type": "string", "minLength": 1, "maxLength": 256}, "operation_id": {"type": "string", "minLength": 1, "maxLength": 256}, "maintenance": {"$ref": "#/components/schemas/DocumentMaintenanceChoice"}, "source": {"$ref": "#/components/schemas/DocumentMaintenanceSource"}}, "required": ["provider", "operation_id", "maintenance", "source"]},
                "DocumentMaintenanceOutcome": {"type": "object", "additionalProperties": false, "properties": {"operation_id": {"type": "string"}, "maintenance_user_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."}, "maintenance_group_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."}, "page_updated_at": {"type": "string", "format": "date-time"}, "published_at": {"type": "string", "format": "date-time", "nullable": true}, "historical": {"type": "boolean", "description": "Exact earlier operation confirmed; never overwrite current metadata with this historical outcome."}}, "required": ["operation_id", "maintenance_user_id", "maintenance_group_id", "page_updated_at", "published_at", "historical"]},
                "DocumentMaintenanceTargets": {"type": "object", "additionalProperties": false, "properties": {"users": {"type": "array", "items": {"type": "object", "additionalProperties": false, "properties": {"id": {"type": "string"}, "name": {"type": "string"}, "active": {"type": "boolean", "enum": [true]}}, "required": ["id", "name", "active"]}}, "groups": {"type": "array", "items": {"type": "object", "additionalProperties": false, "properties": {"id": {"type": "string"}, "name": {"type": "string"}, "managed_space_id": {"type": "string", "nullable": true}}, "required": ["id", "name", "managed_space_id"]}}, "groups_available": {"type": "boolean"}}, "required": ["users", "groups", "groups_available"]},
                "DocumentIdentifierSource": {
                    "type": "object", "additionalProperties": false,
                    "properties": {
                        "page_updated_at": { "type": "string", "format": "date-time" },
                        "published_at": { "type": "string", "format": "date-time", "nullable": true },
                        "human_identifier": { "type": "string", "nullable": true }
                    }, "required": ["page_updated_at", "published_at", "human_identifier"]
                },
                "DocumentIdentifierRequest": {
                    "type": "object", "additionalProperties": false,
                    "properties": {
                        "provider": { "type": "string", "minLength": 1, "maxLength": 256 },
                        "operation_id": { "type": "string", "minLength": 1, "maxLength": 256 },
                        "human_identifier": { "type": "string", "nullable": true, "description": "Trimmed normalized identifier; null or blank clears it." },
                        "source": { "$ref": "#/components/schemas/DocumentIdentifierSource" }
                    }, "required": ["provider", "operation_id", "human_identifier", "source"]
                },
                "DocumentIdentifierOutcome": {
                    "type": "object",
                    "properties": {
                        "operation_id": { "type": "string" },
                        "human_identifier": { "type": "string", "nullable": true },
                        "page_updated_at": { "type": "string", "format": "date-time" },
                        "published_at": { "type": "string", "format": "date-time", "nullable": true },
                        "historical": { "type": "boolean", "description": "Earlier exact operation was confirmed. Do not apply these historical fields over current page state." }
                    }, "required": ["operation_id", "human_identifier", "page_updated_at", "published_at", "historical"]
                },
                "DocumentCreateRequest": {
                    "type": "object",
                    "properties": {
                        "maintenance_user_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."},
                        "maintenance_group_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."},
                        "human_identifier": { "type": "string", "nullable": true, "description": "Optional readable reference; trimmed ASCII letters, digits, dots, underscores or hyphens, starting alphanumeric, 1–64 normalized characters. Case-preserving and reserved per space across lifecycle states." },
                        "title": { "type": "string", "maxLength": 500 },
                        "folder_id": { "type": "string", "format": "uuid", "nullable": true },
                        "parent_id": { "type": "string", "format": "uuid", "nullable": true },
                        "icon": { "type": "string", "nullable": true },
                        "content": { "type": "string", "maxLength": 5242880, "nullable": true },
                        "template_id": { "type": "string", "format": "uuid", "nullable": true, "description": "Optional same-space template whose matching tag and property metadata is applied atomically." },
                        "property_values": {
                            "type": "array",
                            "maxItems": 100,
                            "description": "Optional same-space property assignments applied with page creation. These override matching template defaults; each value is limited to 100000 UTF-8 bytes.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "property_id": { "type": "string" },
                                    "value": { "type": "string", "maxLength": 100000 }
                                },
                                "required": ["property_id", "value"]
                            }
                        },
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
                        "content": { "type": "string" },
                        "published_source_digest": {"type":"string","pattern":"^[0-9a-f]{64}$","description":"Original full raw published row and exact served content digest. Absent for private draft bodies; never a permission proof."},
                        "published_metadata": {"type":"object","additionalProperties":false,"required":["title","icon","cover_image"],"properties":{"title":{"type":"string"},"icon":{"type":["string","null"]},"cover_image":{"type":["string","null"]}},"description":"Presentation from the same selected published row as published_source_digest, revalidated with its body and current audience. Omitted for private draft content; never latest draft/listing metadata or an access grant."}
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
                "DocumentDraft": {
                    "type": "object",
                    "properties": {
                        "birth_operation_id": { "type": "string", "format": "uuid", "description": "Only on a receipt-backed exact birth result; matches the original request operation_id." },
                        "source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$", "description": "SHA256 of the exact full raw selected draft row accompanying this response. Carry this original source unchanged when saving." },
                        "id": { "type": "string", "format": "uuid" },
                        "document_id": { "type": "string", "format": "uuid" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "owner_id": { "type": "string", "format": "uuid" },
                        "owner_name": { "type": "string" },
                        "document_title": { "type": "string" },
                        "title": { "type": "string" },
                        "icon": { "type": "string", "nullable": true },
                        "cover_image": { "type": "string", "nullable": true },
                        "content": { "type": "string" },
                        "base_updated_at": { "type": "string", "format": "date-time" },
                        "access_role": { "type": "string", "enum": ["owner", "editor", "viewer"] },
                        "can_publish": { "type": "boolean" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["source_digest", "id", "document_id", "space_id", "owner_id", "owner_name", "document_title", "title", "content", "base_updated_at", "access_role", "can_publish", "created_at", "updated_at"]
                },
                "DocumentDraftSummary": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "document_id": { "type": "string", "format": "uuid" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "owner_id": { "type": "string", "format": "uuid" },
                        "owner_name": { "type": "string" },
                        "document_title": { "type": "string" },
                        "title": { "type": "string" },
                        "access_role": { "type": "string", "enum": ["owner", "editor", "viewer"] },
                        "can_publish": { "type": "boolean" },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "document_id", "space_id", "owner_id", "owner_name", "document_title", "title", "access_role", "can_publish", "created_at", "updated_at"]
                },
                "DocumentDraftCreateRequest": {
                    "type": "object",
                    "description": "Resume the latest owned draft, or create an authorized source-bound draft. Forced birth requires operation_id for exact retry. New private forks require the already-selected editable source_draft_id and original source_digest. Missing source returns 409 source_required; changed source or receipt returns 409 source_conflict. Retain unknown operations without automatic rebasing.",
                    "properties": {
                        "force_new": { "type": "boolean", "default": false },
                        "operation_id": { "type": "string", "format": "uuid" },
                        "source_draft_id": { "type": "string" },
                        "source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
                    }
                },
                "DocumentDraftTransferRequest": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "One exact reviewed local draft transfer. The local snapshot digest covers title, nullable icon/cover_image and exact content. A private destination requires an exact editable server draft source; a published destination requires the exact published page source. Retain the UUID and unchanged body for uncertain-result retry.",
                    "properties": {
                        "schema_version": { "type": "integer", "const": 1 },
                        "operation_id": { "type": "string", "format": "uuid" },
                        "local_source": { "type": "object", "additionalProperties": false, "properties": {
                            "provider": {"type":"string"}, "root_identity":{"type":"string"}, "space_id":{"type":"string"},
                            "document_id":{"type":"string"}, "draft_id":{"type":"string"}, "draft_created_at":{"type":"string"},
                            "snapshot_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"}
                        }, "required":["provider","root_identity","space_id","document_id","draft_id","draft_created_at","snapshot_digest"] },
                        "server_source": { "oneOf": [
                            {"type":"object","additionalProperties":false,"properties":{"kind":{"const":"draft"},"draft_id":{"type":"string"},"draft_created_at":{"type":"string"},"source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"required":["kind","draft_id","draft_created_at","source_digest"]},
                            {"type":"object","additionalProperties":false,"properties":{"kind":{"const":"published_page"},"page_updated_at":{"type":"string"},"source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"required":["kind","page_updated_at","source_digest"]}
                        ] },
                        "reviewed_draft": {"type":"object","additionalProperties":false,"properties":{"title":{"type":"string","maxLength":500},"icon":{"type":"string","nullable":true},"cover_image":{"type":"string","nullable":true},"content":{"type":"string","maxLength":5242880}},"required":["title","icon","cover_image","content"]},
                        "selected_operation_fingerprints": {"type":"array","uniqueItems":true,"items":{"type":"string","pattern":"^[0-9a-f]{64}$"}}
                    },
                    "required": ["schema_version","operation_id","local_source","server_source","reviewed_draft","selected_operation_fingerprints"]
                },
                "DocumentDraftTransferReceipt": {
                    "type":"object",
                    "properties": {
                        "schema_version":{"type":"integer","const":1}, "operation_id":{"type":"string","format":"uuid"},
                        "request_fingerprint":{"type":"string","pattern":"^[0-9a-f]{64}$"}, "server_actor_id":{"type":"string"},
                        "server_space_id":{"type":"string"}, "server_document_id":{"type":"string"}, "target_draft_id":{"type":"string","format":"uuid"},
                        "target_draft_created_at":{"type":"string"}, "target_source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},
                        "local_source":{"type":"object"}, "selected_operation_fingerprints":{"type":"array","items":{"type":"string"}}, "created_at":{"type":"string"}
                    },
                    "required":["schema_version","operation_id","request_fingerprint","server_actor_id","server_space_id","server_document_id","target_draft_id","target_draft_created_at","target_source_digest","local_source","selected_operation_fingerprints","created_at"]
                },
                "DocumentDraftUpdateRequest": {
                    "type": "object",
                    "description": "Save against the original returned draft source. Missing or invalid source produces 409 source_required; changed source produces 409 source_conflict. Retain edits for review; never rebase from a latest GET.",
                    "properties": {
                        "source_digest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                        "title": { "type": "string", "maxLength": 500 },
                        "icon": { "type": "string", "nullable": true },
                        "cover_image": { "type": "string", "nullable": true },
                        "content": { "type": "string", "maxLength": 5242880 }
                    },
                    "required": ["source_digest"],
                    "minProperties": 1
                },
                "DocumentDraftAccess": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" },
                        "display_name": { "type": "string" },
                        "email": { "type": "string", "format": "email" },
                        "role": { "type": "string", "enum": ["viewer", "editor"] },
                        "created_at": { "type": "string", "format": "date-time" },
                        "updated_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["user_id", "display_name", "email", "role", "created_at", "updated_at"]
                },
                "DocumentDraftAccessRequest": {
                    "type": "object",
                    "properties": {
                        "role": { "type": "string", "enum": ["viewer", "editor"] }
                    },
                    "required": ["role"]
                },
                "DocumentDraftAccessCandidate": {
                    "type": "object",
                    "properties": {
                        "user_id": { "type": "string", "format": "uuid" },
                        "display_name": { "type": "string" },
                        "email": { "type": "string", "format": "email" }
                    },
                    "required": ["user_id", "display_name", "email"]
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
                        "maintenance_user_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."},
                        "maintenance_group_id": {"type": "string", "nullable": true, "description": "Exact responsibility target ID; metadata conveys no access."},
                        "human_identifier": { "type": "string", "nullable": true, "description": "Optional readable reference; trimmed ASCII letters, digits, dots, underscores or hyphens, starting alphanumeric, 1–64 normalized characters. Case-preserving and reserved per space across lifecycle states." },
                        "document_id": { "type": "string", "format": "uuid" },
                        "title": { "type": "string" },
                        "space_id": { "type": "string", "format": "uuid" },
                        "space_name": { "type": "string" },
                        "excerpt": { "type": "string" },
                        "updated_at": { "type": "string", "format": "date-time" },
                        "author": { "type": "string" },
                        "author_id": { "type": "string", "format": "uuid" },
                        "folder_id": { "type": "string", "format": "uuid", "nullable": true },
                        "folder_name": { "type": "string", "nullable": true },
                        "is_draft": { "type": "boolean" },
                        "draft_id": { "type": "string", "format": "uuid", "nullable": true }
                    },
                    "required": ["document_id", "title", "space_id", "space_name", "excerpt", "updated_at", "author", "author_id", "folder_id", "folder_name", "is_draft", "draft_id"]
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
                        "enabled_locked": { "type": "boolean" },
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
                    "required": ["enabled", "enabled_locked", "default_provider", "providers"]
                },
                "AiProvider": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "enum": ["ollama", "openai", "anthropic", "xai"] },
                        "label": { "type": "string" },
                        "configured": { "type": "boolean" },
                        "source": { "type": "string", "enum": ["environment", "database", "unconfigured"] },
                        "locked": { "type": "boolean" },
                        "base_url_locked": { "type": "boolean" },
                        "base_url": { "type": "string", "format": "uri" },
                        "model": { "type": "string" },
                        "has_api_key": { "type": "boolean" },
                        "api_key_locked": { "type": "boolean" },
                        "removable": { "type": "boolean" },
                        "is_default": { "type": "boolean" }
                    },
                    "required": ["id", "label", "configured", "source", "locked", "base_url_locked", "base_url", "model", "has_api_key", "api_key_locked", "removable", "is_default"]
                },
                "AiProviderSaveRequest": {
                    "type": "object",
                    "properties": {
                        "base_url": { "type": "string", "format": "uri" },
                        "api_key": { "type": "string", "nullable": true, "writeOnly": true },
                        "model": { "type": "string" }
                    },
                    "required": ["base_url", "model"]
                },
                "AiProviderTestResponse": {
                    "type": "object",
                    "properties": {
                        "connected": { "type": "boolean" },
                        "message": { "type": "string" },
                        "models": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["connected", "message", "models"]
                },
                "AiSettingsUpdateRequest": {
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean", "nullable": true },
                        "default_provider": { "type": "string", "enum": ["ollama", "openai", "anthropic", "xai"], "nullable": true }
                    }
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
                "PublishedSourceReplayEvidence": {"type":"object","properties":{
                    "published_source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},
                    "payload_fingerprint":{"type":"string","pattern":"^[0-9a-f]{64}$"},
                    "original_source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},
                    "document_id":{"type":"string"},"space_id":{"type":"string"}
                },"required":["published_source_digest","payload_fingerprint","original_source_digest","document_id","space_id"],"description":"Exact new immutable published replay result; omitted for historical old receipts and nonpublished replay. Client must bind every field to its original dispatched request before advancing source."},
                "SyncReplayOperationResult": {
                    "type": "object",
                    "properties": {
                        "operation_id": { "type": "string" },
                        "entity_type": { "type": "string" },
                        "action": { "type": "string" },
                        "status": { "type": "string" },
                        "message": { "type": "string" },
                        "published_source": {"$ref":"#/components/schemas/PublishedSourceReplayEvidence"}
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
                        "events": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 1,
                            "items": { "type": "string", "enum": ["public_page.published"] }
                        },
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
                        "created_by": { "type": "string", "format": "uuid" },
                        "created_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "name", "url", "events", "space_id", "is_active", "created_by", "created_at"]
                },
                "WebhookTestQueued": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": ["queued"] },
                        "delivery_id": { "type": "string", "format": "uuid" }
                    },
                    "required": ["status", "delivery_id"]
                },
                "WebhookDelivery": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "event_type": { "type": "string" },
                        "test_id": { "type": "string", "format": "uuid", "nullable": true },
                        "response_status": { "type": "integer", "nullable": true },
                        "success": { "type": "boolean" },
                        "attempt_number": { "type": "integer" },
                        "delivered_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["id", "event_type", "test_id", "response_status", "success", "attempt_number", "delivered_at"]
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

fn publication_snapshot_schema(detail: bool) -> Value {
    let hash=json!({"type":"string","pattern":"^[0-9a-f]{64}$"});
    let mut schema=json!({"type":"object","additionalProperties":false,
        "required":["schema_version","id","document_id","captured_space_id","kind","capture_operation_id","publication_operation_id","published_at","captured_at","captured_by","snapshot_digest","binding","content"],
        "properties":{"schema_version":{"type":"integer","const":1},"id":{"type":"string","description":"Original capture operation UUID plus :publication or :baseline; not a bare UUID."},"document_id":{"type":"string"},"captured_space_id":{"type":"string","description":"Historical capture provenance; supplies no current route or asset authority."},"kind":{"type":"string","enum":["publication","baseline"]},"capture_operation_id":{"type":"string","format":"uuid"},"publication_operation_id":{"type":["string","null"],"description":"Equal to capture_operation_id for publication; null for baseline."},"published_at":{"type":"string"},"captured_at":{"type":"string"},"captured_by":{"type":"string"},"snapshot_digest":hash,"binding":{"type":"string","enum":["capture_v2","legacy_content_v1"]},
        "content":{"type":"object","additionalProperties":false,"required":["title","icon","cover_reference","extraction","raw_sha256","raw_byte_length"],"properties":{"title":{"type":"string"},"icon":{"type":["string","null"]},"cover_reference":{"type":["string","null"],"description":"Literal captured reference, not a download grant."},"extraction":{"type":"string","enum":["exact_published_draft","proven_generated_header","raw_only"]},"raw_sha256":hash,"raw_byte_length":{"type":"integer","minimum":0,"maximum":6000000}}}}});
    schema["properties"]["snapshot_digest"]["description"]=json!("Digest of the complete private captured payload, not this redacted wire projection.");
    if detail {
        schema["required"].as_array_mut().unwrap().extend(["comparison_policy","asset_policy","comparisons","raw_download_url"].into_iter().map(|key|json!(key)));
        schema["properties"]["content"]["required"].as_array_mut().unwrap().push(json!("portable_markdown"));
        schema["properties"]["content"]["properties"]["portable_markdown"]=json!({"type":["string","null"],"description":"Exact portable source only when extraction is proven. Null for raw_only; never reconstruct it from raw Markdown by guessing."});
        schema["properties"]["comparison_policy"]=json!({"type":"string","enum":["comparison_only_capture_v2_bound","comparison_only_observed_at_commit_original_prepare_unbound"],"description":"Properties are comparison-only observations at capture/commit. Baseline comparisons are not reconstructed original-publication-time metadata. Legacy comparison observations are not bound to the earlier original prepare."});
        schema["properties"]["asset_policy"]=json!({"type":"string","const":"reference_only_current_authorization_and_existence_required"});
        schema["properties"]["raw_download_url"]=json!({"type":"string","description":"Relative /api/v1 URL under the current route. The raw GET independently requires current ordinary Edit and immutable capture proof."});
        schema["properties"]["comparisons"]=json!({"type":"object","additionalProperties":false,"required":["properties_schema","property_values","attachments","drawings"],"properties":{
            "properties_schema":{"type":"array","description":"Only captured definitions referenced by captured document values; no unrelated space schema expansion.","items":{"type":"object","additionalProperties":false,"required":["id","name","property_type","options_raw","sort_order"],"properties":{"id":{"type":"string"},"name":{"type":"string"},"property_type":{"type":"string"},"options_raw":{"type":["string","null"],"description":"Exact raw SQL options string, including unsupported or invalid historical JSON."},"sort_order":{"type":"integer"}}}},
            "property_values":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["property_id","value"],"properties":{"property_id":{"type":"string"},"value":{"type":["string","null"],"description":"Exact captured string; no coercion, relation expansion or property restoration."}}}},
            "attachments":{"type":"array","description":"Reference observations only; not retained blob bytes or current download permission.","items":{"type":"object","additionalProperties":false,"required":["id","original_name","mime_type","size_bytes"],"properties":{"id":{"type":"string"},"original_name":{"type":"string"},"mime_type":{"type":"string"},"size_bytes":{"type":"integer"}}}},
            "drawings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","filename"],"properties":{"id":{"type":"string"},"filename":{"type":"string"}}}}}});
    }
    schema
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
        [
            (
                "put",
                "Authentication",
                "Rename a current user session",
                true,
            ),
            (
                "delete",
                "Authentication",
                "Revoke a current user session",
                true,
            ),
        ],
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
    set_request_body(
        &mut paths,
        "/auth/sessions/{session_id}",
        "put",
        json!({ "$ref": "#/components/schemas/SessionUpdateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/auth/sessions/{session_id}",
        "put",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/UserSession" } }),
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
        (
            "/auth/native-handoff/start",
            "NativeHandoffStartRequest",
            "NativeHandoffStartResponse",
            false,
        ),
        (
            "/auth/native-handoff/complete",
            "NativeHandoffCompleteRequest",
            "NativeHandoffCompleteResponse",
            true,
        ),
        (
            "/auth/native-handoff/exchange",
            "NativeHandoffExchangeRequest",
            "AuthResponse",
            false,
        ),
    ] {
        add_path(
            &mut paths,
            path,
            [(
                "post",
                "Authentication",
                "Native browser authentication handoff",
                secured,
            )],
        );
        set_request_body(
            &mut paths,
            path,
            "post",
            json!({ "$ref": format!("#/components/schemas/{request}") }),
        );
        set_json_response(
            &mut paths,
            path,
            "post",
            "200",
            json!({ "$ref": format!("#/components/schemas/{response}") }),
        );
    }
    for path in [
        "/auth/native-handoff/start",
        "/auth/native-handoff/exchange",
    ] {
        set_error_response(&mut paths, path, "post", "429", "Rate limit exceeded.");
    }
    set_json_response(
        &mut paths,
        "/auth/native-handoff/exchange",
        "post",
        "202",
        json!({ "$ref": "#/components/schemas/NativeHandoffPendingResponse" }),
    );
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
    for (path, summary) in [("/admin/users/{user_id}/guest/review", "Review classifying an existing account as a guest"),
        ("/admin/users/{user_id}/guest/confirm", "Confirm classifying an existing account as a guest")] {
        add_path(&mut paths,path,[("post","Administration",summary,true)]);
        set_string_path_parameters(&mut paths,path,[("user_id","Original selected active ordinary account; self and administrators are excluded.")]);
        for status in ["201","202","204"] { paths.get_mut(path).unwrap()["post"]["responses"].as_object_mut().unwrap().remove(status); }
        for (status,description) in [("401","Current human instance-administrator credential unavailable."),("404","Selected account is unavailable or ineligible."),
            ("409","Original account or authority source changed; retain the original request and inspect current state."),("500","Storage or audit failed; no confirmation is acknowledged.")] {
            set_error_response(&mut paths,path,"post",status,description);
        }
        paths.get_mut(path).unwrap()["post"]["responses"]["422"] = json!({"description":"Request JSON structure rejected.","content":{"text/plain":{"schema":{"type":"string"}}}});
    }
    let guest_review_path="/admin/users/{user_id}/guest/review";
    set_request_body(&mut paths,guest_review_path,"post",json!({"type":"object","additionalProperties":false,"properties":{}}));
    set_json_response(&mut paths,guest_review_path,"post","200",json!({"type":"object","additionalProperties":false,"required":["schema_version","target","source_digest","expires_at"],
        "properties":{"schema_version":{"type":"integer","const":1},"target":{"type":"object","additionalProperties":false,"required":["id","display_name","account_kind"],
            "properties":{"id":{"type":"string"},"display_name":{"type":"string"},"account_kind":{"type":"string","const":"member"}}},
            "source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},"expires_at":{"type":"string","format":"date-time"}}}));
    paths.get_mut(guest_review_path).unwrap()["post"]["description"]=json!("Fresh read-only review by a current human instance administrator, bound to the selected ordinary account, original credential and current source. No classification, grant or association changes.");
    let guest_confirm_path="/admin/users/{user_id}/guest/confirm";
    set_request_body(&mut paths,guest_confirm_path,"post",json!({"type":"object","additionalProperties":false,"required":["source_digest","confirm"],
        "properties":{"source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},"confirm":{"type":"boolean","const":true}}}));
    set_json_response(&mut paths,guest_confirm_path,"post","200",json!({"type":"object","additionalProperties":false,"required":["confirmed","user_id","account_kind"],
        "properties":{"confirmed":{"type":"boolean","const":true},"user_id":{"type":"string"},"account_kind":{"type":"string","const":"guest"}}}));
    paths.get_mut(guest_confirm_path).unwrap()["post"]["description"]=json!("Explicit one-way member-to-guest classification of an existing active ordinary account; self and administrators are excluded. Rechecks the original reviewed source and current human administrator credential inside the guarded transaction; account kind and attributed audit commit together. Existing resources, associations and IDs remain, but guest kind suppresses inherited space/group/global/fallback and human administrator authority. No page grant, invitation or member conversion is created. An interrupted confirmation is unconfirmed; fresh inspection is separate from acknowledgement and no automatic confirmation retry is offered.");
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
    add_path(&mut paths, "/admin/storage-health", [("get", "Administration", "Observe independent storage availability and applied history", true)]);
    set_json_response(&mut paths, "/admin/storage-health", "get", "200", json!({"$ref":"#/components/schemas/StorageHealthReport"}));
    add_path(
        &mut paths,
        "/admin/storage-integrity",
        [(
            "get",
            "Administration",
            "Audit database-owned storage paths",
            true,
        )],
    );
    set_json_response(
        &mut paths,
        "/admin/storage-integrity",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/StorageIntegrityReport" }),
    );
    for path in ["/admin/storage-health", "/admin/storage-integrity"] {
        paths[path]["get"]["description"] = json!("Preserves existing administrator credentials: a current administrator access session or valid personal/service API key with administrator scope. Original account/session or key identity, scopes and expiry are resolved before observation and again before returning evidence. A refused observation returns no storage report; these routes grant no repair, deletion or restore authority.");
    }

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
        "/admin/groups/{group_id}",
        [
            (
                "put",
                "Users and Groups",
                "Update a manually managed group",
                true,
            ),
            (
                "delete",
                "Users and Groups",
                "Delete a manually managed group",
                true,
            ),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/groups/{group_id}",
        [("group_id", "Group ID to update or delete.")],
    );
    set_request_body(
        &mut paths,
        "/admin/groups/{group_id}",
        "put",
        json!({ "$ref": "#/components/schemas/GroupUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/admin/groups/{group_id}",
        "put",
        "204",
        "Group updated.",
    );
    set_empty_response(
        &mut paths,
        "/admin/groups/{group_id}",
        "delete",
        "204",
        "Group, its memberships, and its space grants deleted.",
    );
    add_path(
        &mut paths,
        "/admin/groups/{group_id}/spaces",
        [(
            "get",
            "Users and Groups",
            "List the spaces granted to a group",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/admin/groups/{group_id}/spaces",
        [("group_id", "Group ID whose space grants are listed.")],
    );
    set_json_response(
        &mut paths,
        "/admin/groups/{group_id}/spaces",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/GroupSpaceGrant" } }),
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
    paths.get_mut("/spaces/{space_id}").unwrap()["delete"]["description"] = json!("Delete a space using current human Write/Admin and the existing owner-only space authority, including its existing instance-admin interpretation. Preserve existing SQL cascades and SET NULL effects under ordered audience, related-space and document guards. The deletion and accurately attributed space.delete audit commit together after exact source checks and original credential reproof. Canonical files, private mirrors, assets and immutable receipts are retained; physical cleanup is deferred. An unknown commit returns conflict and requires inspection before another deliberate request, without an idempotent retry or physical-erasure guarantee.");
    set_error_response(&mut paths, "/spaces/{space_id}", "delete", "403", "Current human Write and existing owner authority are required.");
    set_error_response(&mut paths, "/spaces/{space_id}", "delete", "409", "Source changed or deletion outcome is unknown; inspect current state before another deletion.");
    add_path(
        &mut paths,
        "/spaces/{space_id}/retention",
        [
            ("get", "Spaces", "Read space Trash retention", true),
            ("put", "Spaces", "Update space Trash retention", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/retention",
        [("space_id", "Space ID whose Trash retention is managed.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/retention",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/SpaceRetentionResponse" }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/retention",
        "put",
        json!({ "$ref": "#/components/schemas/SpaceRetentionUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/retention",
        "put",
        "204",
        "Trash retention updated.",
    );
    add_path(
        &mut paths,
        "/spaces/role-definitions",
        [(
            "get",
            "Spaces",
            "List built-in space role permissions",
            true,
        )],
    );
    set_json_response(
        &mut paths,
        "/spaces/role-definitions",
        "get",
        "200",
        json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "enum": ["space_admin", "editor", "commenter", "viewer"] },
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "permissions": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["view", "comment", "edit_content", "manage_members", "manage_space"] }
                    }
                },
                "required": ["id", "name", "description", "permissions"]
            }
        }),
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
        "/spaces/{space_id}/access-users",
        [(
            "get",
            "Spaces",
            "List effective user access with direct and group sources",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/access-users",
        [("space_id", "Space ID whose effective access is listed.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/access-users",
        "get",
        "200",
        json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "user_id": { "type": "string", "format": "uuid" },
                    "email": { "type": "string", "format": "email" },
                    "display_name": { "type": "string" },
                    "effective_role": { "type": "string", "nullable": true },
                    "direct_role": { "type": "string", "nullable": true },
                    "blocked": { "type": "boolean" },
                    "sources": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["direct", "group"] },
                                "id": { "type": "string" },
                                "name": { "type": "string" },
                                "role": { "type": "string" }
                            },
                            "required": ["kind", "id", "name", "role"]
                        }
                    }
                },
                "required": ["user_id", "email", "display_name", "effective_role", "direct_role", "blocked", "sources"]
            }
        }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/blocks/{user_id}",
        [
            ("put", "Spaces", "Block a user from a space", true),
            ("delete", "Spaces", "Remove a space user block", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/blocks/{user_id}",
        [
            ("space_id", "Space ID that owns the access override."),
            ("user_id", "User ID to block or unblock."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/blocks/{user_id}",
        "put",
        "204",
        "User blocked from the space.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/blocks/{user_id}",
        "delete",
        "204",
        "Space user block removed.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/roles",
        [
            (
                "get",
                "Spaces",
                "List built-in and space-specific roles",
                true,
            ),
            ("post", "Spaces", "Create a space-specific role", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/roles",
        [("space_id", "Space ID whose roles are managed.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/roles",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/SpaceRoleDefinition" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/roles",
        "post",
        json!({ "$ref": "#/components/schemas/SpaceRoleRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/roles",
        "post",
        "201",
        json!({
            "type": "object",
            "properties": { "id": { "type": "string" } },
            "required": ["id"]
        }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/roles/{role_id}",
        [
            ("put", "Spaces", "Update a space-specific role", true),
            (
                "delete",
                "Spaces",
                "Delete an unused space-specific role",
                true,
            ),
        ],
    );
    set_string_path_parameters(
        &mut paths,
        "/spaces/{space_id}/roles/{role_id}",
        [
            ("space_id", "Space ID that owns the role."),
            ("role_id", "Space-specific role ID."),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/roles/{role_id}",
        "put",
        json!({ "$ref": "#/components/schemas/SpaceRoleRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/roles/{role_id}",
        "put",
        "204",
        "Space role updated.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/roles/{role_id}",
        "delete",
        "204",
        "Space role deleted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/principals",
        [(
            "get",
            "Spaces",
            "List users and groups available to a space administrator",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/principals",
        [(
            "space_id",
            "Space ID whose assignable principals are listed.",
        )],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/principals",
        "get",
        "200",
        json!({
            "type": "object",
            "properties": {
                "users": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "format": "uuid" },
                            "email": { "type": "string", "format": "email" },
                            "display_name": { "type": "string" }
                        },
                        "required": ["id", "email", "display_name"]
                    }
                },
                "groups": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "format": "uuid" },
                            "name": { "type": "string" },
                            "description": { "type": "string", "nullable": true },
                            "member_count": { "type": "integer", "minimum": 0 },
                            "managed_space_id": { "type": "string", "format": "uuid", "nullable": true }
                        },
                        "required": ["id", "name", "description", "member_count", "managed_space_id"]
                    }
                }
            },
            "required": ["users", "groups"]
        }),
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
        "/spaces/{space_id}/groups/{group_id}/members",
        [
            ("get", "Spaces", "List members of an assigned group", true),
            ("post", "Spaces", "Add a member to an assigned group", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}/members",
        [
            ("space_id", "Space ID that owns the group assignment."),
            ("group_id", "Assigned group whose membership is managed."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}/members",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/GroupMember" } }),
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}/members",
        "post",
        json!({ "$ref": "#/components/schemas/GroupMemberAddRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}/members",
        "post",
        "201",
        "Assigned group member added.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}/members/{user_id}",
        [(
            "delete",
            "Spaces",
            "Remove a member from an assigned group",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}/members/{user_id}",
        [
            ("space_id", "Space ID that owns the group assignment."),
            ("group_id", "Assigned group whose membership is managed."),
            ("user_id", "User ID removed from the assigned group."),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/groups/{group_id}/members/{user_id}",
        "delete",
        "204",
        "Assigned group member removed.",
    );

    add_path(
        &mut paths,
        "/spaces/{space_id}/moves/preview",
        [(
            "post",
            "Documents",
            "Preview a document or folder move",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/moves/preview",
        [("space_id", "Actual source space.")],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/moves/preview",
        "post",
        json!({"type":"object","additionalProperties":false,"required":["root","destination"],"properties":{"root":{"type":"object","additionalProperties":false,"required":["kind","id"],"properties":{"kind":{"type":"string","enum":["document","folder"]},"id":{"type":"string"}}},"destination":{"type":"object","additionalProperties":false,"required":["space_id"],"properties":{"space_id":{"type":"string"},"folder_id":{"type":["string","null"]},"parent_id":{"type":["string","null"]}}}}}),
    );
    add_path(
        &mut paths,
        "/moves/previews/{preview_id}",
        [
            (
                "get",
                "Documents",
                "Read and revalidate an actor-owned move preview",
                true,
            ),
            (
                "delete",
                "Documents",
                "Cancel an actor-owned move preview",
                true,
            ),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/moves/previews/{preview_id}",
        [(
            "preview_id",
            "Actor-bound preview ID; expires after ten minutes.",
        )],
    );
    set_empty_response(
        &mut paths,
        "/moves/previews/{preview_id}",
        "delete",
        "204",
        "Review cancelled without relocating content.",
    );
    for (path, method) in [
        ("/spaces/{space_id}/moves/preview", "post"),
        ("/moves/previews/{preview_id}", "get"),
    ] {
        set_json_response(
            &mut paths,
            path,
            method,
            "200",
            json!({"type":"object","required":["preview_id","expires_at","fingerprint","stale","inventory","access_consequences","blockers","commit_available"],"properties":{"preview_id":{"type":"string"},"expires_at":{"type":"string","format":"date-time"},"fingerprint":{"type":"string"},"stale":{"type":"boolean"},"inventory":{"type":"object"},"access_consequences":{"type":"object"},"blockers":{"type":"array","items":{"type":"string"}},"commit_available":{"type":"boolean"}}}),
        );
        set_error_response(
            &mut paths,
            path,
            method,
            "404",
            "Missing, inaccessible, expired or cancelled preview/destination.",
        );
    }
    set_error_response(
        &mut paths,
        "/moves/previews/{preview_id}",
        "delete",
        "404",
        "Missing, inaccessible or expired preview.",
    );

    let viewer_grant_inventory = json!({"type":"object","additionalProperties":false,
        "required":["grant_id","document_id","revision","role","scope","recipient","preserve_allowed","remove_allowed"],
        "properties":{"grant_id":{"type":"string"},"document_id":{"type":"string"},"revision":{"type":"string"},
            "role":{"type":"string","enum":["viewer"]},"scope":{"type":"string","enum":["page"]},
            "recipient":{"type":"object","additionalProperties":false,"required":["available","display_name"],
                "properties":{"available":{"type":"boolean"},"display_name":{"type":["string","null"]}}},
            "preserve_allowed":{"type":"boolean"},"remove_allowed":{"type":"boolean"}}});
    for (path, method) in [("/spaces/{space_id}/moves/preview","post"),("/moves/previews/{preview_id}","get")] {
        paths.get_mut(path).unwrap()[method]["responses"]["200"]["content"]["application/json"]["schema"]["properties"]["inventory"]["properties"]["viewer_grants"] =
            json!({"type":"array","items":viewer_grant_inventory.clone(),"description":"Current manager-scoped direct viewer grants; availability is advisory and never confirmation authority."});
    }
    let grant_decision = json!({"type":"object","additionalProperties":false,"required":["grant_id","revision","choice"],
        "properties":{"grant_id":{"type":"string"},"revision":{"type":"string"},"choice":{"oneOf":[
            {"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"type":"string","enum":["preserve"]}}},
            {"type":"object","additionalProperties":false,"required":["kind","confirm"],"properties":{"kind":{"type":"string","enum":["remove"]},"confirm":{"type":"boolean","const":true}}}
        ]}}});
    let move_commit_path = "/moves/previews/{preview_id}/commit";
    add_path(&mut paths, move_commit_path, [("post","Documents","Confirm a reviewed subtree move",true)]);
    set_string_path_parameters(&mut paths, move_commit_path, [("preview_id","Original actor-bound reviewed preview ID.")]);
    set_request_body(&mut paths, move_commit_path, "post", json!({"type":"object","additionalProperties":false,
        "required":["fingerprint","decisions","public_links","confirm_audience_change","confirm_draft_space_change"],
        "properties":{"fingerprint":{"type":"string"},"decisions":{"type":"array","items":{"type":"object"}},
            "identifier_decisions":{"type":"array","items":{"type":"object"}},"maintenance_decisions":{"type":"array","items":{"type":"object"}},
            "incident_relations":{"type":"array","items":{"type":"object"}},"viewer_grant_decisions":{"type":"array","items":grant_decision},
            "public_links":{"type":"string","enum":["preserve","disable"]},"confirm_audience_change":{"type":"boolean"},"confirm_draft_space_change":{"type":"boolean"}}}));
    paths.get_mut(move_commit_path).unwrap()["post"]["description"] = json!("Current original human Write/Admin plus actual source/destination move authority. Every existing direct viewer grant requires its exact reviewed ID/revision and an explicit preserve or confirmed remove choice. Omitted/empty grant choices remain compatible only with no-grant moves. Source grant management is separate from MoveContent; preservation additionally requires destination grant management and actual account View, active member or guest unblocked recipient and eligible published page/ancestor policy. Exact pages, grant disposition, audit and sealed receipt commit atomically. Missing/stale/unauthorized choices make no partial move. Historical acknowledgement proves only the original operation and never reinstalls grants revoked afterwards or claims current audience.");
    for status in ["201","202","204"] { paths.get_mut(move_commit_path).unwrap()["post"]["responses"].as_object_mut().unwrap().remove(status); }
    set_json_response(&mut paths, move_commit_path, "post", "200", json!({"type":"object","required":["operation_id","committed"],
        "properties":{"operation_id":{"type":"string"},"committed":{"type":"boolean","const":true},"historical_receipt":{"type":"boolean"},
            "commit_acknowledgement_recovered":{"type":"boolean"},"root_id":{"type":"string"},"destination_space_id":{"type":"string"},
            "moved_at":{"type":"string","format":"date-time"},"preserved_metadata_url":{"type":"string"}}}));
    for (status,message) in [("400","Invalid move or missing, duplicate, foreign, incomplete grant selectors or unconfirmed removal."),("401","Current credential unavailable."),("403","Required authority unavailable."),
        ("404","Preview or target unavailable."),("409","Original source, choices or current authority changed; retain the original move request and inspect."),
        ("500","Storage or audit failed; no successful move is acknowledged.")] { set_error_response(&mut paths,move_commit_path,"post",status,message); }
    paths.get_mut(move_commit_path).unwrap()["post"]["responses"]["422"] = json!({"description":"Request JSON structure rejected.","content":{"text/plain":{"schema":{"type":"string"}}}});

    add_path(
        &mut paths,
        "/spaces/{space_id}/folders",
        [
            ("get", "Documents", "List folders", true),
            ("post", "Documents", "Create folder", true),
        ],
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/folders",
        "get",
        "404",
        "Missing or inaccessible space.",
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
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/folders/{folder_id}/move",
        "post",
        "409",
        "Cross-space relocation requires a reviewed metadata and access preview; no changes are made.",
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
    for (path, methods) in [
        (
            "/spaces/{space_id}/folders/{folder_id}",
            &["get", "put", "delete"][..],
        ),
        (
            "/spaces/{space_id}/folders/{folder_id}/reorder",
            &["put"][..],
        ),
    ] {
        for method in methods {
            set_error_response(
                &mut paths,
                path,
                method,
                "404",
                "Missing, inaccessible or foreign-space folder; no mutation is made.",
            );
        }
    }
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
    let trash_delete_path = "/spaces/{space_id}/documents/{document_id}/trash";
    add_path(&mut paths, trash_delete_path, [("delete", "Documents", "Permanently remove an authorized trashed page", true)]);
    paths.get_mut(trash_delete_path).unwrap()["parameters"] = json!([
        {"name":"space_id","in":"path","required":true,"schema":{"type":"string"}},
        {"name":"document_id","in":"path","required":true,"schema":{"type":"string"}}
    ]);
    paths.get_mut(trash_delete_path).unwrap()["delete"]["description"] = json!("Permanent metadata deletion of the exact original trashed page in the routed space. Requires current human Write and DeleteContent, private editor participation and ancestor restrictions; existing body locks do not bar deletion. Mandatory x-starlane-delete-operation is one stable canonical lowercase UUID. Mandatory original_target carries exact raw original created_at and deleted_at strings from authorized Trash; do not normalize. The same original UUID and witness reconcile an unknown outcome only under the current original human and current permission; a changed incarnation or Trash cycle, recreated same-ID page, foreign actor or legacy unwitnessed request refuses review. Child-bearing or inconsistent sources conflict without changes. Final source/cascade/authority checks precede commit. Files are retained: metadata 204 is not physical erasure; original attachment cleanup is a separate explicit operation. No synthetic replay receipt is created.");
    paths.get_mut(trash_delete_path).unwrap()["delete"]["parameters"] = json!([{"name":"x-starlane-delete-operation","in":"header","required":true,"schema":{"type":"string","format":"uuid","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"},"description":"Stable canonical original operation UUID; retain unchanged with exact witness across unknown-outcome retry."}]);
    set_request_body(&mut paths,trash_delete_path,"delete",json!({"$ref":"#/components/schemas/OriginalPermanentDeleteRequest"}));
    set_empty_response(&mut paths, trash_delete_path, "delete", "204", "Page and expected SQL associations removed; stored files retained.");
    paths.get_mut(trash_delete_path).unwrap()["delete"]["responses"]["204"]["headers"] = json!({"x-starlane-delete-operation":{"description":"Exact acknowledged original operation UUID; compare with the dispatched UUID.","schema":{"type":"string","format":"uuid"}}});
    let delete_responses = paths.get_mut(trash_delete_path).unwrap()["delete"]["responses"].as_object_mut().unwrap();
    delete_responses.remove("201");
    delete_responses.remove("202");
    for (status, message) in [
        ("401", "Original current human credential unavailable."),
        ("403", "Original credential does not permit writing."),
        ("404", "Trashed page unavailable in the routed space."),
        ("409", "Dependent children, inconsistent associations or changed source/policy; no deletion."),
        ("500", "Deletion or commit confirmation failed; files retained."),
    ] {
        set_error_response(&mut paths, trash_delete_path, "delete", status, message);
    }
    let cleanup_path="/spaces/{space_id}/documents/{document_id}/permanent-deletions/{operation_id}/cleanup";
    add_path(&mut paths,cleanup_path,[("post","Documents","Review and retry original attachment cleanup",true)]);
    paths.get_mut(cleanup_path).unwrap()["parameters"]=json!([{"name":"space_id","in":"path","required":true,"schema":{"type":"string"}},{"name":"document_id","in":"path","required":true,"schema":{"type":"string"}},{"name":"operation_id","in":"path","required":true,"schema":{"type":"string","format":"uuid","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"}}]);
    paths.get_mut(cleanup_path).unwrap()["post"]["description"]=json!("Separate explicit bounded cleanup of the exact original acknowledged deletion operation and raw witness under current original human/DeleteContent authority. Unknown outcomes retry only this UUID/witness. SQL plan or missing page alone never grants unlink or charge release; ownership-proved native paths, current references/leases and final authority are rechecked. Return pending, retained_review or physically_completed for supported original asset paths; unsupported/unproved classes remain retained and charged. Historical raw receipts remain provenance. A recreated same-ID page refuses cleanup. Metadata acknowledgement is not physical completion.");
    set_request_body(&mut paths,cleanup_path,"post",json!({"$ref":"#/components/schemas/OriginalAttachmentCleanupRequest"}));
    set_json_response(&mut paths,cleanup_path,"post","200",json!({"$ref":"#/components/schemas/OriginalAttachmentCleanupOutcome"}));
    for code in ["201","202","204"]{paths.get_mut(cleanup_path).unwrap()["post"]["responses"].as_object_mut().unwrap().remove(code);}
    for (code,message) in [("401","Current original human unavailable."),("403","Current original permission unavailable."),("404","Original deletion unavailable."),("409","Original UUID/witness/actor/incarnation or custody requires review."),("500","Cleanup outcome unknown; retain original request.")]{set_error_response(&mut paths,cleanup_path,"post",code,message);}
    paths.get_mut(trash_delete_path).unwrap().as_object_mut().unwrap().insert("post".into(), operation("Documents", "Restore an authorized trashed page", true));
    paths.get_mut(trash_delete_path).unwrap()["post"]["description"] = json!("Restore only a currently trashed page in the routed space. Requires current human Write and DeleteContent, private editor participation and ancestor restrictions; body locks do not bar restoration. Clear only deleted_at/deleted_by and update the timestamp; preserve children, metadata, publication/link flags and stored bytes, including any retained archive stamp. The transition and accurately attributed document.restore audit commit together after exact source/audit checks, actual nondeleted outcome policy and original credential reproof. A normal restored public link may become readable again; a retained archived state still suspends ordinary body and anonymous access. Unsafe search reindexing and mirror removal are deferred, leaving derived storage repair pending. No expiry purge or permanent deletion is performed.");
    set_empty_response(&mut paths, trash_delete_path, "post", "204", "Page restored from Trash; retained archive state and bytes preserved.");
    let restore_responses = paths.get_mut(trash_delete_path).unwrap()["post"]["responses"].as_object_mut().unwrap();
    for status in ["200", "201", "202"] { restore_responses.remove(status); }
    for (status, message) in [
        ("401", "Current human credential unavailable."),
        ("403", "Current credential or page cannot be restored."),
        ("404", "Trashed page unavailable."),
        ("409", "Restore source or audit changed; retry."),
        ("500", "Restore transition could not be confirmed; files retained."),
    ] { set_error_response(&mut paths, trash_delete_path, "post", status, message); }
    for (path, summary) in [
        (
            "/spaces/{space_id}/documents/archive",
            "List currently accessible archived page metadata",
        ),
        (
            "/spaces/{space_id}/documents/trash",
            "List currently accessible trashed page metadata",
        ),
    ] {
        add_path(&mut paths, path, [("get", "Documents", summary, true)]);
        set_uuid_path_parameters(
            &mut paths,
            path,
            [("space_id", "Space whose lifecycle metadata is inspected.")],
        );
        paths.get_mut(path).unwrap()["get"]["description"] = json!("Read-only metadata listing requiring the original current human Read credential, space View, page and ancestor restrictions and private-draft participation. Unpublished labels come only from the exact accessible draft. Inventory SQL precedes final current page/draft policy and original Read checks. No page body, restore authority, purge or lifecycle mirror writes; inspection is point-in-time.");
        set_json_response(
            &mut paths,
            path,
            "get",
            "200",
            json!({"type":"array", "items":{"$ref":"#/components/schemas/Document"}}),
        );
        for (status, message) in [
            ("401", "Current credential unavailable."),
            ("403", "Readable human credential required."),
            ("404", "Space or current metadata unavailable."),
            ("409", "Invalid access policy; review required."),
        ] {
            set_error_response(&mut paths, path, "get", status, message);
        }
    }
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
        "/spaces/{space_id}/drafts",
        [("get", "Documents", "List accessible drafts", true),
         ("delete", "Documents", "Purge all drafts in this space", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/drafts",
        [("space_id", "Space ID whose accessible drafts are listed.")],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/drafts",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentDraftSummary" } }),
    );
    paths.get_mut("/spaces/{space_id}/drafts").unwrap()["delete"]["description"] = json!("Explicit current human Write/Admin and ManageSpace whole-space purge, not a filtered owner or PageView operation. Remove every scoped draft and its share rows with an accurate actor audit in one transaction; retain canonical bytes, private mirrors, assets and immutable receipts. The confirmed deleted count is nonnegative, including zero; empty purge has no success audit. An unknown commit returns conflict and requires inspection before another deliberate request; no immutable purge receipt or automatic replay is provided.");
    set_json_response(&mut paths, "/spaces/{space_id}/drafts", "delete", "200",
        json!({"type":"object","properties":{"deleted":{"type":"integer","minimum":0}},"required":["deleted"]}));
    paths.get_mut("/spaces/{space_id}/drafts").unwrap()["delete"]["responses"].as_object_mut().unwrap().remove("204");
    set_error_response(&mut paths,"/spaces/{space_id}/drafts","delete","403","Current manager or Write capability unavailable.");
    set_error_response(&mut paths,"/spaces/{space_id}/drafts","delete","404","Space unavailable.");
    set_error_response(&mut paths,"/spaces/{space_id}/drafts","delete","409","Source changed or purge outcome is unknown; retain evidence and inspect current state.");
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts",
        [(
            "post",
            "Documents",
            "Create or resume a private draft",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID whose draft is created or resumed."),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts",
        "post",
        json!({ "$ref": "#/components/schemas/DocumentDraftCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts",
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/DocumentDraft" }),
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/DocumentDraft" }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/draft-transfers",
        [("post", "Documents", "Create a private draft from an exact reviewed local source", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/draft-transfers",
        [("space_id", "Destination space ID."), ("doc_id", "Destination document ID.")],
    );
    set_request_body(&mut paths, "/spaces/{space_id}/documents/{doc_id}/draft-transfers", "post",
        json!({"$ref":"#/components/schemas/DocumentDraftTransferRequest"}));
    set_json_response(&mut paths, "/spaces/{space_id}/documents/{doc_id}/draft-transfers", "post", "201",
        json!({"$ref":"#/components/schemas/DocumentDraftTransferReceipt"}));
    set_error_response(&mut paths, "/spaces/{space_id}/documents/{doc_id}/draft-transfers", "post", "409",
        "Original local review, server source, authority, or immutable receipt changed. Retain and retry only the exact original operation request.");
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}",
        [
            ("get", "Documents", "Read an accessible draft", true),
            ("put", "Documents", "Autosave an editable draft", true),
            ("delete", "Documents", "Delete an owned draft", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the draft."),
            ("draft_id", "Draft ID."),
        ],
    );
    for method in ["get", "put"] {
        set_json_response(
            &mut paths,
            "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}",
            method,
            "200",
            json!({ "$ref": "#/components/schemas/DocumentDraft" }),
        );
    }
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentDraftUpdateRequest" }),
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}",
        "put",
        "409",
        "source_required: original draft source is missing or invalid; source_conflict: original draft source changed. Retain edits and review before retrying; do not rebase from a latest GET.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}",
        "delete",
        "204",
        "Draft deleted.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish",
        [("post", "Documents", "Publish a draft into its space", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that receives the draft."),
            ("draft_id", "Owner draft ID to publish."),
        ],
    );
    let publication_path = "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish";
    let save_source_path = "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/save-source";
    add_path(&mut paths, save_source_path, [("get", "Documents", "Review a current selected draft save source", true)]);
    set_uuid_path_parameters(&mut paths, save_source_path, [("space_id", "Space ID."), ("doc_id", "Document ID."), ("draft_id", "Selected editable draft ID.")]);
    paths.get_mut(save_source_path).unwrap()["get"]["description"] = json!("Current ordinary Page Edit, selected Draft Edit and original human write identity authorize this uncached exact source review. No published View prerequisite; matching lock ownership alone grants no authority. Returns selected draft fields, exact selected participation/owner/timestamps and protected lock observation, never published body or access policy. selected_draft.document_title is the selected draft title, not the published canonical title. GET writes no draft, receipt or acknowledgement. A current digest is a CAS comparison source, not proof that an earlier REST save committed. Retained saves require deliberate review; do not silently rebase or automatically resume after unlock.");
    set_json_response(&mut paths, save_source_path, "get", "200", json!({"type":"object","additionalProperties":false,"required":["schema_version","space_id","document_id","draft_id","draft_source_digest","draft","locked","locked_by","page_edit_allowed","page_updated_at","page_published","selected_draft"],"properties":{
        "schema_version":{"type":"integer","const":1},"space_id":{"type":"string"},"document_id":{"type":"string"},"draft_id":{"type":"string"},"draft_source_digest":{"type":"string","pattern":"^[0-9a-f]{64}$"},
        "draft":{"type":"object","additionalProperties":false,"required":["title","content","icon","cover_image"],"properties":{"title":{"type":"string"},"content":{"type":"string"},"icon":{"type":["string","null"]},"cover_image":{"type":["string","null"]}}},
        "locked":{"type":"boolean"},"locked_by":{"type":["string","null"]},"page_edit_allowed":{"type":"boolean","const":true},"page_updated_at":{"type":"string"},"page_published":{"type":"boolean"},"selected_draft":{"$ref":"#/components/schemas/DocumentDraft"}}}));
    paths.get_mut(save_source_path).unwrap()["get"]["responses"]["200"]["headers"]["Cache-Control"] = json!({"schema":{"type":"string","const":"no-store"}});
    let publication_source_path = "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish-source";
    let publication_fields = json!({"type":"object","required":["title","content","icon","cover_image"],"properties":{
        "title":{"type":"string"},"content":{"type":"string"},"icon":{"type":["string","null"]},"cover_image":{"type":["string","null"]}}});
    let source_digest = json!({"type":"string","pattern":"^[0-9a-f]{64}$"});
    add_path(&mut paths, publication_source_path, [("get", "Documents", "Review an exact owner publication source", true)]);
    set_uuid_path_parameters(&mut paths, publication_source_path, [("space_id","Space ID."),("doc_id","Document ID."),("draft_id","Owned draft ID.")]);
    paths.get_mut(publication_source_path).unwrap()["get"]["description"] = json!("Current owner publication authority and original human write scope; no unrelated published View prerequisite. Returns exact raw draft digest and page proof for published or unpublished state. Page digest supplies no authority. Review projection is the owned draft, never an unauthorized published body. GET performs no publication or receipt write.");
    set_json_response(&mut paths, publication_source_path, "get", "200", json!({"type":"object","required":["schema_version","space_id","document_id","draft_id","draft_source_digest","page_source_digest","page_published","page_updated_at","draft"],"properties":{
        "schema_version":{"type":"integer","const":1},"space_id":{"type":"string"},"document_id":{"type":"string"},"draft_id":{"type":"string"},"draft_source_digest":source_digest,"page_source_digest":source_digest,"page_published":{"type":"boolean"},"page_updated_at":{"type":"string"},"draft":publication_fields}}));
    set_request_body(&mut paths, publication_path, "post", json!({"type":"object","additionalProperties":false,"required":["schema_version","operation_id","draft_source_digest","page_source_digest"],"properties":{
        "schema_version":{"type":"integer","const":1},"operation_id":{"type":"string","format":"uuid"},"draft_source_digest":source_digest,"page_source_digest":source_digest}}));
    paths.get_mut(publication_path).unwrap()["post"]["description"] = json!("Publish the owner's reviewed exact draft and page source in the protected canonical transaction. Preserve the original UUID/request across ambiguous outcomes. Exact same-actor receipt retry uses current page Edit authority and original identity, even after draft consumption; historical acknowledgement never republishes or supplies a current editing baseline. Unknown or mismatching storage evidence remains held. An exact original uncommitted stage may resume under unchanged reviewed sources and current authority using its original immutable revision. Fresh successful publication atomically records one typed publication snapshot and, when needed, an exact prior published baseline alongside page SQL, receipts, audit and draft consumption. Snapshot failure rolls back all publication SQL. Properties are comparison-only observations; assets are references, not historical blob retention. Native v2 physically binds the capture descriptor. Authentic pre-upgrade uncommitted stages retain content-only binding with explicitly unbound original-prepare comparison observations; old successful receipts never receive retrofitted snapshots. No audience grant, automatic offline publication or generic replay.");
    set_json_response(&mut paths, publication_path, "post", "200", json!({"type":"object","required":["schema_version","operation_id","space_id","document_id","draft_id","draft_source_digest","page_source_digest","published_source_digest","published_at","publication","disposition"],"properties":{
        "schema_version":{"type":"integer","const":1},"operation_id":{"type":"string","format":"uuid"},"space_id":{"type":"string"},"document_id":{"type":"string"},"draft_id":{"type":"string"},"draft_source_digest":source_digest,"page_source_digest":source_digest,"published_source_digest":source_digest,"published_at":{"type":"string"},"publication":publication_fields,"disposition":{"type":"string","enum":["applied","historical"]}}}));
    for (path, method) in [(publication_source_path,"get"),(publication_path,"post")] {
        let responses = paths.get_mut(path).unwrap()[method]["responses"].as_object_mut().unwrap();
        for status in ["201", "202", "204"] { responses.remove(status); }
        for (status, description) in [("401","Original current identity unavailable."),("403","Current publication authority unavailable."),("404","Page or required source unavailable."),("409","Exact source changed or publication outcome needs original-operation review.")] {
            set_error_response(&mut paths,path,method,status,description);
        }
    }
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access",
        [("get", "Documents", "List draft collaborators", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the draft."),
            ("draft_id", "Draft ID."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentDraftAccess" } }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access-candidates",
        [(
            "get",
            "Documents",
            "List eligible draft collaborators",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access-candidates",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the draft."),
            ("draft_id", "Draft ID."),
        ],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access-candidates",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentDraftAccessCandidate" } }),
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}",
        [
            ("put", "Documents", "Grant draft access", true),
            ("delete", "Documents", "Revoke draft access", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}",
        [
            ("space_id", "Space ID that owns the document."),
            ("doc_id", "Document ID that owns the draft."),
            ("draft_id", "Draft ID."),
            ("user_id", "User whose draft access is changed."),
        ],
    );
    set_request_body(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}",
        "put",
        json!({ "$ref": "#/components/schemas/DocumentDraftAccessRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}",
        "put",
        "204",
        "Draft access saved.",
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}",
        "delete",
        "204",
        "Draft access revoked.",
    );
    for (path, method) in [
        ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}", "delete"),
        ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access", "get"),
        ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access-candidates", "get"),
        ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}", "put"),
        ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}", "delete"),
    ] {
        paths[path][method]["description"] = json!(
            "Current draft owner and account PageView are required, including explicit lifecycle, blocks, restrictions and private ancestors. GET requires current Read; mutation requires current Write/Admin without adding EditContent or body-lock constraints. Shares do not grant published-page access. Canonical SQL is authoritative; mutable mirror bytes and immutable retry receipts are retained."
        );
    }
    let discussion_path =
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/discussion";
    add_path(
        &mut paths,
        discussion_path,
        [
            ("get", "Documents", "List selected-draft discussion", true),
            ("post", "Documents", "Create selected-draft comment", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        discussion_path,
        [
            ("space_id", "Space ID that owns the selected draft."),
            ("doc_id", "Document ID that owns the selected draft."),
            ("draft_id", "Exact selected draft ID."),
        ],
    );
    set_source_digest_query_parameter(&mut paths, discussion_path, "get");
    set_json_response(
        &mut paths,
        discussion_path,
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/DraftDiscussion" }),
    );
    set_request_body(
        &mut paths,
        discussion_path,
        "post",
        json!({ "$ref": "#/components/schemas/DraftDiscussionCreateRequest" }),
    );
    set_json_response(
        &mut paths,
        discussion_path,
        "post",
        "201",
        json!({ "$ref": "#/components/schemas/DraftDiscussionCreated" }),
    );
    let discussion_comment_path =
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/discussion/{comment_id}";
    add_path(
        &mut paths,
        discussion_comment_path,
        [
            ("put", "Documents", "Update owned selected-draft comment", true),
            ("delete", "Documents", "Delete owned selected-draft comment", true),
        ],
    );
    set_uuid_path_parameters(
        &mut paths,
        discussion_comment_path,
        [
            ("space_id", "Space ID that owns the selected draft."),
            ("doc_id", "Document ID that owns the selected draft."),
            ("draft_id", "Exact selected draft ID."),
            ("comment_id", "Draft comment ID."),
        ],
    );
    set_request_body(
        &mut paths,
        discussion_comment_path,
        "put",
        json!({ "$ref": "#/components/schemas/DraftDiscussionUpdateRequest" }),
    );
    set_source_digest_query_parameter(&mut paths, discussion_comment_path, "delete");
    set_empty_response(
        &mut paths,
        discussion_comment_path,
        "put",
        "204",
        "Draft comment updated.",
    );
    set_empty_response(
        &mut paths,
        discussion_comment_path,
        "delete",
        "204",
        "Draft comment deleted.",
    );
    let discussion_resolve_path =
        "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/discussion/{comment_id}/resolve";
    add_path(
        &mut paths,
        discussion_resolve_path,
        [("post", "Documents", "Resolve selected-draft comment", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        discussion_resolve_path,
        [
            ("space_id", "Space ID that owns the selected draft."),
            ("doc_id", "Document ID that owns the selected draft."),
            ("draft_id", "Exact selected draft ID."),
            ("comment_id", "Draft comment ID."),
        ],
    );
    set_source_digest_query_parameter(&mut paths, discussion_resolve_path, "post");
    set_empty_response(
        &mut paths,
        discussion_resolve_path,
        "post",
        "204",
        "Draft comment resolved.",
    );
    let closed_discussions_path =
        "/spaces/{space_id}/documents/{doc_id}/draft-discussions";
    add_path(
        &mut paths,
        closed_discussions_path,
        [("get", "Documents", "List owned closed draft discussion custody", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        closed_discussions_path,
        [
            ("space_id", "Current space ID for the published page."),
            ("doc_id", "Current document ID."),
        ],
    );
    set_json_response(
        &mut paths,
        closed_discussions_path,
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/ClosedDraftDiscussionList" }),
    );
    paths[closed_discussions_path]["get"]["description"] = json!("Read-only retained custody summaries for discussions owned by the current user after publication. Requires current page Comment access and returns at most 100 newest records. It does not reopen, publish or notify.");
    let closed_discussion_path =
        "/spaces/{space_id}/documents/{doc_id}/draft-discussions/{discussion_id}";
    add_path(
        &mut paths,
        closed_discussion_path,
        [("get", "Documents", "Inspect closed draft discussion custody", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        closed_discussion_path,
        [
            ("space_id", "Current space ID for the published page."),
            ("doc_id", "Current document ID."),
            ("discussion_id", "Closed draft discussion ID."),
        ],
    );
    set_json_response(
        &mut paths,
        closed_discussion_path,
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/DraftDiscussion" }),
    );
    for (path, method) in [
        (discussion_path, "get"),
        (discussion_path, "post"),
        (discussion_comment_path, "put"),
        (discussion_comment_path, "delete"),
        (discussion_resolve_path, "post"),
    ] {
        paths[path][method]["description"] = json!("Requires current access to the exact selected draft and its current lowercase SHA-256 source digest. Source drift returns 409 and must be reviewed; locks do not grant or remove comment authority. Comment anchors use UTF-16 offsets and report outdated rather than moving silently. Publication closes the discussion in the same transaction that consumes the draft.");
        set_error_response(
            &mut paths,
            path,
            method,
            "409",
            "source_required or source_conflict: retain the comment operation and refresh the exact selected draft before retrying.",
        );
    }
    paths[closed_discussion_path]["get"]["description"] = json!("Read-only retained custody after publication. Requires current page Comment access and the original draft owner; it does not reopen, publish, or notify. Anchors are marked outdated because the selected draft no longer exists.");
    for (path, method) in [
        ("/spaces/{space_id}/documents/{doc_id}/access", "get"),
        (
            "/spaces/{space_id}/documents/{doc_id}/access/inspect",
            "post",
        ),
    ] {
        add_path(
            &mut paths,
            path,
            [(
                method,
                "Documents",
                "Inspect current caller page access",
                true,
            )],
        );
        set_string_path_parameters(
            &mut paths,
            path,
            [
                ("space_id", "Current space ID."),
                ("doc_id", "Stable page ID."),
            ],
        );
        paths.get_mut(path).unwrap()[method]["description"] = json!("Read-only inspection using actual current Read credentials. POST performs no domain mutation and supplies fresh transport for clients that cache GET data. This is the caller's access, not a complete audience or grant-management capability; private drafts and anonymous links remain separate.");
        set_json_response(
            &mut paths,
            path,
            method,
            "200",
            json!({"$ref":"#/components/schemas/PageAccessInspection"}),
        );
        for (status, message) in [
            ("401", "Current credential unavailable."),
            ("403", "Read credential required."),
            ("404", "Page unavailable."),
            ("409", "Access source changed or invalid; retry inspection."),
        ] {
            set_error_response(&mut paths, path, method, status, message);
        }
    }
    for (stage, response_schema) in [
        ("inspect", "PageViewerGrantInspection"),
        ("review", "PageViewerGrantReview"),
        ("confirm", "PageViewerGrantOutcome"),
    ] {
        let path=format!("/spaces/{{space_id}}/documents/{{doc_id}}/viewer-grants/{stage}");
        add_path(&mut paths,&path,[("post","Documents",if stage=="inspect" {"Inspect direct published-page viewers"} else if stage=="review" {"Review a direct published-page viewer change"} else {"Confirm the exact reviewed page viewer change"},true)]);
        set_string_path_parameters(&mut paths,&path,[("space_id","Current target space ID."),("doc_id","Exact published page ID.")]);
        paths.get_mut(&path).unwrap()["post"]["description"]=json!(if stage=="confirm" {"Original current human Write or Admin credential and actual ManageSpace, with current named page management/private participation and ancestor restrictions on an active published target. Exact actor/target/proposal/source_digest must match the reviewed full source. Grant creation additionally requires actual account View before the original credential ceiling, an eligible active unblocked member or guest recipient, and an active published wholly inherited target/ancestor chain. Revocation may remove an already stored inactive or blocked recipient under current typed manager authority; creation-only wholly inherited/published-ancestor eligibility is not imposed on revocation, and unavailable recipient labels remain null. Ordered guarded grant and accurate human audit commit together. Confirmation acknowledgement loss is unknown: retain the original request and inspect; no automatic retry or queue. No space membership, invitation, group, subtree, editor/commenter or draft authority."} else {"Fresh bodyless inspection or explicit proposal review using original current human Read or Admin credential and actual named page restriction-management authority on an active published target. Inspection and revocation add no View prerequisite or creation-only wholly inherited/published-ancestor eligibility; current private participation, ancestor restrictions and space blocks still apply. Grant proposal review additionally requires actual account View and ManageSpace catalog authority, an eligible active unblocked member or guest recipient and an active published wholly inherited target/ancestor chain. Stored unavailable recipient IDs retain generic unavailable/null labels. Only direct member or guest viewer/page grants are supported. The digest binds actor, target, proposal and complete current source; it is not a permission or idempotent receipt."});
        if stage!="inspect" {set_request_body(&mut paths,&path,"post",json!({"$ref":if stage=="review" {"#/components/schemas/PageViewerGrantProposal"} else {"#/components/schemas/PageViewerGrantConfirmation"}}));}
        for status in ["201","202","204"] {paths.get_mut(&path).unwrap()["post"]["responses"].as_object_mut().unwrap().remove(status);}
        set_json_response(&mut paths,&path,"post","200",json!({"$ref":format!("#/components/schemas/{response_schema}")}));
        for (status,message) in [("401","Current credential unavailable."),("403","Required current human credential or management authority unavailable."),("404","Target or recipient unavailable."),("409","Source changed, confirmation invalid or acknowledgement unknown; retain original request and inspect without automatic retry."),("500","Storage or audit failed; no successful change is acknowledged.")] {set_error_response(&mut paths,&path,"post",status,message);}
        if stage!="inspect" {paths.get_mut(&path).unwrap()["post"]["responses"]["422"]=json!({"description":"JSON structure rejected by the request extractor.","content":{"text/plain":{"schema":{"type":"string"}}}});}
    }
    let shared_pages="/shared-pages/inspect";
    add_path(&mut paths,shared_pages,[("post","Documents","Inspect published pages explicitly shared with this account",true)]);
    paths.get_mut(shared_pages).unwrap()["post"]["description"]=json!("Fresh original human Read or Admin credential, including Read-only personal keys despite the POST method. Enumerates only this actual actor's exact direct published-page grants; each target rechecks current named published View, full source, activity, space blocks and ancestor policy. Fixed 20-page keyset response, no hidden names, placement catalogs, inaccessible totals or skipped target IDs. Cursor expires after ten minutes and binds original actor/credential and last RETURNED visible page identity; it grants no access. Admits eligible member and guest direct viewers. No drafts, whole-space membership/catalogs, subtree grants or mutations. Page-only main search is literal published title/body text; hidden catalog filters and advanced query syntax are excluded, and Dataview remains ordinary space-authorized only.");
    set_request_body(&mut paths,shared_pages,"post",json!({"$ref":"#/components/schemas/SharedPageInspectionRequest"}));
    for status in ["201","202","204"] {paths.get_mut(shared_pages).unwrap()["post"]["responses"].as_object_mut().unwrap().remove(status);}
    set_json_response(&mut paths,shared_pages,"post","200",json!({"$ref":"#/components/schemas/SharedPageInspection"}));
    for (status,message) in [("400","Cursor malformed, expired or from another original credential."),("401","Current credential unavailable."),("403","Current human Read credential required."),("409","Returned page or grant source changed; refresh inspection."),("500","Inspection storage failed.")] {set_error_response(&mut paths,shared_pages,"post",status,message);}
    paths.get_mut(shared_pages).unwrap()["post"]["responses"]["422"]=json!({"description":"JSON structure rejected by the request extractor.","content":{"text/plain":{"schema":{"type":"string"}}}});
    let restriction_inspect = "/spaces/{space_id}/documents/{doc_id}/restrictions/inspect";
    add_path(&mut paths, restriction_inspect, [("post", "Documents", "Inspect current page restrictions and scoped recipients", true)]);
    set_string_path_parameters(&mut paths, restriction_inspect, [("space_id", "Current space ID."), ("doc_id", "Stable page ID.")]);
    paths.get_mut(restriction_inspect).unwrap()["post"]["description"] = json!("Fresh bodyless manager inspection requiring original human Read and actual named restriction-management authority, private editor participation, ancestor restrictions and space blocks. No extra View prerequisite. Returns current local policy revision and stored selection IDs; unavailable selections retain IDs without hidden labels or reasons. Minimal eligible users/groups use the same scoped recipient policy as preview, including eligible SCIM groups. can_apply separately checks actual RestrictionApply with the original credential; it is a point-in-time hint, never a grant or confirmation. No ancestor allowlists, hidden descendants, emails, tokens or domain writes. Share controls remain separate integration work.");
    let restriction_selection = json!({"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","available"],"properties":{"id":{"type":"string"},"available":{"type":"boolean"}}}});
    let restriction_inspection = json!({"type":"object","additionalProperties":false,"required":["schema_version","space_id","document_id","checked_at","policy","eligible_recipients","can_apply"],"properties":{
        "schema_version":{"type":"integer","enum":[1]},"space_id":{"type":"string"},"document_id":{"type":"string"},"checked_at":{"type":"string","format":"date-time"},"can_apply":{"type":"boolean"},
        "policy":{"type":"object","additionalProperties":false,"required":["mode","revision","allowlist"],"properties":{"mode":{"type":"string","enum":["inherited","restricted"]},"revision":{"type":["string","null"]},"allowlist":{"type":"object","additionalProperties":false,"required":["users","groups"],"properties":{"users":restriction_selection.clone(),"groups":restriction_selection}}}},
        "eligible_recipients":{"type":"object","additionalProperties":false,"required":["users","groups"],"properties":{"users":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","display_name"],"properties":{"id":{"type":"string"},"display_name":{"type":"string"}}}},"groups":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","name"],"properties":{"id":{"type":"string"},"name":{"type":"string"}}}}}}
    }});
    for status in ["201", "202", "204"] {
        paths.get_mut(restriction_inspect).unwrap()["post"]["responses"].as_object_mut().unwrap().remove(status);
    }
    set_json_response(&mut paths, restriction_inspect, "post", "200", restriction_inspection);
    for (status, message) in [("401", "Current credential unavailable."), ("403", "Human Read credential required."), ("404", "Restriction target unavailable."), ("409", "Inspection source changed or is invalid; inspect again."), ("500", "Inspection storage failed.")] {
        set_error_response(&mut paths, restriction_inspect, "post", status, message);
    }
    for action in ["preview", "apply"] {
        let path = format!("/spaces/{{space_id}}/documents/{{doc_id}}/restrictions/{action}");
        add_path(&mut paths, &path, [("post", "Documents", if action == "preview" { "Preview page restriction consequences" } else { "Confirm a current page restriction preview" }, true)]);
        set_string_path_parameters(&mut paths, &path, [("space_id", "Current space ID."), ("doc_id", "Stable page ID.")]);
        let proposal = json!({"type":"object","additionalProperties":false,"required":["mode","allowlist"],"properties":{"mode":{"type":"string","enum":["inherited","restricted"]},"allowlist":{"type":"object","additionalProperties":false,"required":["users","groups"],"properties":{"users":{"type":"array","uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":256}},"groups":{"type":"array","uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":256}}}}}});
        set_request_body(&mut paths, &path, "post", if action == "preview" { proposal } else { json!({"type":"object","additionalProperties":false,"required":["preview_token","confirm"],"properties":{"preview_token":{"type":"string","maxLength":65536},"confirm":{"type":"boolean","enum":[true]}}}) });
        let response = if action == "preview" {
            json!({"type":"object","additionalProperties":false,"required":["schema_version","space_id","document_id","preview_token","expires_at","scope_notice","readable_pages"],"properties":{"schema_version":{"type":"integer","enum":[1]},"space_id":{"type":"string"},"document_id":{"type":"string"},"preview_token":{"type":"string"},"expires_at":{"type":"string","format":"date-time"},"scope_notice":{"type":"string"},"readable_pages":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["document_id","public_link_configured","public_link_policy_before","public_link_policy_after"],"properties":{"document_id":{"type":"string"},"public_link_configured":{"type":"boolean"},"public_link_policy_before":{"type":"boolean"},"public_link_policy_after":{"type":"boolean"}}}}}})
        } else {
            json!({"type":"object","additionalProperties":false,"required":["schema_version","space_id","document_id","revision","mode","scope_notice"],"properties":{"schema_version":{"type":"integer","enum":[1]},"space_id":{"type":"string"},"document_id":{"type":"string"},"revision":{"type":"string"},"mode":{"type":"string","enum":["inherited","restricted"]},"scope_notice":{"type":"string"}}})
        };
        paths.get_mut(&path).unwrap()["post"]["description"] = json!(if action == "preview" { "Current human Read and actual ManageSpace with private editor participation. At most 100 total exact, unique scoped user/group IDs; inherited mode requires empty lists. Readable descendant IDs and anonymous-link policy flags only, with a constant scope notice. Token expires after ten minutes and binds original actor/key/session, proposal and current source. No grants or guest onboarding." } else { "Current human Write and actual ManageSpace with private editor participation. Requires confirm=true and the original current preview token. Reject stale source, changed actor/key/session, expiry or self-loss. Policy, allowlist and attributed audit commit atomically; matching allowlist entries grant no permissions. Body locks do not block management." });
        for status in ["201", "202", "204"] {
            paths.get_mut(&path).unwrap()["post"]["responses"].as_object_mut().unwrap().remove(status);
        }
        set_json_response(&mut paths, &path, "post", "200", response);
        paths.get_mut(&path).unwrap()["post"]["responses"]["422"] = json!({"description":"JSON structure rejected by the request extractor.","content":{"text/plain":{"schema":{"type":"string"}}}});
        for (status,message) in [("400","Semantically invalid proposal or confirmation."),("401","Current credential unavailable."),("403","Required credential scope unavailable."),("404","Target or recipient unavailable."),("409","Preview stale or self-access would be lost; inspect again."),("500","Storage or audit failed; no successful change is acknowledged.")] {
            set_error_response(&mut paths, &path, "post", status, message);
        }
        paths.get_mut(&path).unwrap()["post"]["responses"]["400"]["content"]["text/plain"] = json!({"schema":{"type":"string"}});
    }
    for (path, method) in [
        ("/moves/commits/{op_id}/preserved/{document_id}", "get"),
        (
            "/moves/commits/{op_id}/preserved/{document_id}/inspect",
            "post",
        ),
    ] {
        add_path(
            &mut paths,
            path,
            [(method, "Documents", "Inspect preserved move metadata", true)],
        );
        set_string_path_parameters(
            &mut paths,
            path,
            [
                ("op_id", "Sealed operation ID bound to this page."),
                ("document_id", "Stable page ID requiring current View."),
            ],
        );
        paths.get_mut(path).unwrap()[method]["description"] = json!("Read-only inactive metadata inspection requiring original current human Read and current page View. The requested page must be bound to sealed moved-page or incident-relation evidence; unrelated operations are unavailable. Related endpoints must remain visible. POST has no body and supplies fresh transport without downloaded GET fallback. No restoration or move authority is granted.");
        set_json_response(
            &mut paths,
            path,
            method,
            "200",
            json!({"$ref":"#/components/schemas/MovePreservedEvidence"}),
        );
        for (status, message) in [
            ("401", "Current credential unavailable."),
            ("403", "Human Read credential required."),
            ("404", "Bound page or operation unavailable."),
            (
                "409",
                "Preserved evidence is changed, invalid or unavailable; review required.",
            ),
        ] {
            set_error_response(&mut paths, path, method, status, message);
        }
    }
    let preserved_index = "/moves/documents/{document_id}/preserved/inspect";
    add_path(
        &mut paths,
        preserved_index,
        [(
            "post",
            "Documents",
            "List current page-bound preserved operations",
            true,
        )],
    );
    set_string_path_parameters(
        &mut paths,
        preserved_index,
        [("document_id", "Stable page ID requiring current View.")],
    );
    paths.get_mut(preserved_index).unwrap()["post"]["parameters"] = json!([{
        "name": "cursor", "in": "query", "required": false,
        "description": "An operation ID previously disclosed for this page. It must remain sealed and bound; unrelated or missing cursors are uniformly unavailable.",
        "schema": {"type": "string", "minLength": 1}
    }]);
    paths.get_mut(preserved_index).unwrap()["post"]["description"] = json!("Fresh read-only bodyless POST requiring original current human Read and actual page View. Lists up to twenty sealed page-bound operation references in descending created_at/op_id order. No totals, hidden counts, bodies, actor labels or restoration authority. Current page policy is rechecked after inventory SQL. Inspection is point-in-time, not continuous authorization.");
    set_json_response(
        &mut paths,
        preserved_index,
        "post",
        "200",
        json!({"$ref":"#/components/schemas/MovePreservedIndex"}),
    );
    for (status, message) in [
        ("401", "Current credential unavailable."),
        ("403", "Human Read credential required."),
        ("404", "Page or bound cursor unavailable."),
        ("409", "Bound evidence changed or needs review."),
    ] {
        set_error_response(&mut paths, preserved_index, "post", status, message);
    }
    let maintenance_path = "/spaces/{space_id}/documents/{doc_id}/maintenance";
    let maintenance_targets_path = "/spaces/{space_id}/documents/{doc_id}/maintenance/targets";
    add_path(
        &mut paths,
        maintenance_path,
        [(
            "patch",
            "Metadata",
            "Assign page maintenance responsibility",
            true,
        )],
    );
    add_path(
        &mut paths,
        maintenance_targets_path,
        [(
            "get",
            "Metadata",
            "List current readable responsibility targets",
            true,
        )],
    );
    for path in [maintenance_path, maintenance_targets_path] {
        set_string_path_parameters(
            &mut paths,
            path,
            [
                ("space_id", "Current space ID."),
                ("doc_id", "Stable page ID."),
            ],
        );
        for (status, message) in [
            ("401", "Current credential unavailable."),
            ("403", "Current credential or page access unavailable."),
            ("404", "Page unavailable."),
        ] {
            set_error_response(
                &mut paths,
                path,
                if path == maintenance_path {
                    "patch"
                } else {
                    "get"
                },
                status,
                message,
            );
        }
    }
    set_request_body(
        &mut paths,
        maintenance_path,
        "patch",
        json!({"$ref":"#/components/schemas/DocumentMaintenanceRequest"}),
    );
    set_json_response(
        &mut paths,
        maintenance_path,
        "patch",
        "200",
        json!({"$ref":"#/components/schemas/DocumentMaintenanceOutcome"}),
    );
    set_json_response(
        &mut paths,
        maintenance_targets_path,
        "get",
        "200",
        json!({"$ref":"#/components/schemas/DocumentMaintenanceTargets"}),
    );
    set_error_response(
        &mut paths,
        maintenance_path,
        "patch",
        "400",
        "Invalid explicit responsibility/source.",
    );
    set_error_response(
        &mut paths,
        maintenance_path,
        "patch",
        "409",
        "Target, source or operation changed; retain the original request.",
    );
    let identifier_path = "/spaces/{space_id}/documents/{doc_id}/identifier";
    add_path(
        &mut paths,
        identifier_path,
        [("patch", "Metadata", "Set readable page identifier", true)],
    );
    set_string_path_parameters(
        &mut paths,
        identifier_path,
        [
            ("space_id", "Current space ID."),
            ("doc_id", "Stable page ID."),
        ],
    );
    set_request_body(
        &mut paths,
        identifier_path,
        "patch",
        json!({"$ref":"#/components/schemas/DocumentIdentifierRequest"}),
    );
    set_json_response(
        &mut paths,
        identifier_path,
        "patch",
        "200",
        json!({"$ref":"#/components/schemas/DocumentIdentifierOutcome"}),
    );
    for (status, message) in [
        ("400", "Invalid identifier or explicit source."),
        ("401", "Current credential unavailable."),
        ("403", "Current credential or page cannot be edited."),
        ("404", "Page unavailable."),
        (
            "409",
            "Identifier reservation, source or operation conflict; retain the original request.",
        ),
    ] {
        set_error_response(&mut paths, identifier_path, "patch", status, message);
    }
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
    paths.get_mut("/spaces/{space_id}/documents/{doc_id}").unwrap()["get"]["responses"]["200"]["headers"]["Cache-Control"] = json!({"schema":{"type":"string","const":"no-store"}});
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
    let archive_path = "/spaces/{space_id}/documents/{doc_id}/archive";
    add_path(&mut paths, archive_path, [("post", "Documents", "Archive page", true)]);
    set_string_path_parameters(&mut paths, archive_path, [
        ("space_id", "Space ID that owns the page."),
        ("doc_id", "Opaque stable page ID to archive."),
    ]);
    set_empty_response(&mut paths, archive_path, "post", "204", "Page archived; children, metadata and stored bytes retained.");
    paths.get_mut(archive_path).unwrap()["post"]["description"] = json!("Archive an active or already archived nondeleted page; repeat requests refresh its archive timestamp. Requires current human Write and actual ArchiveContent, private editor participation and ancestor restrictions. Body locks do not bar archiving. Change only the target archive/update timestamps; preserve children, metadata, published-link flags and stored bytes. Anonymous access is suspended while archived. The transition and accurately attributed document.archive audit commit together after exact source/audit checks, final Archive policy and original credential reproof. Search cleanup is advisory and bound to the committed lifecycle stamp; new filesystem mirrors are deferred. Restoration remains a separate operation.");
    let archive_responses = paths.get_mut(archive_path).unwrap()["post"]["responses"].as_object_mut().unwrap();
    for status in ["200", "201", "202"] { archive_responses.remove(status); }
    for (status, message) in [
        ("401", "Current human credential unavailable."),
        ("403", "Current credential or page cannot be archived."),
        ("404", "Page unavailable or trashed."),
        ("409", "Archive source or audit changed; retry."),
        ("500", "Archive transition could not be confirmed; files retained."),
    ] { set_error_response(&mut paths, archive_path, "post", status, message); }
    let unarchive_path = "/spaces/{space_id}/documents/{doc_id}/archive";
    paths.get_mut(unarchive_path).unwrap().as_object_mut().unwrap().insert("delete".into(), operation("Documents", "Unarchive page", true));
    set_string_path_parameters(&mut paths, unarchive_path, [
        ("space_id", "Space ID that owns the page."),
        ("doc_id", "Opaque stable archived page ID."),
    ]);
    set_empty_response(&mut paths, unarchive_path, "delete", "204", "Page unarchived; children, metadata and stored bytes retained.");
    paths.get_mut(unarchive_path).unwrap()["delete"]["description"] = json!("Unarchive only an archived nondeleted page. Requires current human Write and ArchiveContent, private editor participation and ancestor restrictions; body locks do not bar unarchiving. Change only the target archive/update timestamps; preserve children, metadata, publication/link flags and stored bytes. A retained public link may become readable again. The transition and accurately attributed document.unarchive audit commit together after exact source/audit checks, actual active outcome policy and original credential reproof. Unsafe search reindexing and mirror removal are deferred; a restored page may remain absent from search until derived storage is safely repaired. Already active, trashed, missing or foreign targets are unavailable.");
    let unarchive_responses = paths.get_mut(unarchive_path).unwrap()["delete"]["responses"].as_object_mut().unwrap();
    for status in ["200", "201", "202"] { unarchive_responses.remove(status); }
    for (status, message) in [
        ("401", "Current human credential unavailable."),
        ("403", "Current credential or page cannot be unarchived."),
        ("404", "Archived page unavailable."),
        ("409", "Unarchive source or audit changed; retry."),
        ("500", "Unarchive transition could not be confirmed; files retained."),
    ] { set_error_response(&mut paths, unarchive_path, "delete", status, message); }
    let soft_trash_path = "/spaces/{space_id}/documents/{doc_id}";
    paths.get_mut(soft_trash_path).unwrap()["delete"]["description"] = json!("Trash an active or archived page in the routed space. Requires current human Write and DeleteContent, private editor participation and ancestor restrictions. Body locks do not bar trashing. Preserve child pages, metadata, published-link flags and stored bytes; suspend anonymous access while trashed. The lifecycle transition and accurately attributed audit commit together after source and current authority checks. Search cleanup is advisory; new filesystem lifecycle mirrors are deferred. This is not permanent deletion or physical erasure.");
    let soft_trash_responses = paths.get_mut(soft_trash_path).unwrap()["delete"]["responses"].as_object_mut().unwrap();
    for status in ["200", "201", "202"] { soft_trash_responses.remove(status); }
    for (status, message) in [
        ("401", "Current human credential unavailable."),
        ("403", "Current credential or page cannot be trashed."),
        ("404", "Page unavailable or already trashed."),
        ("409", "Lifecycle source changed; retry without discarding local work."),
        ("500", "Transition could not be confirmed; files retained."),
    ] {
        set_error_response(&mut paths, soft_trash_path, "delete", status, message);
    }
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
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/move",
        "post",
        "409",
        "Cross-space relocation requires a reviewed metadata and access preview; no changes are made.",
    );
    add_path(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/content",
        [
            ("get", "Documents", "Read document content", true),
            (
                "put",
                "Documents",
                "Legacy direct content update (use a document draft)",
                true,
            ),
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
        "409",
        "Direct content writes are rejected; create or update a document draft.",
    );
    set_error_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/content",
        "put",
        "409",
        "Direct content writes are rejected; create or update a document draft.",
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
        [(
            "post",
            "Documents",
            "Enable public link for published page",
            true,
        )],
    );
    set_string_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/publish",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "doc_id",
                "Published page ID belonging to the requested space.",
            ),
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
        [("post", "Documents", "Disable public page link", true)],
    );
    set_string_path_parameters(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unpublish",
        [
            ("space_id", "Space ID that owns the document."),
            (
                "doc_id",
                "Page ID whose anonymous public link should be disabled.",
            ),
        ],
    );
    set_empty_response(
        &mut paths,
        "/spaces/{space_id}/documents/{doc_id}/unpublish",
        "post",
        "204",
        "Public link disabled; published revision and drafts are unchanged.",
    );
    for (suffix, success) in [("publish", "200"), ("unpublish", "204")] {
        let path = format!("/spaces/{{space_id}}/documents/{{doc_id}}/{suffix}");
        paths.get_mut(&path).unwrap()["post"]["description"] = json!("Manage only an anonymous public link, separately from publishing draft changes or granting people access. Requires current human Write and actual page EditContent, private editor participation and ancestor restrictions; body locks do not bar link management. Enable requires an active published page and the deployment switch on. Disable remains available for archived pages and when public pages are disabled. Scoped link changes and accurately attributed fixed-action audit commit together after exact source/audit checks, final page policy and original credential reproof. Audit-file delivery is advisory after confirmed commit. No publication file or replay receipt is created.");
        let responses = paths.get_mut(&path).unwrap()["post"]["responses"].as_object_mut().unwrap();
        for status in ["200", "201", "202", "204"] {
            if status != success { responses.remove(status); }
        }
        for (status, message) in [
            ("401", "Current human credential unavailable."),
            ("403", "Current credential or page cannot manage this link."),
            ("404", "Scoped page unavailable."),
            ("409", "Public link source or audit changed; retry."),
            ("500", "Link change could not be confirmed; no success is acknowledged."),
        ] { set_error_response(&mut paths, &path, "post", status, message); }
        if suffix == "publish" {
            set_error_response(&mut paths, &path, "post", "400", "Public pages are disabled.");
        }
    }
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
    let history_list="/spaces/{space_id}/documents/{doc_id}/publication-snapshots";
    let history_get="/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}";
    let history_raw="/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/raw";
    for (path,title) in [(history_list,"List typed publication snapshots"),(history_get,"Inspect a typed publication snapshot"),(history_raw,"Download exact captured publication bytes")] {
        add_path(&mut paths,path,[("get","Documents",title,true)]);
        set_uuid_path_parameters(&mut paths,path,[("space_id","Current space owning the page."),("doc_id","Current page independently authorized for ordinary Edit.")]);
        if path!=history_list {paths.get_mut(path).unwrap()["parameters"].as_array_mut().unwrap().push(json!({"name":"snapshot_id","in":"path","required":true,"description":"Exact original capture UUID plus :publication or :baseline; never a bare legacy version UUID.","schema":{"type":"string"}}));}
        paths.get_mut(path).unwrap()["get"]["description"]=json!("Current ordinary page Edit authority and current human credential are required before and after exact page/snapshot/receipt inventory checks; custom EditContent without View remains eligible. API keys need read plus write (or admin) scope. Published View, direct viewers and guests gain no historical content access. Reads verify complete native capture/receipt/custody bindings, never retrofit old successful receipts, mutate legacy versions or restore page/property/audience/draft state. Asset observations are reference-only and require current download authority/existence. Missing/corrupt v2 capture groups remain held. Serving-path retirement does not discard validated captured raw bytes.");
        for(status,description)in [("401","Current original identity unavailable."),("403","Current ordinary Edit unavailable."),("404","Current page or exact snapshot unavailable."),("409","Snapshot/source/custody evidence changed or requires review.")] {set_error_response(&mut paths,path,"get",status,description);}
    }
    set_json_response(&mut paths,history_list,"get","200",json!({"$ref":"#/components/schemas/PublicationSnapshotList"}));
    set_json_response(&mut paths,history_get,"get","200",json!({"$ref":"#/components/schemas/PublicationSnapshotGet"}));
    paths.get_mut(history_raw).unwrap()["get"]["responses"]["200"]=json!({"description":"Exact captured bytes; raw_only remains usable without guessed portable extraction.","content":{"application/octet-stream":{"schema":{"type":"string","format":"binary"}}},"headers":{
        "Content-Type":{"schema":{"type":"string","const":"application/octet-stream"}},"Content-Disposition":{"schema":{"type":"string","const":"attachment; filename=\"publication-snapshot.md\""}},"Content-Length":{"schema":{"type":"integer","minimum":0,"maximum":6000000}},"X-Starlane-Snapshot-Id":{"schema":{"type":"string"}},"X-Starlane-Snapshot-Sha256":{"schema":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"X-Content-Type-Options":{"schema":{"type":"string","const":"nosniff"}}}});
    for path in [history_list,history_get,history_raw] {paths.get_mut(path).unwrap()["get"]["responses"]["200"]["headers"]["Cache-Control"]=json!({"schema":{"type":"string","const":"no-store"}});}

    let recovery_source = "/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/recovery-source";
    let recovery_apply = "/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/recover";
    let recovery_content = json!({"type":"object","additionalProperties":false,"required":["title","content","icon","cover_image"],"properties":{
        "title":{"type":"string","maxLength":500},"content":{"type":"string","description":"Exact portable captured Markdown within existing draft byte bounds."},"icon":{"type":["string","null"]},"cover_image":{"type":["string","null"]}}});
    let recovery_digest = json!({"type":"string","pattern":"^[0-9a-f]{64}$"});
    add_path(&mut paths,recovery_source,[("get","Documents","Review a published snapshot for private draft recovery",true)]);
    add_path(&mut paths,recovery_apply,[("post","Documents","Recover the reviewed snapshot into a new private draft",true)]);
    for (path,method) in [(recovery_source,"get"),(recovery_apply,"post")] {
        set_string_path_parameters(&mut paths,path,[("space_id","Current route space ID."),("doc_id","Current document ID."),("snapshot_id","Exact typed snapshot ID; not a UUID.")]);
        for status in ["201","202","204"] {paths.get_mut(path).unwrap()[method]["responses"].as_object_mut().unwrap().remove(status);}
        for (status,description) in [("400","Unsupported recovery source or malformed request."),("401","Original current human identity unavailable."),("403","Current ordinary page Edit unavailable."),("404","Current page or selected snapshot unavailable."),("409","Reviewed source changed, original receipt needs review, or commit outcome is unknown."),("422","Strict request JSON is invalid.")] {set_error_response(&mut paths,path,method,status,description);}
    }
    paths.get_mut(recovery_source).unwrap()["get"]["description"] = json!("Read-only exact recovery review under current ordinary page Edit and current human write identity, including applicable locks and restrictions. No unrelated View prerequisite or current published body projection. Bind complete snapshot provenance and current page row/bytes through separate source digests. Raw-only or out-of-bounds captures are explicit unavailable sources, retaining guarded raw download. Properties and asset observations are excluded from recovery.");
    set_json_response(&mut paths,recovery_source,"get","200",json!({"type":"object","additionalProperties":false,"required":["schema_version","space_id","document_id","snapshot_id","snapshot_digest","snapshot_source_digest","page_source_digest","page_published","page_updated_at","recoverable","reason","extraction","draft"],"properties":{
        "schema_version":{"type":"integer","const":1},"space_id":{"type":"string"},"document_id":{"type":"string"},"snapshot_id":{"type":"string"},"snapshot_digest":recovery_digest,"snapshot_source_digest":recovery_digest,"page_source_digest":recovery_digest,"page_published":{"type":"boolean"},"page_updated_at":{"type":"string"},"recoverable":{"type":"boolean"},"reason":{"type":["string","null"],"enum":[null,"raw_only","title_bounds","content_bounds"]},"extraction":{"type":"string","enum":["exact_published_draft","proven_generated_header","raw_only"]},"draft":{"oneOf":[recovery_content,{"type":"null"}]}}}));
    paths.get_mut(recovery_source).unwrap()["get"]["responses"]["200"]["headers"]["Cache-Control"] = json!({"schema":{"type":"string","const":"no-store"}});
    set_request_body(&mut paths,recovery_apply,"post",json!({"type":"object","additionalProperties":false,"required":["schema_version","operation_id","snapshot_source_digest","page_source_digest"],"properties":{
        "schema_version":{"type":"integer","const":1},"operation_id":{"type":"string","format":"uuid"},"snapshot_source_digest":recovery_digest,"page_source_digest":recovery_digest}}));
    paths.get_mut(recovery_apply).unwrap()["post"]["description"] = json!("Confirm the original UUID and exact reviewed snapshot/page proofs. Atomically create one actor-owned private draft with captured title, portable body, icon and cover reference, current published base and provenance, plus attributed audit and durable original-operation receipt. Preserve published bytes, audience, properties, assets, draft shares and existing drafts. Current ordinary page Edit is proven before inserting ownership and again at commit; lock ownership does not grant authority. Exact original retry acknowledges the same unchanged owned result without duplicate creation or rebasing. Changed/deleted result stays held. Historical acknowledgement is not a current editing baseline. No publication or generic offline replay; original unknown outcomes remain retained for explicit exact retry. Only fresh reviewed digest mismatch after proving no original receipt and no retained operation audit returns recovery_rejected before any draft creation; it permits a new source review. Historical evidence damage and uncertain commit never receive that marker.");
    let recovery_result=json!({"type":"object","additionalProperties":false,"required":["schema_version","operation_id","space_id","document_id","snapshot_id","snapshot_source_digest","page_source_digest","draft","disposition"],"properties":{
        "schema_version":{"type":"integer","const":1},"operation_id":{"type":"string","format":"uuid"},"space_id":{"type":"string"},"document_id":{"type":"string"},"snapshot_id":{"type":"string"},"snapshot_source_digest":recovery_digest,"page_source_digest":recovery_digest,"draft":{"$ref":"#/components/schemas/DocumentDraft"},"disposition":{"type":"string","enum":["applied","historical"]}}});
    for status in ["200","201"] {set_json_response(&mut paths,recovery_apply,"post",status,recovery_result.clone());paths.get_mut(recovery_apply).unwrap()["post"]["responses"][status]["headers"]["Cache-Control"] = json!({"schema":{"type":"string","const":"no-store"}});}
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
    if let Some(operation) = paths
        .get_mut("/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore")
        .and_then(|item| item.get_mut("post"))
    {
        operation["description"] = json!("Content restoration is currently unavailable. After authentication, space-role and object checks, this endpoint returns an error without changing page content or creating a version.");
        if let Some(responses) = operation
            .get_mut("responses")
            .and_then(Value::as_object_mut)
        {
            responses.remove("200");
            responses.insert("400".into(), json!({ "description": "Version content restoration is unavailable, or the version does not belong to this document.", "content": error_content() }));
        }
    }
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
        "/spaces/{space_id}/attachments",
        [("get", "Documents", "List reusable space attachments", true)],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/spaces/{space_id}/attachments",
        [(
            "space_id",
            "Space ID whose reusable attachments are listed.",
        )],
    );
    set_json_response(
        &mut paths,
        "/spaces/{space_id}/attachments",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/DocumentAttachment" } }),
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
            { "name": "maintenance_user", "in": "query", "description": "Exact person responsibility ID, preserving letter case; current page access still applies.", "schema": { "type": "string" } },
            { "name": "maintenance_group", "in": "query", "description": "Exact team responsibility ID, preserving letter case; current page access still applies.", "schema": { "type": "string" } },
            { "name": "identifier", "in": "query", "description": "Exact readable page identifier, ignoring letter case; current page access still applies.", "schema": { "type": "string" } },
            { "name": "space_id", "in": "query", "schema": { "type": "string" } },
            { "name": "folder", "in": "query", "description": "Exact folder name or ID.", "schema": { "type": "string" } },
            { "name": "user", "in": "query", "description": "Exact last-updating user display name, email, or ID.", "schema": { "type": "string" } },
            { "name": "tag", "in": "query", "schema": { "type": "string" } },
            { "name": "property_name", "in": "query", "schema": { "type": "string" } },
            { "name": "property_value", "in": "query", "schema": { "type": "string" } },
            { "name": "has_attachment", "in": "query", "schema": { "type": "boolean" } },
            { "name": "has_cover", "in": "query", "schema": { "type": "boolean" } },
            { "name": "has_workflow", "in": "query", "description": "Whether a document has a workflow instance.", "schema": { "type": "boolean" } },
            { "name": "has_tag", "in": "query", "description": "Whether a document has at least one tag.", "schema": { "type": "boolean" } },
            { "name": "has_property", "in": "query", "description": "Whether a document has at least one stored property value.", "schema": { "type": "boolean" } },
            { "name": "is_locked", "in": "query", "description": "Whether a document is currently locked.", "schema": { "type": "boolean" } },
            { "name": "is_draft", "in": "query", "description": "Search only drafts owned by or explicitly shared with the authenticated user.", "schema": { "type": "boolean" } },
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
        "/webhooks/{webhook_id}/test",
        [(
            "post",
            "Webhooks",
            "Queue a synthetic test to the saved webhook endpoint",
            true,
        )],
    );
    set_uuid_path_parameters(
        &mut paths,
        "/webhooks/{webhook_id}/test",
        [("webhook_id", "Saved active webhook ID to test.")],
    );
    set_empty_response(
        &mut paths,
        "/webhooks/{webhook_id}/test",
        "post",
        "202",
        "Synthetic test queued; match delivery history test_id to the returned delivery_id for recorded attempts.",
    );
    set_json_response(
        &mut paths,
        "/webhooks/{webhook_id}/test",
        "post",
        "202",
        json!({ "$ref": "#/components/schemas/WebhookTestQueued" }),
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
        [(
            "post",
            "Notifications",
            "Record authenticated system activity",
            true,
        )],
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
    add_path(
        &mut paths,
        "/ai/providers",
        [("get", "AI", "List managed AI providers", true)],
    );
    add_path(
        &mut paths,
        "/ai/providers/{provider}",
        [
            ("put", "AI", "Create or update an AI provider", true),
            (
                "delete",
                "AI",
                "Remove application AI provider configuration",
                true,
            ),
        ],
    );
    add_path(
        &mut paths,
        "/ai/providers/{provider}/test",
        [(
            "post",
            "AI",
            "Test provider connection and load models",
            true,
        )],
    );
    add_path(
        &mut paths,
        "/ai/settings",
        [("put", "AI", "Update AI availability or default route", true)],
    );
    set_string_path_parameters(
        &mut paths,
        "/ai/providers/{provider}",
        [("provider", "AI provider identifier")],
    );
    set_string_path_parameters(
        &mut paths,
        "/ai/providers/{provider}/test",
        [("provider", "AI provider identifier")],
    );
    set_json_response(
        &mut paths,
        "/ai/config",
        "get",
        "200",
        json!({ "$ref": "#/components/schemas/AiConfig" }),
    );
    set_json_response(
        &mut paths,
        "/ai/providers",
        "get",
        "200",
        json!({ "type": "array", "items": { "$ref": "#/components/schemas/AiProvider" } }),
    );
    set_request_body(
        &mut paths,
        "/ai/providers/{provider}",
        "put",
        json!({ "$ref": "#/components/schemas/AiProviderSaveRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/ai/providers/{provider}",
        "put",
        "204",
        "AI provider saved.",
    );
    set_empty_response(
        &mut paths,
        "/ai/providers/{provider}",
        "delete",
        "204",
        "Application AI provider configuration removed.",
    );
    set_json_response(
        &mut paths,
        "/ai/providers/{provider}/test",
        "post",
        "200",
        json!({ "$ref": "#/components/schemas/AiProviderTestResponse" }),
    );
    set_request_body(
        &mut paths,
        "/ai/settings",
        "put",
        json!({ "$ref": "#/components/schemas/AiSettingsUpdateRequest" }),
    );
    set_empty_response(
        &mut paths,
        "/ai/settings",
        "put",
        "204",
        "AI settings updated.",
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

    for path in [
        "/spaces/{space_id}/documents/{doc_id}",
        "/spaces/{space_id}/documents/{doc_id}/content",
        "/spaces/{space_id}/documents/{doc_id}/attachments",
    ] {
        if let Some(operation) = paths.get_mut(path).and_then(|entry| entry.get_mut("get")) {
            let previous = operation["description"].as_str().unwrap_or("");
            operation["description"] = json!(format!("{previous} Active published pages may also admit a current direct ordinary-user page viewer grant through named published View. This grants no draft, ancestor, whole-space catalog or mutation access. Grant-only metadata carries page_access_source=direct_page_grant and omits hidden placement and editor identity labels; page-owned attachment reads retain original credential, policy, source and file proofs."));
        }
    }
    if let Some(operation) = paths.get_mut("/search").and_then(|entry| entry.get_mut("get")) {
        let previous = operation["description"].as_str().unwrap_or("");
        operation["description"] = json!(format!("{previous} Direct page-viewer-only results are limited to literal published title/body matching with hidden space/folder/author labels suppressed. Catalog filters and advanced query syntax do not expose grant-only results. Dataview and draft search retain ordinary authority."));
    }
    attachment_audience_paths(&mut paths);
    Value::Object(paths)
}

fn attachment_audience_paths(paths:&mut serde_json::Map<String,Value>){
    let root="/spaces/{space_id}/documents/{doc_id}";
    let mut version_properties=serde_json::Map::new();
    for name in ["version_id","logical_attachment_id","original_space_id","original_document_id","uploader_id","original_name","mime_type","binary_sha256","original_created_at","original_storage_path","original_filename","upload_operation_id","upload_attempt_id","raw_receipt_sha256","intent_sha256","install_proof_sha256","provenance"]{version_properties.insert(name.into(),json!({"type":"string"}));}
    version_properties.insert("version_number".into(),json!({"type":"integer","minimum":1}));
    version_properties.insert("byte_length".into(),json!({"type":"integer","minimum":0}));
    let version_required=version_properties.keys().cloned().collect::<Vec<_>>();
    let version=json!({"type":"object","additionalProperties":false,"properties":version_properties,"required":version_required});
    let mut legacy_properties=serde_json::Map::new();
    for name in ["asset_id","space_id","document_id","original_name","mime_type","created_at","storage_path","binary_sha256"] {legacy_properties.insert(name.into(),json!({"type":"string"}));}
    legacy_properties.insert("kind".into(),json!({"const":"legacy_snapshot_v1"}));legacy_properties.insert("size_bytes".into(),json!({"type":"integer","minimum":0}));
    let legacy_required=legacy_properties.keys().cloned().collect::<Vec<_>>();
    let legacy=json!({"type":"object","additionalProperties":false,"properties":legacy_properties,"required":legacy_required});
    let draft_header=json!({"name":"x-starlane-upload-draft","in":"header","required":false,"description":"Original selected draft UUID for newly scoped uploads, replacements and private retirement. Omission retains explicit legacy page-owned upload semantics; never fill from a later selection during retry.","schema":{"type":"string","format":"uuid"}});
    let upload=format!("{root}/attachments");
    paths.get_mut(&upload).unwrap()["post"]["description"]=json!("Durable multipart upload. Retain the canonical operation UUID and exact original name, MIME, bytes and draft context. New draft uploads require current selected Draft Edit before admission, commit and acknowledgement. A completed upload is separate from insertion or publication; exact original retries do not adopt a later draft. Existing unscoped page-owned operations remain explicit compatibility behavior.");
    paths.get_mut(&upload).unwrap()["post"]["parameters"]=json!([draft_header.clone(),{"name":"x-starlane-upload-operation","in":"header","required":false,"schema":{"type":"string","format":"uuid"}}]);
    let deletion=format!("{root}/attachments/{{attachment_id}}");paths.get_mut(&deletion).unwrap()["delete"]["parameters"]=json!([draft_header.clone()]);
    let family=format!("{root}/attachment-families/{{logical_attachment_id}}/versions");
    add_path(paths,&family,[("get","Documents","Inspect authorized immutable attachment versions",true),("post","Documents","Replace the exact authorized attachment head",true)]);
    set_string_path_parameters(paths,&family,[("space_id","Current space"),("doc_id","Current page"),("logical_attachment_id","Logical attachment family")]);
    paths.get_mut(&family).unwrap()["get"]["description"]=json!("Schema2 returns only authorized versions and retirement entries. total is the visible version count, not the family revision. current_head is null when the actual current head is unavailable; otherwise its exact version_id/revision may be used for replacement. after retains immutable version ordinals; next_after never identifies an inaccessible-only suffix.");
    paths.get_mut(&family).unwrap()["get"]["parameters"]=json!([{"name":"after","in":"query","schema":{"type":"integer","minimum":0}},{"name":"limit","in":"query","schema":{"type":"integer","minimum":1,"maximum":100,"default":100}}]);
    set_json_response(paths,&family,"get","200",json!({"type":"object","required":["schema_version","current_head","versions","retained_versions","total","next_after"],"properties":{"schema_version":{"const":2},"current_head":{"oneOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["version_id","revision"],"properties":{"version_id":{"type":"string"},"revision":{"type":"integer","minimum":1}}}]},"versions":{"type":"array","items":version.clone()},"retained_versions":{"type":"object"},"total":{"type":"integer","minimum":0},"next_after":{"type":["integer","null"]}}}));
    paths.get_mut(&family).unwrap()["post"]["parameters"]=json!([draft_header,{"name":"x-starlane-upload-operation","in":"header","required":true,"schema":{"type":"string","format":"uuid"}},{"name":"x-starlane-attachment-version","in":"header","required":true,"schema":{"type":"string","format":"uuid"}},{"name":"x-starlane-attachment-revision","in":"header","required":true,"schema":{"type":"integer","minimum":1}}]);
    set_multipart_file_request_body(paths,&family,"post","Exact replacement file; a new scoped version starts in the selected draft audience and does not change a published predecessor.");
    set_json_response(paths,&family,"post","201",json!({"type":"object","required":["expected","version","origin"],"properties":{"expected":{"type":"object"},"version":version.clone(),"origin":{"type":"string"},"audience":{"type":"object"}}}));
    let exact=format!("{root}/attachment-versions/{{version_id}}");
    add_path(paths,&exact,[("get","Documents","Inspect one currently authorized exact attachment version",true)]);
    set_string_path_parameters(paths,&exact,[("space_id","Current space"),("doc_id","Current page"),("version_id","Exact immutable version")]);
    set_json_response(paths,&exact,"get","200",json!({"type":"object","properties":{"version":version,"logical_attachment_id":{"type":"string"},"current_space_id":{"type":"string"},"current_document_id":{"type":"string"},"current_storage_path":{"type":"string"},"retirement":{"type":["object","null"]}}}));
    let bytes=format!("{family}/{{version_id}}/file");add_path(paths,&bytes,[("get","Documents","Stream one authorized immutable attachment version",true)]);
    set_string_path_parameters(paths,&bytes,[("space_id","Current space"),("doc_id","Current page"),("logical_attachment_id","Logical attachment family"),("version_id","Exact immutable version")]);
    paths.get_mut(&bytes).unwrap()["get"]["responses"]["200"]=json!({"description":"Exact currently authorized immutable bytes","content":{"application/octet-stream":{"schema":{"type":"string","format":"binary"}}},"headers":{"Content-Length":{"schema":{"type":"integer","minimum":0}},"Cache-Control":{"schema":{"type":"string","const":"private, no-store"}}}});
    let retained=format!("{root}/attachment-retirements");add_path(paths,&retained,[("get","Documents","List authorized retained attachment decisions",true)]);set_string_path_parameters(paths,&retained,[("space_id","Current space"),("doc_id","Current page")]);set_json_response(paths,&retained,"get","200",json!({"type":"array","items":{"type":"object"}}));
    let reuse=format!("{root}/drafts/{{draft_id}}/attachment-bindings");add_path(paths,&reuse,[("post","Documents","Review exact-version reuse into another draft on this page",true)]);set_string_path_parameters(paths,&reuse,[("space_id","Current space"),("doc_id","Page"),("draft_id","Original editable target draft")]);
    set_request_body(paths,&reuse,"post",json!({"type":"object","additionalProperties":false,"required":["schema_version","operation_id","version_id"],"properties":{"schema_version":{"const":1},"operation_id":{"type":"string","format":"uuid"},"version_id":{"type":"string","format":"uuid"}}}));
    paths.get_mut(&reuse).unwrap()["post"]["description"]=json!("Requires current source-version read and target Draft Edit. Records one immutable original-operation binding; pasted identifiers alone grant no access. Exact original retries retain the same target. Cross-page reuse uses the copy endpoint instead.");
    set_json_response(paths,&reuse,"post","200",json!({"type":"object","required":["schema_version","operation_id","version_id","draft_id","draft_created_at","binding_digest"]}));
    let review=format!("{root}/attachment-copy-source/{{asset_id}}");add_path(paths,&review,[("get","Documents","Review the current snapshot of an unregistered legacy file",true)]);set_string_path_parameters(paths,&review,[("space_id","Current space"),("doc_id","Original source page"),("asset_id","Original legacy asset ID")]);set_json_response(paths,&review,"get","200",legacy.clone());
    let copy=format!("{root}/drafts/{{draft_id}}/attachments/copy");add_path(paths,&copy,[("post","Documents","Copy an authorized exact source into a private target draft",true)]);set_string_path_parameters(paths,&copy,[("space_id","Current source and target space"),("doc_id","Target page"),("draft_id","Original target draft")]);
    set_request_body(paths,&copy,"post",json!({"type":"object","additionalProperties":false,"required":["schema_version","operation_id","source_document_id","version_id"],"properties":{"schema_version":{"const":1},"operation_id":{"type":"string","format":"uuid"},"source_document_id":{"type":"string"},"version_id":{"type":"string"},"expected_source_version":{"type":"object"},"expected_legacy_source":legacy},"oneOf":[{"required":["expected_source_version"],"not":{"required":["expected_legacy_source"]}},{"required":["expected_legacy_source"],"not":{"required":["expected_source_version"]}}]}));
    paths.get_mut(&copy).unwrap()["post"]["description"]=json!("Streams the reviewed registered version or explicitly labeled legacy snapshot into independently owned target custody. No source association or audience is changed. Current source read and original target Draft Edit are rechecked before commit. Retain the complete request before dispatch; a completed own retry verifies its original target receipt without requiring a later source grant. A legacy snapshot is not fabricated immutable history.");
    set_json_response(paths,&copy,"post","201",json!({"$ref":"#/components/schemas/DocumentAttachment"}));
    for path in [&family,&exact,&bytes,&retained,&reuse,&review,&copy] {for method in ["get","post"] {if paths.get(path).and_then(|v|v.get(method)).is_some(){set_error_response(paths,path,method,"404","Original source or current authorized audience unavailable.");set_error_response(paths,path,method,"409","Original operation, source, head, draft or retained evidence changed; preserve the original request for review.");}}}
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

fn set_source_digest_query_parameter(
    paths: &mut serde_json::Map<String, Value>,
    path: &str,
    method: &str,
) {
    if let Some(operation) = paths
        .get_mut(path)
        .and_then(|item| item.get_mut(method))
        .and_then(Value::as_object_mut)
    {
        operation.insert(
            "parameters".into(),
            json!([{
                "name": "source_digest",
                "in": "query",
                "required": true,
                "description": "Exact current selected-draft source digest.",
                "schema": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
            }]),
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
    #[test]
    fn original_trash_timestamp_is_optional_raw_transport_not_authority() {
        let spec=openapi_spec();let doc=&spec["components"]["schemas"]["Document"];
        assert_eq!(doc["properties"]["deleted_at"]["type"],"string");
        assert!(!doc["required"].as_array().unwrap().iter().any(|v|v=="deleted_at"));
        let description=doc["properties"]["deleted_at"]["description"].as_str().unwrap();
        assert!(description.contains("byte-for-byte")&&description.contains("not deletion authority"));
        assert_eq!(doc["properties"]["created_at"]["type"],"string");
    }

    use super::openapi_spec;
    use serde_json::json;

    #[test]
    fn direct_page_viewer_management_routes_bind_exact_review_and_confirmation() {
        let spec = openapi_spec();
        for (stage, response) in [("inspect", "PageViewerGrantInspection"), ("review", "PageViewerGrantReview"), ("confirm", "PageViewerGrantOutcome")] {
            let path = format!("/spaces/{{space_id}}/documents/{{doc_id}}/viewer-grants/{stage}");
            let op = &spec["paths"][&path]["post"];
            assert!(!op["security"].as_array().unwrap().is_empty());
            assert_eq!(op["responses"]["200"]["content"]["application/json"]["schema"]["$ref"], format!("#/components/schemas/{response}"));
            for status in ["201", "202", "204"] { assert!(op["responses"].get(status).is_none()); }
            for status in ["401", "403", "404", "409", "500"] { assert!(op["responses"].get(status).is_some()); }
            if stage == "inspect" { assert!(op.get("requestBody").is_none()); }
            else {
                assert_eq!(op["requestBody"]["content"]["application/json"]["schema"]["$ref"], if stage == "review" { "#/components/schemas/PageViewerGrantProposal" } else { "#/components/schemas/PageViewerGrantConfirmation" });
                assert_eq!(op["responses"]["422"]["content"]["text/plain"]["schema"]["type"], "string");
            }
        }
        let schemas = &spec["components"]["schemas"];
        assert_eq!(schemas["PageViewerGrantProposal"]["properties"]["action"]["enum"], json!(["grant", "revoke"]));
        assert_eq!(schemas["PageViewerGrantConfirmation"]["properties"]["confirm"]["enum"], json!([true]));
        assert_eq!(schemas["PageViewerGrantConfirmation"]["properties"]["source_digest"]["pattern"], "^[0-9a-f]{64}$");
        assert_eq!(schemas["PageViewerGrant"]["properties"]["role"]["enum"], json!(["viewer"]));
        assert_eq!(schemas["PageViewerGrant"]["properties"]["scope"]["enum"], json!(["page"]));
        assert_eq!(schemas["PageViewerGrantInspection"]["properties"]["grants"]["items"]["properties"]["display_name"]["type"], json!(["string", "null"]));
    }

    #[test]
    fn shared_page_discovery_has_only_visible_keyset_presentation_and_read_contract() {
        let spec = openapi_spec();
        let op = &spec["paths"]["/shared-pages/inspect"]["post"];
        assert!(!op["security"].as_array().unwrap().is_empty());
        assert_eq!(op["responses"]["200"]["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/SharedPageInspection");
        assert!(op["description"].as_str().unwrap().contains("Read-only personal keys"));
        for status in ["201", "202", "204"] { assert!(op["responses"].get(status).is_none()); }
        let schemas = &spec["components"]["schemas"];
        let response = &schemas["SharedPageInspection"];
        assert_eq!(response["required"], json!(["pages", "next_cursor"]));
        assert_eq!(response["properties"].as_object().unwrap().len(), 2);
        assert_eq!(response["properties"]["pages"]["maxItems"], 20);
        let item = &response["properties"]["pages"]["items"];
        assert_eq!(item["required"], json!(["id", "space_id", "title", "published_at", "updated_at", "human_identifier", "icon"]));
        assert_eq!(item["properties"].as_object().unwrap().len(), 7);
        assert_eq!(item["additionalProperties"], false);
        for key in ["space_name", "folder_id", "folder_name", "ancestor_ids", "author", "total", "content"] { assert!(item["properties"].get(key).is_none()); }
        assert_eq!(schemas["SharedPageInspectionRequest"]["required"], json!([]));
        assert_eq!(schemas["SharedPageInspectionRequest"]["properties"]["cursor"]["maxLength"], 8192);
    }

    #[test]
    fn page_viewer_provenance_is_separate_from_space_authority_and_guest_availability() {
        let spec = openapi_spec();
        let schemas = &spec["components"]["schemas"];
        let document = &schemas["Document"];
        assert_eq!(document["properties"]["page_access_source"]["enum"], json!(["direct_page_grant"]));
        assert!(!document["required"].as_array().unwrap().contains(&json!("page_access_source")));
        for field in ["created_by", "last_updated_by"] { assert_eq!(document["properties"][field]["oneOf"][1]["enum"], json!([""])); }
        let report = &schemas["PageAccessInspection"];
        for field in ["page_permissions", "page_permission_sources", "space_permission_sources"] { assert!(report["required"].as_array().unwrap().contains(&json!(field))); }
        let source = &report["properties"]["page_permission_sources"]["items"];
        assert_eq!(source["properties"]["kind"]["enum"], json!(["direct_page_viewer"]));
        assert_eq!(source["properties"]["permissions"]["items"]["enum"], json!(["view"]));
        assert_eq!(source["required"], json!(["kind", "grant_id", "revision", "permissions"]));
        let sharing = &report["properties"]["sharing"]["properties"];
        assert_eq!(sharing["people_grants_available"]["type"], "boolean");
        assert!(sharing["people_grants_available"].get("enum").is_none());
        assert_eq!(sharing["guests_available"]["type"], "boolean");
        assert!(sharing["guests_available"].get("enum").is_none());
    }

    #[test]
    fn draft_save_contract_requires_original_digest_and_create_source_is_optional() {
        let spec = openapi_spec();
        let schemas = &spec["components"]["schemas"];
        for name in ["DocumentDraft", "DocumentDraftUpdateRequest"] {
            assert_eq!(schemas[name]["properties"]["source_digest"]["type"], "string");
            assert_eq!(schemas[name]["properties"]["source_digest"]["pattern"], "^[0-9a-f]{64}$");
            assert!(schemas[name]["required"].as_array().unwrap().contains(&json!("source_digest")));
        }
        assert_eq!(schemas["Document"]["properties"]["draft_source_digest"]["pattern"], "^[0-9a-f]{64}$");
        assert!(!schemas["Document"]["required"].as_array().unwrap().contains(&json!("draft_source_digest")));
        assert!(schemas["Document"]["properties"]["draft_source_digest"]["description"].as_str().unwrap().contains("only with document creation"));
        assert!(schemas["DocumentDraftSummary"]["properties"]["source_digest"].is_null());
        let put = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}"]["put"];
        assert_eq!(put["requestBody"]["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/DocumentDraftUpdateRequest");
        assert_eq!(put["responses"]["200"]["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/DocumentDraft");
        let conflict = put["responses"]["409"]["description"].as_str().unwrap();
        assert!(conflict.contains("source_required") && conflict.contains("source_conflict") && conflict.contains("do not rebase"));
    }
    #[test]
    fn public_link_contract_distinguishes_fixed_audit_and_exact_success_shapes() {
        let spec = openapi_spec();
        for (suffix, success) in [("publish", "200"), ("unpublish", "204")] {
            let path = format!("/spaces/{{space_id}}/documents/{{doc_id}}/{suffix}");
            let item = &spec["paths"][&path];
            let op = &item["post"];
            for name in ["space_id", "doc_id"] {
                let parameter = item["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap();
                assert_eq!(parameter["required"], true);
                assert_eq!(parameter["schema"]["type"], "string");
                assert!(parameter["schema"].get("format").is_none(), "stable legacy IDs remain opaque");
            }
            assert!(op["security"].is_array());
            assert!(op.get("requestBody").is_none());
            assert!(op["responses"][success].is_object());
            for status in ["200", "201", "202", "204"] {
                if status != success { assert!(op["responses"].get(status).is_none()); }
            }
            for status in ["401", "403", "404", "409", "500"] { assert!(op["responses"][status].is_object()); }
            let description = op["description"].as_str().unwrap();
            assert!(description.contains("audit commit together"));
            assert!(description.contains("original credential reproof"));
            assert!(description.contains("separately from publishing draft changes"));
        }
        let enabled = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/publish"]["post"];
        assert_eq!(enabled["responses"]["200"]["content"]["application/json"]["schema"]["$ref"], "#/components/schemas/PublishDocumentResponse");
        assert!(enabled["responses"]["400"].is_object());
        let disabled = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/unpublish"]["post"];
        assert!(disabled["responses"]["204"].get("content").is_none());
    }

    #[test]
    fn unarchive_has_exact_empty_success_and_active_outcome_contract() {
        let spec = openapi_spec();
        let item = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/archive"];
        let op = &item["delete"];
        assert!(spec["paths"].get("/spaces/{space_id}/documents/{doc_id}/unarchive").is_none());
        assert!(item["post"]["responses"]["204"].is_object(), "Archive POST remains on the same path");
        assert!(op["security"].is_array());
        assert!(op.get("requestBody").is_none());
        for name in ["space_id", "doc_id"] {
            let p = item["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap();
            assert_eq!(p["required"], true);
            assert_eq!(p["schema"]["type"], "string");
            assert!(p["schema"].get("format").is_none());
        }
        assert!(op["responses"]["204"].is_object());
        assert!(op["responses"]["204"].get("content").is_none());
        for status in ["200", "201", "202"] { assert!(op["responses"].get(status).is_none()); }
        for status in ["401", "403", "404", "409", "500"] { assert!(op["responses"][status].is_object()); }
        let description = op["description"].as_str().unwrap();
        for boundary in ["ArchiveContent", "active outcome", "audit commit together", "reindexing and mirror removal are deferred", "public link may become readable"] {
            assert!(description.contains(boundary));
        }
    }

    #[test]
    fn archive_has_exact_empty_success_and_actual_permission_contract() {
        let spec = openapi_spec();
        let item = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/archive"];
        let op = &item["post"];
        assert!(op["security"].is_array());
        assert!(op.get("requestBody").is_none());
        for name in ["space_id", "doc_id"] {
            let p = item["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap();
            assert_eq!(p["required"], true);
            assert_eq!(p["schema"]["type"], "string");
            assert!(p["schema"].get("format").is_none());
        }
        assert!(op["responses"]["204"].is_object());
        assert!(op["responses"]["204"].get("content").is_none());
        for status in ["200", "201", "202"] { assert!(op["responses"].get(status).is_none()); }
        for status in ["401", "403", "404", "409", "500"] { assert!(op["responses"][status].is_object()); }
        let description = op["description"].as_str().unwrap();
        for boundary in ["ArchiveContent", "already archived", "preserve children", "audit commit together", "mirrors are deferred"] {
            assert!(description.contains(boundary));
        }
    }

    #[test]
    fn soft_trash_has_empty_success_and_preservation_contract() {
        let spec = openapi_spec();
        let op = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}"]["delete"];
        assert!(op["security"].is_array());
        assert!(op.get("requestBody").is_none());
        assert!(op["responses"]["204"].is_object());
        assert!(op["responses"]["204"].get("content").is_none());
        for status in ["200", "201", "202"] { assert!(op["responses"].get(status).is_none()); }
        for status in ["401", "403", "404", "409", "500"] { assert!(op["responses"][status].is_object()); }
        let description = op["description"].as_str().unwrap();
        for boundary in ["DeleteContent", "Preserve child pages", "audit commit together", "mirrors are deferred", "not permanent deletion"] {
            assert!(description.contains(boundary));
        }
    }

    #[test]
    fn trash_restore_has_exact_empty_success_and_retained_archive_contract() {
        let spec = openapi_spec();
        let item = &spec["paths"]["/spaces/{space_id}/documents/{document_id}/trash"];
        let op = &item["post"];
        assert!(spec["paths"].get("/spaces/{space_id}/documents/{doc_id}/trash").is_none());
        assert!(item["delete"]["responses"]["204"].is_object(), "Permanent DELETE is preserved on the same path");
        assert!(op["security"].is_array());
        assert!(op.get("requestBody").is_none());
        for name in ["space_id", "document_id"] {
            let p = item["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap();
            assert_eq!(p["required"], true);
            assert_eq!(p["schema"]["type"], "string");
            assert!(p["schema"].get("format").is_none());
        }
        assert!(op["responses"]["204"].is_object());
        assert!(op["responses"]["204"].get("content").is_none());
        for status in ["200", "201", "202"] { assert!(op["responses"].get(status).is_none()); }
        for status in ["401", "403", "404", "409", "500"] { assert!(op["responses"][status].is_object()); }
        let description = op["description"].as_str().unwrap();
        for boundary in ["DeleteContent", "retained archive stamp", "nondeleted outcome", "audit commit together", "reindexing and mirror removal are deferred", "No expiry purge"] {
            assert!(description.contains(boundary));
        }
    }

    #[test]
    fn permanent_trash_delete_has_exact_empty_success_and_retained_file_contract() {
        let spec = openapi_spec();
        let item = &spec["paths"]["/spaces/{space_id}/documents/{document_id}/trash"];
        let op = &item["delete"];
        assert!(op["security"].is_array());
        assert_eq!(op["requestBody"]["required"],true);
        assert_eq!(op["requestBody"]["content"]["application/json"]["schema"]["$ref"],"#/components/schemas/OriginalPermanentDeleteRequest");
        let header=&op["parameters"][0];assert_eq!(header["name"],"x-starlane-delete-operation");assert_eq!(header["required"],true);
        assert!(op["responses"]["204"]["headers"]["x-starlane-delete-operation"].is_object());
        let target=&spec["components"]["schemas"]["OriginalTrashedDeleteTarget"];assert_eq!(target["required"],json!(["created_at","deleted_at"]));
        assert!(target["properties"]["deleted_at"].get("nullable").is_none());
        for name in ["space_id", "document_id"] {
            let parameter = item["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap();
            assert_eq!(parameter["required"], true);
            assert_eq!(parameter["schema"]["type"], "string");
            assert!(parameter["schema"].get("format").is_none(), "legacy stable IDs remain opaque");
        }
        assert!(op["responses"]["204"].is_object());
        assert!(op["responses"]["204"].get("content").is_none());
        for status in ["200", "201", "202"] { assert!(op["responses"].get(status).is_none()); }
        for status in ["401", "403", "404", "409", "500"] { assert!(op["responses"][status].is_object()); }
        let description = op["description"].as_str().unwrap();
        assert!(description.contains("Files are retained"));
        assert!(description.contains("No synthetic replay receipt"));
    }

    #[test]
    fn original_cleanup_is_separate_bounded_outcome_not_metadata_204() {
        let spec=openapi_spec();let op=&spec["paths"]["/spaces/{space_id}/documents/{document_id}/permanent-deletions/{operation_id}/cleanup"]["post"];
        assert_eq!(op["requestBody"]["required"],true);assert!(op["responses"]["200"].is_object());assert!(op["responses"].get("204").is_none());
        assert_eq!(spec["components"]["schemas"]["OriginalAttachmentCleanupOutcome"]["properties"]["outcome"]["enum"],json!(["pending","retained_review","physically_completed"]));
        let description=op["description"].as_str().unwrap();assert!(description.contains("current original human")&&description.contains("never grants unlink")&&description.contains("recreated same-ID"));
    }

    #[test]
    fn lifecycle_metadata_lists_have_secured_read_only_array_contracts() {
        let spec = openapi_spec();
        for suffix in ["archive", "trash"] {
            let path = format!("/spaces/{{space_id}}/documents/{suffix}");
            let item = &spec["paths"][&path];
            let operation = &item["get"];
            assert!(operation["security"].is_array());
            assert!(operation.get("requestBody").is_none());
            assert!(item.get("post").is_none());
            assert_eq!(item["parameters"][0]["name"], "space_id");
            let schema = &operation["responses"]["200"]["content"]["application/json"]["schema"];
            assert_eq!(schema["type"], "array");
            assert_eq!(schema["items"]["$ref"], "#/components/schemas/Document");
            for status in ["401", "403", "404", "409"] {
                assert!(operation["responses"][status].is_object());
            }
        }
    }

    #[test]
    fn preserved_operation_index_is_fresh_bounded_and_omits_global_audience_data() {
        let spec = openapi_spec();
        let operation = &spec["paths"]["/moves/documents/{document_id}/preserved/inspect"]["post"];
        assert!(operation["security"].is_array());
        let path_parameters =
            &spec["paths"]["/moves/documents/{document_id}/preserved/inspect"]["parameters"];
        assert!(path_parameters
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "document_id" && p["in"] == "path" && p["required"] == true));
        assert!(operation.get("requestBody").is_none());
        assert!(operation["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "cursor" && p["in"] == "query" && p["required"] == false));
        let schema = &spec["components"]["schemas"]["MovePreservedIndex"];
        assert_eq!(schema["properties"]["operations"]["maxItems"], 20);
        assert_eq!(schema["properties"]["next_cursor"]["nullable"], true);
        assert_eq!(schema["additionalProperties"], false);
        for field in [
            "actor_id",
            "total",
            "hidden_count",
            "source_space_id",
            "destination_space_id",
            "draft_body",
        ] {
            assert!(!schema.to_string().contains(field));
        }
    }

    #[test]
    fn preserved_move_inspection_is_secured_read_only_and_has_no_restore_contract() {
        let spec = openapi_spec();
        for (path, method) in [
            ("/moves/commits/{op_id}/preserved/{document_id}", "get"),
            (
                "/moves/commits/{op_id}/preserved/{document_id}/inspect",
                "post",
            ),
        ] {
            let operation = &spec["paths"][path][method];
            assert!(operation["security"].is_array());
            assert!(operation.get("requestBody").is_none());
            assert_eq!(
                operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
                "#/components/schemas/MovePreservedEvidence"
            );
            assert!(operation["responses"]["404"].is_object());
        }
        let schema = &spec["components"]["schemas"]["MovePreservedEvidence"];
        assert_eq!(schema["additionalProperties"], false);
        for field in ["editable", "restoration_available"] {
            assert_eq!(schema["properties"][field]["enum"], json!([false]));
        }
        assert_eq!(
            schema["properties"]["entries"]["items"]["additionalProperties"],
            false
        );
        for field in ["draft_body", "public_slug", "permissions", "participants"] {
            assert!(!schema.to_string().contains(field));
        }
    }

    #[test]
    fn restriction_inspection_keeps_local_unavailable_ids_without_secret_labels() {
        let spec = openapi_spec();
        let path = "/spaces/{space_id}/documents/{doc_id}/restrictions/inspect";
        let operation = &spec["paths"][path]["post"];
        assert!(operation["security"].is_array());
        assert!(operation.get("requestBody").is_none());
        let successes: Vec<_> = operation["responses"].as_object().unwrap().keys().filter(|status| status.starts_with('2')).map(String::as_str).collect();
        assert_eq!(successes, vec!["200"]);
        for status in ["401", "403", "404", "409", "500"] { assert!(operation["responses"][status].is_object()); }
        let parameters = spec["paths"][path]["parameters"].as_array().unwrap();
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters.iter().map(|p| p["name"].as_str().unwrap()).collect::<Vec<_>>(), vec!["space_id", "doc_id"]);
        for parameter in parameters {
            assert_eq!(parameter["in"], "path"); assert_eq!(parameter["required"], true);
            assert_eq!(parameter["schema"]["type"], "string"); assert!(parameter["schema"].get("format").is_none());
        }
        let schema = &operation["responses"]["200"]["content"]["application/json"]["schema"];
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["can_apply"]["type"], "boolean");
        assert_eq!(schema["properties"]["policy"]["properties"]["revision"]["type"], json!(["string", "null"]));
        for kind in ["users", "groups"] {
            let selection = &schema["properties"]["policy"]["properties"]["allowlist"]["properties"][kind]["items"];
            assert_eq!(selection["additionalProperties"], false);
            assert_eq!(selection["required"], json!(["id", "available"]));
            assert_eq!(selection["properties"].as_object().unwrap().len(), 2);
        }
        for hidden in ["ancestor_allowlists", "hidden_count", "email", "preview_token", "source_rows", "public_slug", "draft_body"] { assert!(!schema.to_string().contains(hidden)); }
    }

    #[test]
    fn restriction_routes_require_separate_proposal_and_explicit_confirmation() {
        let spec = openapi_spec();
        let preview = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/restrictions/preview"]["post"];
        let apply = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/restrictions/apply"]["post"];
        for op in [preview, apply] {
            assert!(op["security"].is_array());
            assert!(op["responses"]["409"].is_object());
            assert!(op["responses"]["500"].is_object());
            let successes: Vec<_> = op["responses"].as_object().unwrap().keys().filter(|status| status.starts_with('2')).map(String::as_str).collect();
            assert_eq!(successes, vec!["200"]);
            assert_eq!(op["responses"]["422"]["content"]["text/plain"]["schema"]["type"], "string");
            assert!(op["responses"]["422"]["content"].get("application/json").is_none());
            assert_eq!(op["requestBody"]["content"]["application/json"]["schema"]["additionalProperties"], false);
        }
        // Shared path parameters live on the path item, applying to every method.
        for action in ["preview", "apply"] {
            let path = format!("/spaces/{{space_id}}/documents/{{doc_id}}/restrictions/{action}");
            let parameters = spec["paths"][&path]["parameters"].as_array().unwrap();
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters.iter().map(|parameter| parameter["name"].as_str().unwrap()).collect::<Vec<_>>(), vec!["space_id", "doc_id"]);
            for parameter in parameters {
                assert_eq!(parameter["required"], true);
                assert_eq!(parameter["in"], "path");
                assert_eq!(parameter["schema"]["type"], "string");
                assert!(parameter["schema"].get("format").is_none());
            }
        }
        let confirmation = &apply["requestBody"]["content"]["application/json"]["schema"];
        assert_eq!(confirmation["required"], json!(["preview_token","confirm"]));
        assert_eq!(confirmation["properties"]["confirm"]["enum"], json!([true]));
        let response = &preview["responses"]["200"]["content"]["application/json"]["schema"];
        for hidden in ["hidden_count", "title", "public_slug", "source_rows"] {
            assert!(!response.to_string().contains(hidden));
        }
        assert!(response["properties"]["readable_pages"].is_object());
    }

    #[test]
    fn page_access_inspection_is_secured_read_only_and_omits_audience_secrets() {
        let spec = openapi_spec();
        for (path, method) in [
            ("/spaces/{space_id}/documents/{doc_id}/access", "get"),
            (
                "/spaces/{space_id}/documents/{doc_id}/access/inspect",
                "post",
            ),
        ] {
            let op = &spec["paths"][path][method];
            assert!(op["security"].is_array());
            assert!(op.get("requestBody").is_none());
            assert_eq!(
                op["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
                "#/components/schemas/PageAccessInspection"
            );
        }
        let schema = &spec["components"]["schemas"]["PageAccessInspection"];
        assert_eq!(schema["additionalProperties"], false);
        for forbidden in [
            "public_slug",
            "url",
            "ancestor_ids",
            "group_id",
            "user_id",
            "allowlist",
            "participants",
        ] {
            assert!(!schema.to_string().contains(forbidden));
        }
        assert_eq!(
            schema["properties"]["sharing"]["properties"]["guests_available"]["type"],
            json!("boolean")
        );
    }

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
            "/admin/groups/{group_id}",
            "/admin/groups/{group_id}/spaces",
            "/admin/groups/{group_id}/members",
            "/admin/groups/{group_id}/members/{user_id}",
            "/admin/plugin-runtime-audit",
            "/admin/search/reindex",
            "/admin/storage-health",
            "/admin/storage-integrity",
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
            "/ai/providers",
            "/ai/providers/{provider}",
            "/ai/providers/{provider}/test",
            "/ai/settings",
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
            "/spaces/{space_id}/retention",
            "/spaces/{space_id}/attachments",
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
            "/spaces/{space_id}/documents/{doc_id}/publication-snapshots",
            "/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}",
            "/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/raw",
            "/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/recovery-source",
            "/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/recover",
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
            "/webhooks/{webhook_id}/test",
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
    fn document_edit_projection_is_optional_current_get_capability() {
        let spec = openapi_spec();
        let document = &spec["components"]["schemas"]["Document"];
        let capability = &document["properties"]["page_edit_allowed"];
        assert_eq!(capability["type"], "boolean");
        assert!(!document["required"].as_array().unwrap().iter().any(|v| v == "page_edit_allowed"));
        let description = capability["description"].as_str().unwrap();
        for rule in ["ordinary Page Edit", "lock grants no authority", "Cached values", "viewers receive false"] { assert!(description.contains(rule)); }
        let get = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}"]["get"];
        assert_eq!(get["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"], "no-store");
    }

    #[test]
    fn selected_draft_save_source_is_edit_authorized_uncached_exact_projection() {
        let spec=openapi_spec();
        let get=&spec["paths"]["/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/save-source"]["get"];
        let description=get["description"].as_str().unwrap();
        for rule in ["ordinary Page Edit", "selected Draft Edit", "No published View prerequisite", "not proof", "deliberate review"] {assert!(description.contains(rule));}
        let response=&get["responses"]["200"];
        assert_eq!(response["headers"]["Cache-Control"]["schema"]["const"],"no-store");
        let schema=&response["content"]["application/json"]["schema"];
        assert_eq!(schema["additionalProperties"],false);
        assert_eq!(schema["required"].as_array().unwrap().len(),12);
        assert_eq!(schema["properties"]["page_edit_allowed"]["const"],true);
        assert_eq!(schema["properties"]["draft"]["additionalProperties"],false);
        for forbidden in ["published_content","audience","access_policy","receipt","operation_id"] {assert!(schema["properties"][forbidden].is_null());}
    }

    #[test]
    fn snapshot_recovery_requires_original_review_and_returns_separate_owned_draft() {
        let spec=openapi_spec();let base="/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}";
        let review=&spec["paths"][format!("{base}/recovery-source")]["get"];
        let post=&spec["paths"][format!("{base}/recover")]["post"];
        assert!(review["description"].as_str().unwrap().contains("No unrelated View prerequisite"));
        let source=&review["responses"]["200"]["content"]["application/json"]["schema"];
        assert_eq!(source["additionalProperties"],false);assert!(source["properties"]["reason"]["enum"].as_array().unwrap().contains(&json!("raw_only")));
        let request=&post["requestBody"]["content"]["application/json"]["schema"];
        assert_eq!(request["additionalProperties"],false);assert_eq!(request["required"].as_array().unwrap().len(),4);
        for forbidden in ["content","title","target_draft_id","properties","publish"] {assert!(request["properties"][forbidden].is_null());}
        for status in ["200","201"] {assert_eq!(post["responses"][status]["headers"]["Cache-Control"]["schema"]["const"],"no-store");assert_eq!(post["responses"][status]["content"]["application/json"]["schema"]["properties"]["draft"]["$ref"],"#/components/schemas/DocumentDraft");}
        assert!(post["description"].as_str().unwrap().contains("Historical acknowledgement is not a current editing baseline"));
    }

    #[test]
    fn typed_history_wire_is_editor_guarded_whitelisted_and_raw_downloadable() {
        let spec=openapi_spec();let base="/spaces/{space_id}/documents/{doc_id}/publication-snapshots";
        for path in [base,&format!("{base}/{{snapshot_id}}"),&format!("{base}/{{snapshot_id}}/raw")] {
            let operation=&spec["paths"][path]["get"];
            assert!(operation["security"].is_array());assert!(operation["description"].as_str().unwrap().contains("ordinary page Edit"));
            assert_eq!(operation["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"],"no-store");
            for code in ["401","403","404","409"] {assert!(operation["responses"][code].is_object());}
            assert!(spec["paths"][path]["post"].is_null());
        }
        let raw=&spec["paths"][format!("{base}/{{snapshot_id}}/raw")]["get"]["responses"]["200"];
        assert_eq!(raw["content"]["application/octet-stream"]["schema"]["format"],"binary");assert!(raw["content"]["application/json"].is_null());
        assert_eq!(raw["headers"]["Content-Disposition"]["schema"]["const"],"attachment; filename=\"publication-snapshot.md\"");
        assert_eq!(spec["paths"][format!("{base}/{{snapshot_id}}")]["parameters"][2]["name"],"snapshot_id");assert!(spec["paths"][format!("{base}/{{snapshot_id}}")]["parameters"][2]["schema"]["format"].is_null());
        let detail=&spec["components"]["schemas"]["PublicationSnapshotDetail"];
        assert_eq!(detail["additionalProperties"],false);assert_eq!(detail["properties"]["content"]["properties"]["raw_byte_length"]["maximum"],6000000);
        assert_eq!(detail["properties"]["comparisons"]["properties"]["properties_schema"]["items"]["properties"]["options_raw"]["type"],json!(["string","null"]));
        let text=detail.to_string();for key in ["storage_path","baseline_source_page","request_json","storage_manifest_json","change_set","locked_by"]{assert!(!text.contains(key),"private field documented: {key}")}
        assert!(spec["paths"]["/spaces/{space_id}/documents/{doc_id}/versions"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["items"]["$ref"].as_str().unwrap().contains("DocumentVersion"));
        assert!(spec["paths"]["/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish"]["post"]["description"].as_str().unwrap().contains("atomically records"));
    }

    #[test]
    fn owned_draft_publication_requires_original_sources_and_returns_receipt() {
        let spec = openapi_spec();
        let source = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish-source"]["get"];
        assert!(source.get("requestBody").is_none());
        assert!(source["responses"]["200"]["content"]["application/json"]["schema"]["required"].as_array().unwrap().contains(&json!("page_source_digest")));
        let publish = &spec["paths"]["/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish"]["post"];
        assert_eq!(publish["requestBody"]["required"], true);
        assert_eq!(publish["requestBody"]["content"]["application/json"]["schema"]["required"],json!(["schema_version","operation_id","draft_source_digest","page_source_digest"]));
        assert_eq!(publish["responses"]["200"]["content"]["application/json"]["schema"]["properties"]["disposition"]["enum"],json!(["applied","historical"]));
        for op in [source,publish] { for status in ["201","202","204"] { assert!(op["responses"].get(status).is_none()); } }
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
        assert!(spec["components"]["schemas"]["Group"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("space_grant_count")));
        assert!(spec["components"]["schemas"]["Group"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("scim_managed")));
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}"]["put"]["requestBody"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/GroupUpdateRequest"
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}"]["delete"]["responses"]["204"]["description"],
            "Group, its memberships, and its space grants deleted."
        );
        assert_eq!(
            spec["paths"]["/admin/groups/{group_id}/spaces"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/GroupSpaceGrant"
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
            spec["components"]["schemas"]["Document"]["properties"]["draft_id"]["nullable"],
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
        assert_eq!(spec["components"]["schemas"]["DocumentContent"]["properties"]["published_metadata"]["required"],json!(["title","icon","cover_image"]));
        assert_eq!(spec["components"]["schemas"]["DocumentContent"]["properties"]["published_metadata"]["additionalProperties"],false);
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
                ["409"]["description"],
            "Direct content writes are rejected; create or update a document draft."
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
        assert!(spec["paths"]["/spaces/{space_id}/documents/{doc_id}/publish"]["parameters"][1]
            ["schema"].get("format").is_none());
        assert_eq!(
            spec["paths"]["/spaces/{space_id}/documents/{doc_id}/unpublish"]["post"]["responses"]
                ["204"]["description"],
            "Public link disabled; published revision and drafts are unchanged."
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
    fn draft_discussion_routes_document_exact_source_and_closed_custody() {
        let spec = openapi_spec();
        let active = "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/discussion";
        let comment = "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/discussion/{comment_id}";
        let resolve = "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/discussion/{comment_id}/resolve";
        let closed_list = "/spaces/{space_id}/documents/{doc_id}/draft-discussions";
        let closed = "/spaces/{space_id}/documents/{doc_id}/draft-discussions/{discussion_id}";
        assert_eq!(
            spec["paths"][active]["get"]["parameters"][0]["name"],
            "source_digest"
        );
        assert_eq!(
            spec["paths"][active]["post"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/DraftDiscussionCreateRequest"
        );
        assert_eq!(
            spec["paths"][comment]["put"]["requestBody"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/DraftDiscussionUpdateRequest"
        );
        assert_eq!(
            spec["paths"][comment]["delete"]["parameters"][0]["schema"]["pattern"],
            "^[0-9a-f]{64}$"
        );
        assert_eq!(
            spec["paths"][resolve]["post"]["responses"]["204"]["description"],
            "Draft comment resolved."
        );
        assert_eq!(
            spec["paths"][closed_list]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/ClosedDraftDiscussionList"
        );
        assert_eq!(
            spec["components"]["schemas"]["ClosedDraftDiscussionList"]["properties"]
                ["discussions"]["maxItems"],
            100
        );
        assert_eq!(
            spec["paths"][closed]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/DraftDiscussion"
        );
        assert_eq!(
            spec["components"]["schemas"]["DraftDiscussionComment"]["properties"]
                ["anchor_outdated"]["type"],
            "boolean"
        );
        assert_eq!(
            spec["components"]["schemas"]["DraftDiscussion"]["properties"]
                ["mention_candidates"]["maxItems"],
            100
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
                ["post"]["responses"]["400"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/Error"
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
        for field in ["actor_kind", "actor_id"] {
            assert_eq!(
                spec["components"]["schemas"]["AuditLogEntry"]["properties"][field]["nullable"],
                true
            );
            assert!(!spec["components"]["schemas"]["AuditLogEntry"]["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(field)));
        }
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
    fn saved_webhook_test_spec_is_queued_without_arbitrary_request_body() {
        let spec = openapi_spec();
        let route = &spec["paths"]["/webhooks/{webhook_id}/test"];
        assert_eq!(route["parameters"][0]["name"], "webhook_id");
        assert_eq!(
            route["post"]["responses"]["202"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/WebhookTestQueued"
        );
        assert!(route["post"].get("requestBody").is_none());
        assert!(route["post"]["responses"].get("200").is_none());
        assert_eq!(
            spec["components"]["schemas"]["WebhookTestQueued"]["properties"]["status"]["enum"],
            json!(["queued"])
        );
        assert!(spec["components"]["schemas"]["Webhook"]["properties"]
            .get("secret_hash")
            .is_none());
        assert!(spec["components"]["schemas"]["Webhook"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("created_by")));
        assert!(spec["components"]["schemas"]["WebhookDelivery"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("test_id")));
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
            spec["components"]["schemas"]["WebhookCreateRequest"]["properties"]["events"]
                ["items"]["enum"],
            json!(["public_page.published"])
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
            spec["paths"]["/auth/native-handoff/complete"]["post"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/NativeHandoffCompleteResponse"
        );
        assert_eq!(
            spec["paths"]["/auth/native-handoff/exchange"]["post"]["responses"]["202"]["content"]
                ["application/json"]["schema"]["$ref"],
            "#/components/schemas/NativeHandoffPendingResponse"
        );
        assert_eq!(
            spec["components"]["schemas"]["NativeHandoffExchangeRequest"]["required"],
            json!(["handoff_id", "code_verifier"])
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
                    && required.contains(&serde_json::json!("client_platform"))
                    && required.contains(&serde_json::json!("is_current"))
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
    #[test]
    fn published_source_original_and_exact_receipt_result_contract() {
        let spec=openapi_spec();
        assert_eq!(spec["components"]["schemas"]["DocumentContent"]["properties"]["published_source_digest"]["pattern"],"^[0-9a-f]{64}$");
        assert_eq!(spec["components"]["schemas"]["SyncReplayOperationResult"]["properties"]["published_source"]["$ref"],"#/components/schemas/PublishedSourceReplayEvidence");
        let required=spec["components"]["schemas"]["PublishedSourceReplayEvidence"]["required"].as_array().unwrap();for field in ["published_source_digest","payload_fingerprint","original_source_digest","document_id","space_id"] {assert!(required.iter().any(|v|v==field));}
    }

    #[test]
    fn draft_birth_original_private_source_and_receipt_contract() {
        let spec=openapi_spec();let request=&spec["components"]["schemas"]["DocumentDraftCreateRequest"];
        assert_eq!(request["properties"]["operation_id"]["format"],"uuid");assert_eq!(request["properties"]["source_digest"]["pattern"],"^[0-9a-f]{64}$");assert_eq!(request["properties"]["source_draft_id"]["type"],"string");assert!(request["description"].as_str().unwrap().contains("source_required"));assert_eq!(spec["components"]["schemas"]["DocumentDraft"]["properties"]["birth_operation_id"]["format"],"uuid");
    }

    #[test]
    fn owner_draft_metadata_keeps_view_method_and_no_erasure_contract() {
        let spec = openapi_spec();
        for (path, method) in [
            ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}", "delete"),
            ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access", "get"),
            ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access-candidates", "get"),
            ("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}", "put"),
        ] {
            let operation = &spec["paths"][path][method];
            assert!(operation["description"].as_str().unwrap().contains("PageView"));
            assert!(operation["description"].as_str().unwrap().contains("retained"));
            assert!(operation["security"].is_array());
        }
        assert!(spec["paths"]["/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}"]["put"]["responses"]["204"].is_object());
    }

    #[test]
    fn explicit_manager_purge_has_count_retention_and_unknown_contract() {
        let spec=openapi_spec(); let op=&spec["paths"]["/spaces/{space_id}/drafts"]["delete"];
        assert!(op["security"].is_array());
        let description=op["description"].as_str().unwrap();
        for required in ["Write/Admin", "ManageSpace", "retain", "zero", "no success audit", "unknown", "no immutable purge receipt"] {assert!(description.contains(required));}
        assert_eq!(op["responses"]["200"]["content"]["application/json"]["schema"]["properties"]["deleted"]["minimum"],0);
        assert_eq!(op["responses"]["200"]["content"]["application/json"]["schema"]["required"],json!(["deleted"]));
        assert!(op["responses"]["409"].is_object()); assert!(op["responses"]["204"].is_null());
        assert!(spec["paths"]["/spaces/{space_id}/drafts"]["get"].is_object());
    }

    #[test]
    fn space_delete_owner_write_atomic_audit_and_retained_bytes_contract() {
        let spec = openapi_spec();
        let op = &spec["paths"]["/spaces/{space_id}"]["delete"];
        assert!(op["security"].is_array());
        let description = op["description"].as_str().unwrap();
        for required in ["Write/Admin", "owner-only", "SQL cascades", "space.delete", "immutable receipts", "retained", "unknown", "cleanup is deferred"] {
            assert!(description.contains(required));
        }
        assert_eq!(op["responses"]["204"]["description"], "Space deleted.");
        assert!(op["responses"]["204"]["content"].is_null());
        assert!(op["responses"]["200"].is_null());
        assert!(op["responses"]["403"].is_object());
        assert!(op["responses"]["409"].is_object());
        assert!(op["requestBody"].is_null());
    }

    #[test]
    fn historical_application_test_schema_never_promotes_verified_health() {
        let spec=openapi_spec();
        let report=&spec["components"]["schemas"]["StorageHealthReport"];
        let history=&report["properties"]["application_restore_history"];
        assert_eq!(history["properties"]["status"]["enum"],serde_json::json!(["unavailable","completed_isolated_test"]));
        assert_eq!(history["properties"]["full_supported_restore_verified"]["enum"],serde_json::json!([false]));
        assert_eq!(history["properties"]["scope"]["enum"],serde_json::json!(["restored-pg-ledger1-ordinary-application-v1","restored-sqlite-ledger1-ordinary-application-v1",null]));
        assert_eq!(history["properties"]["completed_at"]["format"],"date-time");
        assert_eq!(history["properties"]["exact_binary_bytes"]["type"],"string");
        assert_eq!(report["properties"]["verified_restore"]["properties"]["status"]["enum"],serde_json::json!(["unknown"]));
        assert!(report["required"].as_array().unwrap().iter().any(|v|v=="application_restore_history"));
    }

}
