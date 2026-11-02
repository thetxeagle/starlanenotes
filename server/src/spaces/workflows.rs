use super::mutation_guards::{lock_spaces_for_mutation, require_current_page_edit};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::AnyConnection;
use std::{collections::HashSet, time::Duration};
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::auth::middleware::{AdminUser, AuthUser};
use crate::auth::routes::AppError;
use crate::AppState;

const WORKFLOW_DEADLINE_ESCALATION_INTERVAL_ENV: &str =
    "WORKFLOW_DEADLINE_ESCALATION_INTERVAL_MINUTES";
const WORKFLOW_DEADLINE_ESCALATION_ACTOR_ENV: &str = "WORKFLOW_DEADLINE_ESCALATION_ACTOR_ID";
const WORKFLOW_DEADLINE_ESCALATION_TARGET_ENV: &str = "WORKFLOW_DEADLINE_ESCALATION_TARGET";
const WORKFLOW_REASSIGNMENT_POLICY_ENV: &str = "WORKFLOW_REASSIGNMENT_POLICY";

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/workflows/deadline-escalations",
            get(list_workflow_deadline_escalations).post(run_workflow_deadline_escalations),
        )
        .route(
            "/spaces/{space_id}/workflow-templates",
            get(list_workflow_templates).post(create_workflow_template),
        )
        .route(
            "/spaces/{space_id}/workflow-templates/{template_id}",
            get(get_workflow_template)
                .put(update_workflow_template)
                .delete(delete_workflow_template),
        )
        .route(
            "/spaces/{space_id}/documents/{document_id}/workflow",
            get(get_document_workflow)
                .put(enable_document_workflow)
                .delete(disable_document_workflow),
        )
        .route(
            "/spaces/{space_id}/documents/{document_id}/workflow/events",
            get(list_document_workflow_events).post(record_document_workflow_event),
        )
        .route(
            "/spaces/{space_id}/documents/{document_id}/workflow/transitions",
            get(list_document_workflow_events).post(transition_document_workflow),
        )
}

pub fn maybe_spawn_deadline_escalation_worker(state: AppState) {
    let interval_minutes = match std::env::var(WORKFLOW_DEADLINE_ESCALATION_INTERVAL_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
    {
        Some(value) => value,
        None => return,
    };
    let actor_id = match std::env::var(WORKFLOW_DEADLINE_ESCALATION_ACTOR_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) => value,
        None => {
            tracing::warn!(
                "{} is set but {} is missing; workflow deadline escalation worker is disabled",
                WORKFLOW_DEADLINE_ESCALATION_INTERVAL_ENV,
                WORKFLOW_DEADLINE_ESCALATION_ACTOR_ENV
            );
            return;
        }
    };
    let target = std::env::var(WORKFLOW_DEADLINE_ESCALATION_TARGET_ENV)
        .ok()
        .and_then(|value| normalize_deadline_escalation_target(Some(&value)).ok())
        .unwrap_or_else(|| "admin".to_string());

    tracing::info!(
        "Workflow deadline escalation worker enabled: every {} minute(s), target {}",
        interval_minutes,
        target
    );
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_minutes * 60));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            match run_workflow_deadline_escalations_for_actor(
                &state,
                &actor_id,
                &target,
                chrono::Utc::now().date_naive(),
            )
            .await
            {
                Ok(result) if result.escalated > 0 => {
                    tracing::info!(
                        "Workflow deadline escalation worker recorded {} escalation(s)",
                        result.escalated
                    );
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!("Workflow deadline escalation worker failed: {:?}", err);
                }
            }
        }
    });
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WorkflowStage {
    pub id: Option<String>,
    pub name: String,
    pub sort_order: Option<i64>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WorkflowApprover {
    pub id: Option<String>,
    pub approver_type: String,
    pub approver_id: String,
    pub stage_id: Option<String>,
}

#[derive(Deserialize)]
pub struct WorkflowTemplateRequest {
    pub name: String,
    pub description: Option<String>,
    pub stages: Vec<WorkflowStage>,
    pub approvers: Option<Vec<WorkflowApprover>>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowTemplateResponse {
    pub id: String,
    pub space_id: String,
    pub name: String,
    pub description: Option<String>,
    pub stages: Vec<WorkflowStage>,
    pub approvers: Vec<WorkflowApprover>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct EnableDocumentWorkflowRequest {
    pub template_id: String,
    pub stage_id: Option<String>,
    pub source_id: Option<String>,
    pub event_source_id: Option<String>,
    pub event_created_by: Option<String>,
    pub event_created_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct DocumentWorkflowResponse {
    pub id: String,
    pub document_id: String,
    pub workflow_template_id: String,
    pub workflow_template_name: String,
    pub current_stage_id: String,
    pub current_stage_name: String,
    pub status: String,
    pub stages: Vec<WorkflowStage>,
    pub approvers: Vec<WorkflowApprover>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct TransitionDocumentWorkflowRequest {
    pub stage_id: String,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct RecordDocumentWorkflowEventRequest {
    pub action: String,
    pub note: Option<String>,
    pub source_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: Option<String>,
    pub from_stage_id: Option<String>,
    pub to_stage_id: Option<String>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct WorkflowReassignmentPolicy {
    #[serde(default = "default_true")]
    pub allow_delegate: bool,
    #[serde(default = "default_true")]
    pub allow_escalate: bool,
    #[serde(default)]
    pub allowed_targets: Option<Vec<String>>,
    #[serde(default)]
    pub require_due_date: bool,
    #[serde(default)]
    pub max_due_days: Option<i64>,
}

impl Default for WorkflowReassignmentPolicy {
    fn default() -> Self {
        Self {
            allow_delegate: true,
            allow_escalate: true,
            allowed_targets: None,
            require_due_date: false,
            max_due_days: None,
        }
    }
}

#[derive(Deserialize)]
pub struct WorkflowDeadlineEscalationQuery {
    pub before: Option<String>,
}

#[derive(Deserialize)]
pub struct WorkflowDeadlineEscalationRunQuery {
    pub before: Option<String>,
    pub target: Option<String>,
    pub dry_run: Option<bool>,
}

#[derive(Clone, Serialize)]
pub struct WorkflowDeadlineEscalationCandidateResponse {
    pub event_id: String,
    pub document_workflow_id: String,
    pub document_id: String,
    pub document_title: String,
    pub space_id: String,
    pub workflow_template_name: String,
    pub current_stage_id: String,
    pub target: String,
    pub due_date: String,
    pub note: String,
    pub delegated_by: String,
    pub delegated_at: String,
}

#[derive(Serialize)]
pub struct WorkflowDeadlineEscalationRunResponse {
    pub scanned: usize,
    pub escalated: usize,
    pub dry_run: bool,
    pub target: String,
    pub candidates: Vec<WorkflowDeadlineEscalationCandidateResponse>,
}

#[derive(Serialize)]
pub struct DocumentWorkflowEventResponse {
    pub id: String,
    pub document_workflow_id: String,
    pub document_id: String,
    pub from_stage_id: Option<String>,
    pub to_stage_id: String,
    pub action: String,
    pub note: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

fn normalize_request(
    req: &WorkflowTemplateRequest,
) -> Result<(Vec<WorkflowStage>, Vec<WorkflowApprover>), AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Workflow template name is required".into(),
        ));
    }
    if req.name.len() > 100 {
        return Err(AppError::BadRequest(
            "Workflow template name must be at most 100 characters".into(),
        ));
    }
    if let Some(description) = &req.description {
        if description.len() > 1000 {
            return Err(AppError::BadRequest(
                "Workflow template description must be at most 1000 characters".into(),
            ));
        }
    }

    let mut stages = req
        .stages
        .iter()
        .enumerate()
        .filter_map(|(index, stage)| {
            let name = stage.name.trim();
            if name.is_empty() {
                return None;
            }
            Some(WorkflowStage {
                id: Some(
                    stage
                        .id
                        .as_deref()
                        .filter(|id| !id.trim().is_empty())
                        .map(str::trim)
                        .map(str::to_string)
                        .unwrap_or_else(|| Uuid::new_v4().to_string()),
                ),
                name: name.to_string(),
                sort_order: Some(stage.sort_order.unwrap_or(index as i64)),
            })
        })
        .collect::<Vec<_>>();

    if stages.len() < 2 {
        return Err(AppError::BadRequest(
            "Workflow templates require at least two stages".into(),
        ));
    }
    if stages.len() > 12 {
        return Err(AppError::BadRequest(
            "Workflow templates can have at most 12 stages".into(),
        ));
    }
    stages.sort_by_key(|stage| stage.sort_order.unwrap_or(0));

    let stage_ids = stages
        .iter()
        .filter_map(|stage| stage.id.as_deref())
        .collect::<std::collections::HashSet<_>>();
    if stage_ids.len() != stages.len() {
        return Err(AppError::BadRequest(
            "Workflow stage IDs must be unique".into(),
        ));
    }
    let approvers = req
        .approvers
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|approver| {
            let approver_id = approver.approver_id.trim().to_string();
            if approver_id.is_empty() {
                return Err(AppError::BadRequest("Workflow approver is required".into()));
            }
            if approver.approver_type != "user" && approver.approver_type != "role" {
                return Err(AppError::BadRequest(
                    "Workflow approver type must be user or role".into(),
                ));
            }
            if let Some(stage_id) = approver.stage_id.as_deref() {
                if !stage_ids.contains(stage_id) {
                    return Err(AppError::BadRequest(
                        "Workflow approver stage must exist".into(),
                    ));
                }
            }
            Ok(WorkflowApprover {
                id: Some(
                    approver
                        .id
                        .as_deref()
                        .filter(|id| !id.trim().is_empty())
                        .map(str::trim)
                        .map(str::to_string)
                        .unwrap_or_else(|| Uuid::new_v4().to_string()),
                ),
                approver_type: approver.approver_type,
                approver_id,
                stage_id: approver.stage_id,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    Ok((stages, approvers))
}

fn parse_json_vec<T: for<'de> Deserialize<'de>>(raw: String) -> Vec<T> {
    serde_json::from_str(&raw).unwrap_or_default()
}

fn default_true() -> bool {
    true
}

fn configured_workflow_reassignment_policy() -> Result<WorkflowReassignmentPolicy, AppError> {
    let Some(raw) = std::env::var(WORKFLOW_REASSIGNMENT_POLICY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(WorkflowReassignmentPolicy::default());
    };
    serde_json::from_str(&raw).map_err(|error| {
        AppError::Internal(format!("Invalid workflow reassignment policy: {error}"))
    })
}

fn validate_workflow_reassignment_policy(
    policy: &WorkflowReassignmentPolicy,
    action: &str,
    note: &str,
    today: NaiveDate,
) -> Result<(), AppError> {
    if action == "delegate" && !policy.allow_delegate {
        return Err(AppError::BadRequest(
            "Workflow delegation is disabled by reassignment policy".into(),
        ));
    }
    if action == "escalate" && !policy.allow_escalate {
        return Err(AppError::BadRequest(
            "Workflow escalation is disabled by reassignment policy".into(),
        ));
    }

    if let Some(allowed_targets) = &policy.allowed_targets {
        let target = extract_workflow_handoff_target(note).ok_or_else(|| {
            AppError::BadRequest("Workflow reassignment target is required by policy".into())
        })?;
        let normalized_target = normalize_policy_target(&target);
        let allowed = allowed_targets
            .iter()
            .map(|target| normalize_policy_target(target))
            .any(|target| target == normalized_target);
        if !allowed {
            return Err(AppError::BadRequest(
                "Workflow reassignment target is not allowed by policy".into(),
            ));
        }
    }

    let due_date = extract_workflow_due_date(note);
    if policy.require_due_date && due_date.is_none() {
        return Err(AppError::BadRequest(
            "Workflow reassignment due date is required by policy".into(),
        ));
    }
    if let (Some(max_due_days), Some(due_date)) = (policy.max_due_days, due_date) {
        if max_due_days >= 0 && due_date > today + chrono::Duration::days(max_due_days) {
            return Err(AppError::BadRequest(
                "Workflow reassignment due date exceeds policy window".into(),
            ));
        }
    }

    Ok(())
}

fn normalize_policy_target(target: &str) -> String {
    target
        .trim()
        .trim_start_matches('@')
        .strip_prefix("role:")
        .unwrap_or_else(|| target.trim().trim_start_matches('@'))
        .trim()
        .to_lowercase()
}

fn row_to_workflow_template(
    row: (
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
    ),
) -> WorkflowTemplateResponse {
    let (id, space_id, name, description, stages, approvers, created_by, created_at, updated_at) =
        row;
    WorkflowTemplateResponse {
        id,
        space_id,
        name,
        description,
        stages: parse_json_vec(stages),
        approvers: parse_json_vec(approvers),
        created_by,
        created_at,
        updated_at,
    }
}

fn document_workflow_response_from_row(
    row: (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ),
) -> DocumentWorkflowResponse {
    let (
        id,
        document_id,
        workflow_template_id,
        current_stage_id,
        status,
        created_by,
        created_at,
        updated_at,
        workflow_template_name,
        stages_raw,
        approvers_raw,
    ) = row;
    let stages: Vec<WorkflowStage> = parse_json_vec(stages_raw);
    let current_stage_name = stages
        .iter()
        .find(|stage| stage.id.as_deref() == Some(current_stage_id.as_str()))
        .map(|stage| stage.name.clone())
        .unwrap_or_else(|| "Unknown stage".to_string());

    DocumentWorkflowResponse {
        id,
        document_id,
        workflow_template_id,
        workflow_template_name,
        current_stage_id,
        current_stage_name,
        status,
        stages,
        approvers: parse_json_vec(approvers_raw),
        created_by,
        created_at,
        updated_at,
    }
}

fn workflow_event_from_row(
    row: (
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        String,
        String,
    ),
) -> DocumentWorkflowEventResponse {
    let (
        id,
        document_workflow_id,
        document_id,
        from_stage_id,
        to_stage_id,
        action,
        note,
        created_by,
        created_at,
    ) = row;
    DocumentWorkflowEventResponse {
        id,
        document_workflow_id,
        document_id,
        from_stage_id,
        to_stage_id,
        action,
        note,
        created_by,
        created_at,
    }
}

fn workflow_approver_allows_transition(
    approvers: &[WorkflowApprover],
    stage_id: &str,
    user_id: &str,
    access: &super::access::SpaceAccess,
) -> bool {
    if approvers.is_empty() {
        return true;
    }

    let scoped_approvers = approvers
        .iter()
        .filter(|approver| approver.stage_id.as_deref() == Some(stage_id))
        .collect::<Vec<_>>();
    let applicable_approvers = if scoped_approvers.is_empty() {
        approvers
            .iter()
            .filter(|approver| approver.stage_id.is_none())
            .collect::<Vec<_>>()
    } else {
        scoped_approvers
    };

    if applicable_approvers.is_empty() {
        return true;
    }

    applicable_approvers.iter().any(|approver| {
        (approver.approver_type == "user" && approver.approver_id == user_id)
            || (approver.approver_type == "role"
                && (access
                    .roles
                    .iter()
                    .any(|role| role.id == approver.approver_id)
                    || access.instance_admin))
    })
}

fn applicable_workflow_approvers<'a>(
    approvers: &'a [WorkflowApprover],
    stage_id: &str,
) -> Vec<&'a WorkflowApprover> {
    let scoped = approvers
        .iter()
        .filter(|approver| approver.stage_id.as_deref() == Some(stage_id))
        .collect::<Vec<_>>();
    if !scoped.is_empty() {
        return scoped;
    }
    approvers
        .iter()
        .filter(|approver| approver.stage_id.is_none())
        .collect()
}

async fn workflow_role_recipient_candidates(
    state: &AppState,
    space_id: &str,
    role_id: &str,
) -> Result<HashSet<String>, AppError> {
    let users:Vec<String>=sqlx::query_scalar("SELECT u.id FROM users u WHERE u.is_active=1 AND (u.role='admin' OR EXISTS(SELECT 1 FROM space_members sm WHERE sm.space_id=$1 AND sm.user_id=u.id AND sm.role=$2) OR EXISTS(SELECT 1 FROM space_group_permissions sg JOIN group_members gm ON gm.group_id=sg.group_id WHERE sg.space_id=$1 AND gm.user_id=u.id AND sg.role=$2) OR EXISTS(SELECT 1 FROM group_global_roles gr JOIN group_members gm ON gm.group_id=gr.group_id WHERE gm.user_id=u.id AND gr.role_id=$2))").bind(space_id).bind(role_id).fetch_all(&state.db).await.map_err(AppError::internal)?;
    Ok(users.into_iter().collect())
}
async fn workflow_notification_recipient(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    user_id: &str,
) -> Result<Option<(super::access::SpaceAccess, String)>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::documents::readable_notification_page_title(&mut conn, space_id, document_id, user_id)
        .await
}
async fn notify_workflow_approvers(
    state: &AppState,
    space_id: &str,
    workflow: &DocumentWorkflowResponse,
    stage_id: &str,
    actor_id: &str,
) -> Result<(), AppError> {
    let approvers = applicable_workflow_approvers(&workflow.approvers, stage_id);
    if approvers.is_empty() {
        return Ok(());
    }
    let stage_name = workflow
        .stages
        .iter()
        .find(|stage| stage.id.as_deref() == Some(stage_id))
        .map(|stage| stage.name.as_str())
        .unwrap_or("approval");
    let mut recipients = HashSet::new();
    for approver in approvers {
        match approver.approver_type.as_str() {
            "user" => {
                recipients.insert(approver.approver_id.clone());
            }
            "role" => {
                recipients.extend(
                    workflow_role_recipient_candidates(state, space_id, &approver.approver_id)
                        .await?,
                );
            }
            _ => {}
        }
    }
    for recipient in recipients {
        if recipient == actor_id {
            continue;
        }
        let Some((access, title)) =
            workflow_notification_recipient(state, space_id, &workflow.document_id, &recipient)
                .await?
        else {
            continue;
        };
        if !workflow_approver_allows_transition(&workflow.approvers, stage_id, &recipient, &access)
        {
            continue;
        }
        crate::notifications::notify(
            &state.db,
            &recipient,
            "workflow_approval",
            &format!("Workflow approval requested: {title}"),
            Some(&format!(
                "{title} is ready for {stage_name} in {}.",
                workflow.workflow_template_name
            )),
            Some(actor_id),
            Some("document"),
            Some(&workflow.document_id),
        )
        .await;
    }
    Ok(())
}
async fn notify_workflow_handoff_targets(
    state: &AppState,
    space_id: &str,
    workflow: &DocumentWorkflowResponse,
    action: &str,
    note: &str,
    actor_id: &str,
) -> Result<(), AppError> {
    let Some(target) = extract_workflow_handoff_target(note) else {
        return Ok(());
    };
    let recipients =
        resolve_handoff_target_users(state, space_id, &workflow.document_id, &target).await?;
    let verb = if action == "delegate" {
        "delegated"
    } else {
        "escalated"
    };
    for recipient in recipients {
        if recipient == actor_id {
            continue;
        }
        let Some((_, title)) =
            workflow_notification_recipient(state, space_id, &workflow.document_id, &recipient)
                .await?
        else {
            continue;
        };
        crate::notifications::notify(
            &state.db,
            &recipient,
            if action == "delegate" {
                "workflow_delegate"
            } else {
                "workflow_escalate"
            },
            &format!("Workflow {verb}: {title}"),
            Some(&format!(
                "{title} was {verb} to {target} in {}. {note}",
                workflow.workflow_template_name
            )),
            Some(actor_id),
            Some("document"),
            Some(&workflow.document_id),
        )
        .await;
    }
    Ok(())
}
async fn resolve_handoff_target_users(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    target: &str,
) -> Result<HashSet<String>, AppError> {
    let normalized = target
        .trim()
        .trim_start_matches('@')
        .strip_prefix("role:")
        .unwrap_or_else(|| target.trim().trim_start_matches('@'))
        .trim();
    if normalized.is_empty() {
        return Ok(HashSet::new());
    }
    let matches: Vec<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE is_active=1 AND (id=$1 OR email=$1)")
            .bind(normalized)
            .fetch_all(&state.db)
            .await
            .map_err(AppError::internal)?;
    let explicit: HashSet<String> = matches.into_iter().collect();
    let mut candidates = workflow_role_recipient_candidates(state, space_id, normalized).await?;
    candidates.extend(explicit.iter().cloned());
    let mut recipients = HashSet::new();
    for user in candidates {
        let Some((access, _)) =
            workflow_notification_recipient(state, space_id, document_id, &user).await?
        else {
            continue;
        };
        if explicit.contains(&user)
            || access.roles.iter().any(|role| role.id == normalized)
            || (normalized == "admin" && access.instance_admin)
        {
            recipients.insert(user);
        }
    }
    Ok(recipients)
}

fn extract_workflow_handoff_target(note: &str) -> Option<String> {
    let lower = note.to_lowercase();
    let prefix = if lower.starts_with("delegated to ") {
        "delegated to "
    } else if lower.starts_with("escalated to ") {
        "escalated to "
    } else {
        return None;
    };
    let target = note[prefix.len()..]
        .split(':')
        .next()
        .unwrap_or_default()
        .trim();
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

fn extract_workflow_due_date(note: &str) -> Option<NaiveDate> {
    let lower = note.to_lowercase();
    let marker = lower.find("due").or_else(|| lower.find("deadline"))?;
    let candidate = note[marker..]
        .split(|c: char| c.is_whitespace() || c == ':' || c == ';' || c == ',')
        .find(|part| {
            part.len() == 10
                && part.as_bytes().get(4) == Some(&b'-')
                && part.as_bytes().get(7) == Some(&b'-')
        })?;
    NaiveDate::parse_from_str(candidate, "%Y-%m-%d").ok()
}

fn parse_deadline_scan_before(value: Option<&str>) -> Result<NaiveDate, AppError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(date) => NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| AppError::BadRequest("before must use YYYY-MM-DD".into())),
        None => Ok(chrono::Utc::now().date_naive()),
    }
}

fn normalize_deadline_escalation_target(value: Option<&str>) -> Result<String, AppError> {
    let target = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("admin");
    if target.len() > 254 {
        return Err(AppError::BadRequest(
            "target must be at most 254 characters".into(),
        ));
    }
    Ok(target.to_string())
}

fn format_deadline_escalation_note(
    target: &str,
    candidate: &WorkflowDeadlineEscalationCandidateResponse,
) -> String {
    let prior_target = if candidate.target.trim().is_empty() {
        "unassigned".to_string()
    } else {
        candidate.target.clone()
    };
    format!(
        "Escalated to {}: deadline {} passed after delegation to {}",
        target, candidate.due_date, prior_target
    )
}

/// HTTP proof is retained exactly; only the configured internal worker may use
/// an explicitly sessionless principal. A service key never becomes a job user.
#[derive(Clone, Copy)]
enum DeadlineActor<'a> {
    Http {
        admin: &'a AdminUser,
        auth: &'a AuthUser,
    },
    BackgroundUser(&'a str),
}
impl<'a> DeadlineActor<'a> {
    fn user_id(self) -> &'a str {
        match self {
            Self::Http { auth, .. } => &auth.claims.sub,
            Self::BackgroundUser(id) => id,
        }
    }
    fn principal(self) -> Result<super::access::Principal<'a>, AppError> {
        match self {
            Self::Http { auth, .. } => super::access::principal_from_auth(auth),
            Self::BackgroundUser(user_id) => Ok(super::access::Principal::User {
                user_id,
                session_id: None,
                token_expires_at: None,
            }),
        }
    }
    async fn require_identity(self, conn: &mut AnyConnection) -> Result<(), AppError> {
        match self {
            Self::Http { admin, .. } => {
                match crate::auth::middleware::require_current_admin(conn, admin).await? {
                    crate::auth::middleware::CurrentAdminAuthority::HumanInstanceAdmin {
                        user_id,
                    } if user_id == self.user_id() => {}
                    _ => {
                        return Err(AppError::Unauthorized(
                            "Workflow deadline actor unavailable".into(),
                        ))
                    }
                }
            }
            Self::BackgroundUser(id) => {
                let active: Option<i64> =
                    sqlx::query_scalar("SELECT 1 FROM users WHERE id=$1 AND is_active=1")
                        .bind(id)
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(AppError::internal)?;
                if active.is_none() {
                    return Err(AppError::Unauthorized(
                        "Workflow deadline actor unavailable".into(),
                    ));
                }
            }
        }
        self.principal()?;
        Ok(())
    }
}
fn deadline_http_auth(admin: &AdminUser) -> AuthUser {
    AuthUser {
        claims: admin.claims.clone(),
        api_key_scopes: admin.api_key_scopes.clone(),
    }
}
async fn deadline_page_view(
    conn: &mut AnyConnection,
    actor: DeadlineActor<'_>,
    candidate: &WorkflowDeadlineEscalationCandidateResponse,
) -> Result<(), AppError> {
    super::access::resolve_page_access(
        conn,
        actor.principal()?,
        &candidate.space_id,
        &candidate.document_id,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
async fn deadline_candidate_source(
    conn: &mut AnyConnection,
    candidate: &WorkflowDeadlineEscalationCandidateResponse,
) -> Result<(), AppError> {
    let current: Option<(String,String,String,String,String,String)> = sqlx::query_as("SELECT d.title,wt.name,dw.current_stage_id,dwe.note,dwe.created_by,dwe.created_at FROM document_workflow_events dwe JOIN document_workflows dw ON dw.id=dwe.document_workflow_id AND dw.document_id=dwe.document_id JOIN documents d ON d.id=dw.document_id JOIN workflow_templates wt ON wt.id=dw.workflow_template_id AND wt.space_id=d.space_id WHERE dwe.id=$1 AND dw.id=$2 AND d.id=$3 AND d.space_id=$4 AND dwe.action='delegate' AND dw.status='active' AND d.deleted_at IS NULL AND d.archived_at IS NULL")
        .bind(&candidate.event_id).bind(&candidate.document_workflow_id).bind(&candidate.document_id).bind(&candidate.space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let expected = (
        candidate.document_title.clone(),
        candidate.workflow_template_name.clone(),
        candidate.current_stage_id.clone(),
        candidate.note.clone(),
        candidate.delegated_by.clone(),
        candidate.delegated_at.clone(),
    );
    if current != Some(expected) {
        return Err(AppError::Conflict(
            "Workflow deadline source changed; refresh candidates".into(),
        ));
    }
    Ok(())
}
struct VisibleDeadlineCandidate {
    raw: WorkflowDeadlineEscalationCandidateResponse,
    published_at: Option<String>,
    draft: Option<(String, String)>,
}
impl VisibleDeadlineCandidate {
    fn response(&self) -> WorkflowDeadlineEscalationCandidateResponse {
        let mut response = self.raw.clone();
        if let Some((_, title)) = &self.draft {
            response.document_title = title.clone();
        }
        response
    }
}
async fn select_deadline_title(
    conn: &mut AnyConnection,
    actor: DeadlineActor<'_>,
    raw: WorkflowDeadlineEscalationCandidateResponse,
) -> Result<VisibleDeadlineCandidate, AppError> {
    let published:Option<(Option<String>,)>=sqlx::query_as("SELECT published_at FROM documents WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL").bind(&raw.document_id).bind(&raw.space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let published_at = published
        .ok_or_else(|| AppError::NotFound("Workflow deadline page unavailable".into()))?
        .0;
    let draft = if published_at.is_none() {
        let selected:Option<(String,String)>=sqlx::query_as("SELECT dd.id,dd.title FROM document_drafts dd WHERE dd.document_id=$1 AND dd.space_id=$2 AND (dd.owner_id=$3 OR EXISTS(SELECT 1 FROM document_draft_access dda WHERE dda.draft_id=dd.id AND dda.user_id=$3 AND dda.role IN ('viewer','editor'))) ORDER BY CASE WHEN dd.owner_id=$3 THEN 0 ELSE 1 END,dd.updated_at DESC,dd.id ASC LIMIT 1").bind(&raw.document_id).bind(&raw.space_id).bind(actor.user_id()).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        let draft = selected
            .ok_or_else(|| AppError::NotFound("Workflow deadline page unavailable".into()))?;
        super::access::resolve_draft_access(
            conn,
            actor.principal()?,
            &raw.space_id,
            &raw.document_id,
            &draft.0,
            super::access::DraftOperation::View,
            super::access::Purpose::Routine,
        )
        .await?;
        Some(draft)
    } else {
        None
    };
    Ok(VisibleDeadlineCandidate {
        raw,
        published_at,
        draft,
    })
}
async fn visible_deadline_candidates(
    state: &AppState,
    actor: DeadlineActor<'_>,
    before: NaiveDate,
) -> Result<Vec<VisibleDeadlineCandidate>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    actor.require_identity(&mut conn).await?;
    drop(conn);
    let candidates = fetch_workflow_deadline_escalation_candidates(state, before).await?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let mut visible = Vec::new();
    for candidate in candidates {
        match deadline_page_view(&mut conn, actor, &candidate).await {
            Ok(()) => {}
            Err(AppError::NotFound(_) | AppError::Forbidden(_)) => continue,
            Err(error) => return Err(error),
        }
        visible.push(select_deadline_title(&mut conn, actor, candidate).await?);
    }
    final_deadline_candidates(&mut conn, actor, &visible).await?;
    Ok(visible)
}
async fn final_deadline_candidates(
    conn: &mut AnyConnection,
    actor: DeadlineActor<'_>,
    candidates: &[VisibleDeadlineCandidate],
) -> Result<(), AppError> {
    #[cfg(test)]
    deadline_read_pause().await;
    actor.require_identity(conn).await?;
    for candidate in candidates {
        let raw = &candidate.raw;
        deadline_page_view(conn, actor, raw).await?;
        if let Some((id, title)) = &candidate.draft {
            super::access::resolve_draft_access(
                conn,
                actor.principal()?,
                &raw.space_id,
                &raw.document_id,
                id,
                super::access::DraftOperation::View,
                super::access::Purpose::Routine,
            )
            .await?;
            let current: Option<String> = sqlx::query_scalar(
                "SELECT title FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",
            )
            .bind(id)
            .bind(&raw.document_id)
            .bind(&raw.space_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
            if current.as_deref() != Some(title.as_str()) {
                return Err(AppError::Conflict(
                    "Workflow deadline draft changed; refresh".into(),
                ));
            }
        }
        deadline_candidate_source(conn, raw).await?;
        let current:Option<(Option<String>,)>=sqlx::query_as("SELECT published_at FROM documents WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL").bind(&raw.document_id).bind(&raw.space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if current != Some((candidate.published_at.clone(),)) {
            return Err(AppError::Conflict(
                "Workflow deadline publication changed; refresh".into(),
            ));
        }
        #[cfg(test)]
        deadline_final_resource_pause().await;
        if let Some((id, _)) = &candidate.draft {
            super::access::resolve_draft_access(
                conn,
                actor.principal()?,
                &raw.space_id,
                &raw.document_id,
                id,
                super::access::DraftOperation::View,
                super::access::Purpose::Routine,
            )
            .await?;
        }
    }
    #[cfg(test)]
    deadline_final_identity_pause().await;
    actor.require_identity(conn).await?;
    Ok(())
}
#[cfg(test)]
thread_local! {static DEADLINE_READ_PAUSE: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const {std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn deadline_read_pause() {
    let hook = DEADLINE_READ_PAUSE.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}

#[cfg(test)]
thread_local! {
 static DEADLINE_FINAL_RESOURCE_PAUSE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const {std::cell::RefCell::new(None)};
 static DEADLINE_FINAL_IDENTITY_PAUSE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const {std::cell::RefCell::new(None)};
}
#[cfg(test)]
async fn deadline_final_resource_pause() {
    let hook = DEADLINE_FINAL_RESOURCE_PAUSE.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}
#[cfg(test)]
async fn deadline_final_identity_pause() {
    let hook = DEADLINE_FINAL_IDENTITY_PAUSE.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}

async fn list_workflow_deadline_escalations(
    State(state): State<AppState>,
    admin: AdminUser,
    Query(query): Query<WorkflowDeadlineEscalationQuery>,
) -> Result<impl IntoResponse, AppError> {
    let before = parse_deadline_scan_before(query.before.as_deref())?;
    let auth = deadline_http_auth(&admin);
    let candidates = visible_deadline_candidates(
        &state,
        DeadlineActor::Http {
            admin: &admin,
            auth: &auth,
        },
        before,
    )
    .await?;

    Ok(Json(
        candidates
            .iter()
            .map(VisibleDeadlineCandidate::response)
            .collect::<Vec<_>>(),
    ))
}

async fn fetch_workflow_deadline_escalation_candidates(
    state: &AppState,
    before: NaiveDate,
) -> Result<Vec<WorkflowDeadlineEscalationCandidateResponse>, AppError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
        ),
    >(
        "SELECT dwe.id, dwe.document_workflow_id, dwe.document_id, d.title, d.space_id, \
                wt.name, dw.current_stage_id, dwe.note, dwe.created_by, dwe.created_at \
         FROM document_workflow_events dwe \
         JOIN document_workflows dw ON dw.id = dwe.document_workflow_id \
         JOIN documents d ON d.id = dwe.document_id \
         JOIN workflow_templates wt ON wt.id = dw.workflow_template_id \
         WHERE dw.status = 'active' \
           AND dwe.action = 'delegate' \
           AND wt.space_id = d.space_id AND d.deleted_at IS NULL AND d.archived_at IS NULL \
           AND NOT EXISTS ( \
             SELECT 1 FROM document_workflow_events latest \
             WHERE latest.document_workflow_id = dwe.document_workflow_id \
               AND latest.action IN ('delegate', 'escalate') \
               AND (latest.created_at > dwe.created_at OR (latest.created_at = dwe.created_at AND latest.id > dwe.id)) \
           ) \
         ORDER BY dwe.created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let candidates = rows
        .into_iter()
        .filter_map(
            |(
                event_id,
                document_workflow_id,
                document_id,
                document_title,
                space_id,
                workflow_template_name,
                current_stage_id,
                note,
                delegated_by,
                delegated_at,
            )| {
                let note = note?;
                let due_date = extract_workflow_due_date(&note)?;
                if due_date >= before {
                    return None;
                }
                Some(WorkflowDeadlineEscalationCandidateResponse {
                    event_id,
                    document_workflow_id,
                    document_id,
                    document_title,
                    space_id,
                    workflow_template_name,
                    current_stage_id,
                    target: extract_workflow_handoff_target(&note).unwrap_or_default(),
                    due_date: due_date.format("%Y-%m-%d").to_string(),
                    note,
                    delegated_by,
                    delegated_at,
                })
            },
        )
        .collect::<Vec<_>>();

    Ok(candidates)
}

async fn run_workflow_deadline_escalations(
    State(state): State<AppState>,
    admin: AdminUser,
    Query(query): Query<WorkflowDeadlineEscalationRunQuery>,
) -> Result<impl IntoResponse, AppError> {
    let before = parse_deadline_scan_before(query.before.as_deref())?;
    let target = normalize_deadline_escalation_target(query.target.as_deref())?;
    let dry_run = query.dry_run.unwrap_or(false);
    let auth = deadline_http_auth(&admin);
    let actor = DeadlineActor::Http {
        admin: &admin,
        auth: &auth,
    };
    let candidates = visible_deadline_candidates(&state, actor, before).await?;

    if dry_run {
        return Ok(Json(WorkflowDeadlineEscalationRunResponse {
            scanned: candidates.len(),
            escalated: 0,
            dry_run,
            target,
            candidates: candidates
                .iter()
                .map(VisibleDeadlineCandidate::response)
                .collect(),
        }));
    }

    let result = run_deadline_escalations(&state, actor, &target, before).await?;
    Ok(Json(result))
}

fn deadline_escalation_event_id(workflow_id: &str, delegation_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"starlane-workflow-deadline-v1\0");
    hash.update(workflow_id.as_bytes());
    hash.update(b"\0");
    hash.update(delegation_id.as_bytes());
    let bytes = hash.finalize();
    Uuid::from_slice(&bytes[..16])
        .expect("16-byte deadline identity")
        .to_string()
}

async fn commit_deadline_escalation_candidate(
    state: &AppState,
    actor: DeadlineActor<'_>,
    target: &str,
    before: NaiveDate,
    candidate: &WorkflowDeadlineEscalationCandidateResponse,
) -> Result<Option<(DocumentWorkflowResponse, String)>, AppError> {
    let actor_id = actor.user_id();
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&candidate.space_id]).await?;
    actor.require_identity(&mut tx).await?;
    require_current_page_edit(
        &mut tx,
        actor.principal()?,
        &candidate.space_id,
        &candidate.document_id,
    )
    .await?;
    let Some(workflow) =
        fetch_document_workflow_on_connection(&mut tx, &candidate.space_id, &candidate.document_id)
            .await?
    else {
        return Ok(None);
    };
    let latest: Option<(String, String, Option<String>)> = sqlx::query_as("SELECT id, action, note FROM document_workflow_events WHERE document_workflow_id = $1 AND document_id = $2 AND action IN ('delegate', 'escalate') ORDER BY created_at DESC, id DESC LIMIT 1")
            .bind(&workflow.id).bind(&candidate.document_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    let Some((latest_id, action, latest_note)) = latest else {
        return Ok(None);
    };
    if workflow.id != candidate.document_workflow_id
        || workflow.status != "active"
        || workflow.current_stage_id != candidate.current_stage_id
        || latest_id != candidate.event_id
        || action != "delegate"
        || latest_note
            .as_deref()
            .and_then(extract_workflow_due_date)
            .is_none_or(|due| due >= before)
    {
        return Ok(None);
    }
    let event_id = deadline_escalation_event_id(&workflow.id, &candidate.event_id);
    let existing: Option<(String, String, String)> = sqlx::query_as("SELECT document_workflow_id, document_id, action FROM document_workflow_events WHERE id = $1")
        .bind(&event_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if let Some((existing_workflow, existing_document, existing_action)) = existing {
        if existing_workflow != workflow.id
            || existing_document != workflow.document_id
            || existing_action != "escalate"
        {
            return Err(AppError::Conflict(
                "Workflow event identity is already in use".into(),
            ));
        }
        return Ok(None);
    }
    deadline_candidate_source(&mut tx, candidate).await?;
    let source = serde_json::to_value(&workflow).map_err(AppError::internal)?;
    let note = format_deadline_escalation_note(&target, candidate);
    let event_at = chrono::Utc::now().to_rfc3339();
    insert_workflow_event_on_connection(
        &mut tx,
        &workflow,
        None,
        &workflow.current_stage_id,
        "escalate",
        Some(&note),
        actor_id,
        Some(&event_id),
        None,
        Some(&event_at),
        false,
    )
    .await?;
    actor.require_identity(&mut tx).await?;
    require_current_page_edit(
        &mut tx,
        actor.principal()?,
        &candidate.space_id,
        &candidate.document_id,
    )
    .await?;
    let current =
        fetch_document_workflow_on_connection(&mut tx, &candidate.space_id, &candidate.document_id)
            .await?;
    if current
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(AppError::internal)?
        != Some(source)
    {
        return Err(AppError::Conflict(
            "Workflow deadline source changed; retry".into(),
        ));
    }
    deadline_candidate_source(&mut tx, candidate).await?;
    let event:Option<(String,String,String,Option<String>,String,String,String,String)>=sqlx::query_as("SELECT document_workflow_id,document_id,action,from_stage_id,to_stage_id,note,created_by,created_at FROM document_workflow_events WHERE id=$1").bind(&event_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    let latest:Option<String>=sqlx::query_scalar("SELECT id FROM document_workflow_events WHERE document_workflow_id=$1 AND document_id=$2 AND action IN ('delegate','escalate') ORDER BY created_at DESC,id DESC LIMIT 1").bind(&workflow.id).bind(&workflow.document_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if event
        != Some((
            workflow.id.clone(),
            workflow.document_id.clone(),
            "escalate".into(),
            None,
            workflow.current_stage_id.clone(),
            note.clone(),
            actor_id.into(),
            event_at,
        ))
        || latest.as_deref() != Some(event_id.as_str())
    {
        return Err(AppError::Conflict(
            "Workflow deadline event changed; retry".into(),
        ));
    }
    actor.require_identity(&mut tx).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(Some((workflow, note)))
}

/// Internal worker entry point; caller is the configured trusted job user,
/// never an HTTP fallback for missing sessions or a service identity.
pub async fn run_workflow_deadline_escalations_for_actor(
    state: &AppState,
    actor_id: &str,
    target: &str,
    before: NaiveDate,
) -> Result<WorkflowDeadlineEscalationRunResponse, AppError> {
    run_deadline_escalations(
        state,
        DeadlineActor::BackgroundUser(actor_id),
        target,
        before,
    )
    .await
}

async fn run_deadline_escalations(
    state: &AppState,
    actor: DeadlineActor<'_>,
    target: &str,
    before: NaiveDate,
) -> Result<WorkflowDeadlineEscalationRunResponse, AppError> {
    let actor_id = actor.user_id();
    let target = normalize_deadline_escalation_target(Some(target))?;
    let candidates = visible_deadline_candidates(state, actor, before).await?;

    let mut escalated = 0;
    for selected in &candidates {
        let candidate = &selected.raw;
        let Some((workflow, note)) =
            commit_deadline_escalation_candidate(state, actor, &target, before, candidate).await?
        else {
            continue;
        };
        escalated += 1;
        if let Err(error) = notify_workflow_handoff_targets(
            state,
            &candidate.space_id,
            &workflow,
            "escalate",
            &note,
            actor_id,
        )
        .await
        {
            tracing::warn!(error = ?error, "Workflow committed; notification delivery failed");
        }
        crate::audit::log_action(
            &state.db,
            Some(actor_id),
            None,
            "document.workflow_deadline_escalate",
            Some("document"),
            Some(&candidate.document_id),
        )
        .await;
    }

    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    final_deadline_candidates(&mut conn, actor, &candidates).await?;
    Ok(WorkflowDeadlineEscalationRunResponse {
        scanned: candidates.len(),
        escalated,
        dry_run: false,
        target,
        candidates: candidates
            .iter()
            .map(VisibleDeadlineCandidate::response)
            .collect(),
    })
}

async fn fetch_document_workflow(
    state: &AppState,
    space_id: &str,
    document_id: &str,
) -> Result<Option<DocumentWorkflowResponse>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    fetch_document_workflow_on_connection(&mut conn, space_id, document_id).await
}

fn validate_stored_workflow(stages: &str, approvers: &str) -> Result<(), AppError> {
    let stages: Vec<WorkflowStage> = serde_json::from_str(stages)
        .map_err(|_| AppError::Conflict("Workflow stages are invalid".into()))?;
    let approvers: Vec<WorkflowApprover> = serde_json::from_str(approvers)
        .map_err(|_| AppError::Conflict("Workflow approvers are invalid".into()))?;
    let ids: HashSet<&str> = stages
        .iter()
        .filter_map(|stage| stage.id.as_deref())
        .collect();
    if !(2..=12).contains(&stages.len())
        || ids.len() != stages.len()
        || stages.iter().any(|stage| {
            stage.name.trim().is_empty()
                || stage.id.as_deref().is_none_or(|id| id.trim().is_empty())
        })
        || approvers.iter().any(|approver| {
            !matches!(approver.approver_type.as_str(), "user" | "role")
                || approver.approver_id.trim().is_empty()
                || approver
                    .stage_id
                    .as_deref()
                    .is_some_and(|id| !ids.contains(id))
        })
    {
        return Err(AppError::Conflict("Workflow definition is invalid".into()));
    }
    Ok(())
}

async fn fetch_document_workflow_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    document_id: &str,
) -> Result<Option<DocumentWorkflowResponse>, AppError> {
    let inconsistent: Option<i64> = sqlx::query_scalar("SELECT 1 FROM document_workflows dw JOIN workflow_templates wt ON wt.id = dw.workflow_template_id WHERE dw.document_id = $1 AND (wt.space_id <> $2 OR EXISTS (SELECT 1 FROM document_workflow_events e WHERE e.document_workflow_id = dw.id AND e.document_id <> dw.document_id)) LIMIT 1")
        .bind(document_id).bind(space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if inconsistent.is_some() {
        return Err(AppError::Conflict(
            "Workflow association is inconsistent".into(),
        ));
    }
    sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    >(
        "SELECT dw.id, dw.document_id, dw.workflow_template_id, dw.current_stage_id, dw.status, \
                dw.created_by, dw.created_at, dw.updated_at, wt.name, wt.stages, wt.approvers \
         FROM document_workflows dw \
         JOIN workflow_templates wt ON wt.id = dw.workflow_template_id \
         JOIN documents d ON d.id = dw.document_id \
         WHERE dw.document_id = $1 AND wt.space_id = $2 AND d.space_id = $2",
    )
    .bind(document_id)
    .bind(space_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)
    .and_then(|row| {
        row.map(|row| {
            validate_stored_workflow(&row.9, &row.10)?;
            let workflow = document_workflow_response_from_row(row);
            if !workflow
                .stages
                .iter()
                .any(|stage| stage.id.as_deref() == Some(&workflow.current_stage_id))
            {
                return Err(AppError::Conflict(
                    "Workflow current stage is invalid".into(),
                ));
            }
            Ok(workflow)
        })
        .transpose()
    })
}

async fn insert_workflow_event_on_connection(
    conn: &mut AnyConnection,
    workflow: &DocumentWorkflowResponse,
    from_stage_id: Option<&str>,
    to_stage_id: &str,
    action: &str,
    note: Option<&str>,
    user_id: &str,
    source_id: Option<&str>,
    created_by: Option<&str>,
    created_at: Option<&str>,
    replay: bool,
) -> Result<(), AppError> {
    if !workflow
        .stages
        .iter()
        .any(|stage| stage.id.as_deref() == Some(to_stage_id))
        || from_stage_id.is_some_and(|id| {
            !workflow
                .stages
                .iter()
                .any(|stage| stage.id.as_deref() == Some(id))
        })
    {
        return Err(AppError::BadRequest(
            "Workflow event stage must exist".into(),
        ));
    }
    let event_id = workflow_mutation_id(source_id, replay)?;
    let match_created_at = created_at.is_some();
    let created_at = super::routes::import_timestamp_or_now(created_at)?;
    let created_by = if source_id.is_some() {
        match created_by
            .map(|value| if replay { value } else { value.trim() })
            .filter(|value| !value.trim().is_empty())
        {
            Some(candidate) => {
                let existing_user: Option<(String,)> =
                    sqlx::query_as("SELECT id FROM users WHERE id = $1")
                        .bind(candidate)
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(AppError::internal)?;
                existing_user.map(|row| row.0).ok_or_else(|| {
                    AppError::BadRequest("Workflow event actor is unavailable".into())
                })?
            }
            None => user_id.to_string(),
        }
    } else {
        user_id.to_string()
    };
    if replay {
        let existing:Option<(String,String,Option<String>,String,String,Option<String>,String,String)>=sqlx::query_as("SELECT document_workflow_id,document_id,from_stage_id,to_stage_id,action,note,created_by,created_at FROM document_workflow_events WHERE id=$1").bind(&event_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if let Some((wid, doc, from, to, stored_action, stored_note, actor, time)) = existing {
            let same_time = chrono::DateTime::parse_from_rfc3339(&time)
                .ok()
                .zip(chrono::DateTime::parse_from_rfc3339(&created_at).ok())
                .is_some_and(|(a, b)| a == b);
            if wid != workflow.id
                || doc != workflow.document_id
                || from.as_deref() != from_stage_id
                || to != to_stage_id
                || stored_action != action
                || stored_note.as_deref() != note
                || actor != created_by
                || (match_created_at && !same_time)
            {
                return Err(AppError::Conflict(
                    "Workflow event import identity conflicts with canonical history".into(),
                ));
            }
            return Ok(());
        }
    }
    let written=sqlx::query(
        "INSERT INTO document_workflow_events \
             (id, document_workflow_id, document_id, from_stage_id, to_stage_id, action, note, created_by, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(event_id)
    .bind(&workflow.id)
    .bind(&workflow.document_id)
    .bind(from_stage_id)
    .bind(to_stage_id)
    .bind(action)
    .bind(note)
    .bind(&created_by)
    .bind(&created_at)
    .execute(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    if written.rows_affected() != 1 {
        return Err(AppError::Conflict("Workflow write did not apply".into()));
    }

    Ok(())
}

fn workflow_http_actor(auth: &AuthUser) -> Result<&str, AppError> {
    if matches!(
        super::access::principal_from_auth(auth)?,
        super::access::Principal::ServiceKey { .. }
    ) {
        return Err(AppError::Unauthorized(
            "Workflow principal unavailable".into(),
        ));
    }
    Ok(auth.claims.sub.as_str())
}

async fn require_workflow_page_view(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    document: &str,
) -> Result<super::access::SpaceAccess, AppError> {
    Ok(super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        document,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?
    .space)
}
async fn require_workflow_page_edit(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    document: &str,
) -> Result<super::access::SpaceAccess, AppError> {
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL").bind(document).bind(space).execute(&mut *conn).await.map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Document unavailable".into()));
    }
    Ok(super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        document,
        super::access::PageOperation::Edit,
        super::access::Purpose::Routine,
    )
    .await?
    .space)
}
async fn require_workflow_template_mutation(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<super::access::SpaceAccess, AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::ManageTemplates)
        || !access.allows(super::access::Permission::ManageSpace)
    {
        return Err(AppError::NotFound("Workflow template unavailable".into()));
    }
    Ok(access)
}
async fn require_workflow_template_view(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<(), AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::View) {
        return Err(AppError::NotFound("Workflow template unavailable".into()));
    }
    Ok(())
}

async fn list_workflow_templates(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_workflow_template_view(&mut conn, &auth, &space_id).await?;

    let templates = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
        "SELECT id, space_id, name, description, stages, approvers, created_by, created_at, updated_at \
         FROM workflow_templates WHERE space_id = $1 ORDER BY name",
    )
    .bind(&space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?
    .into_iter()
    .map(row_to_workflow_template)
    .collect::<Vec<_>>();

    require_workflow_template_view(&mut conn, &auth, &space_id).await?;
    Ok(Json(templates))
}

async fn get_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_workflow_page_view(&mut conn, &auth, &space_id, &document_id).await?;

    let workflow =
        fetch_document_workflow_on_connection(&mut conn, &space_id, &document_id).await?;
    require_workflow_page_view(&mut conn, &auth, &space_id, &document_id).await?;
    Ok(Json(workflow))
}

/// Replay imports use legacy string identities; operation authority remains the
/// same guarded domain policy as REST. Transport adapters validate bindings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkflowMutationOrigin {
    Rest,
    Replay,
}
#[derive(Clone)]
pub(crate) struct WorkflowReplayImport {
    pub id: String,
    pub template_id: Option<String>,
    pub status: Option<String>,
    pub created_by: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub event: Option<WorkflowEventImport>,
}
#[derive(Clone)]
pub(crate) struct WorkflowEventImport {
    pub id: String,
    pub action: String,
    pub from_stage_id: Option<String>,
    pub to_stage_id: String,
    pub note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: Option<String>,
}
fn workflow_mutation_id(source: Option<&str>, replay: bool) -> Result<String, AppError> {
    if !replay {
        return super::routes::import_id_or_uuid(source);
    }
    match source {
        Some(id) if !id.trim().is_empty() => Ok(id.to_owned()),
        Some(_) => Err(AppError::BadRequest("Workflow identity is required".into())),
        None => Ok(Uuid::new_v4().to_string()),
    }
}
async fn workflow_import_actor(
    conn: &mut AnyConnection,
    candidate: Option<&str>,
    user: &str,
) -> Result<String, AppError> {
    let id = candidate.unwrap_or(user);
    if id.trim().is_empty() {
        return Err(AppError::BadRequest("Workflow actor is unavailable".into()));
    }
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE id=$1")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(AppError::internal)?;
    exists.ok_or_else(|| AppError::BadRequest("Workflow actor is unavailable".into()))
}
async fn insert_imported_workflow_event(
    conn: &mut AnyConnection,
    workflow: &DocumentWorkflowResponse,
    event: &WorkflowEventImport,
    user: &str,
) -> Result<(), AppError> {
    if !matches!(
        event.action.as_str(),
        "enable" | "disable" | "transition" | "delegate" | "escalate"
    ) {
        return Err(AppError::BadRequest(
            "Workflow event action is not supported".into(),
        ));
    }
    insert_workflow_event_on_connection(
        conn,
        workflow,
        event.from_stage_id.as_deref(),
        &event.to_stage_id,
        &event.action,
        event.note.as_deref(),
        user,
        Some(&event.id),
        event.created_by.as_deref(),
        event.created_at.as_deref(),
        true,
    )
    .await
}

pub(crate) async fn mutate_enable_workflow(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    document_id: String,
    req: EnableDocumentWorkflowRequest,
    replay: Option<WorkflowReplayImport>,
) -> Result<DocumentWorkflowResponse, AppError> {
    let user_id = workflow_http_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;

    let template_id = if replay.is_some() {
        req.template_id.as_str()
    } else {
        req.template_id.trim()
    };
    let template = sqlx::query_as::<_, (String, String, String)>(
        "SELECT name, stages, approvers FROM workflow_templates WHERE id = $1 AND space_id = $2",
    )
    .bind(template_id)
    .bind(&space_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::internal)?
    .ok_or_else(|| AppError::NotFound("Workflow template not found".into()))?;

    validate_stored_workflow(&template.1, &template.2)?;
    let stages: Vec<WorkflowStage> = parse_json_vec(template.1.clone());
    let stage_id = req
        .stage_id
        .as_deref()
        .map(|id| if replay.is_some() { id } else { id.trim() })
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| stages.first().and_then(|stage| stage.id.clone()))
        .ok_or_else(|| AppError::BadRequest("Workflow template has no stages".into()))?;

    if !stages
        .iter()
        .any(|stage| stage.id.as_deref() == Some(stage_id.as_str()))
    {
        return Err(AppError::BadRequest("Workflow stage not found".into()));
    }

    let existing: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, workflow_template_id, created_by, created_at FROM document_workflows WHERE document_id = $1",
    )
    .bind(&document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if let Some((existing_id, existing_template, author, time)) = &existing {
        if let Some(import) = &replay {
            let time_matches = import.created_at.as_deref().is_none_or(|requested| {
                chrono::DateTime::parse_from_rfc3339(requested)
                    .ok()
                    .zip(chrono::DateTime::parse_from_rfc3339(time).ok())
                    .is_some_and(|(a, b)| a == b)
            });
            if import
                .created_by
                .as_deref()
                .is_some_and(|requested| requested != author)
                || !time_matches
            {
                return Err(AppError::Conflict(
                    "Workflow import creation metadata conflicts with canonical state".into(),
                ));
            }
        }
        if req.source_id.as_deref().is_some_and(|id| {
            if replay.is_some() {
                id != existing_id
            } else {
                id.trim() != existing_id
            }
        }) {
            return Err(AppError::Conflict(
                "Workflow import ID does not match existing workflow".into(),
            ));
        }
        if existing_template != template_id {
            return Err(AppError::Conflict("Preserve the existing workflow binding and history; reviewed replacement is required".into()));
        }
    }
    let workflow_id = workflow_mutation_id(req.source_id.as_deref(), replay.is_some())?;
    let creator = workflow_import_actor(
        &mut tx,
        replay.as_ref().and_then(|i| i.created_by.as_deref()),
        user_id,
    )
    .await?;
    let status = replay
        .as_ref()
        .and_then(|i| i.status.as_deref())
        .unwrap_or("active");
    if !matches!(status, "active" | "completed" | "cancelled") {
        return Err(AppError::BadRequest("Workflow status is invalid".into()));
    }
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    let written=sqlx::query(
        "INSERT INTO document_workflows \
             (id, document_id, workflow_template_id, current_stage_id, status, created_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $8, $5, $6, $7) \
         ON CONFLICT(document_id) DO UPDATE SET \
             workflow_template_id = excluded.workflow_template_id, \
             current_stage_id = excluded.current_stage_id, \
             status = excluded.status, \
             updated_at = excluded.updated_at",
    )
    .bind(&workflow_id)
    .bind(&document_id)
    .bind(template_id)
    .bind(&stage_id)
    .bind(&creator)
    .bind(&created_at)
    .bind(&updated_at)
    .bind(status)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if written.rows_affected() != 1 {
        return Err(AppError::Conflict("Workflow write did not apply".into()));
    }

    let workflow = fetch_document_workflow_on_connection(&mut tx, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to load document workflow".into()))?;
    if let Some(import) = &replay {
        if let Some(event) = &import.event {
            if event.action != "enable" || event.to_stage_id != stage_id {
                return Err(AppError::BadRequest(
                    "Workflow event does not match state mutation".into(),
                ));
            }
            insert_imported_workflow_event(&mut tx, &workflow, event, user_id).await?;
        }
    } else {
        insert_workflow_event_on_connection(
            &mut tx,
            &workflow,
            None,
            &stage_id,
            "enable",
            None,
            user_id,
            req.event_source_id.as_deref(),
            req.event_created_by.as_deref(),
            req.event_created_at
                .as_deref()
                .or_else(|| req.created_at.as_deref()),
            false,
        )
        .await?;
    }
    require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;
    tx.commit().await.map_err(AppError::internal)?;
    // Historical replay imports retain their ledger/history without REST notifications.
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "document.workflow_enable",
            Some("document"),
            Some(&document_id),
        )
        .await;

        if let Err(error) =
            notify_workflow_approvers(&state, &space_id, &workflow, &stage_id, user_id).await
        {
            tracing::warn!(error = ?error, "Workflow committed; notification delivery failed");
        }
    }
    Ok(workflow)
}

pub(crate) async fn mutate_transition_workflow(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    document_id: String,
    req: TransitionDocumentWorkflowRequest,
    replay: Option<WorkflowReplayImport>,
) -> Result<DocumentWorkflowResponse, AppError> {
    let user_id = workflow_http_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    let effective_access =
        require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;

    let workflow = fetch_document_workflow_on_connection(&mut tx, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Document workflow not found".into()))?;
    if let Some(import) = &replay {
        if import.id != workflow.id
            || import.template_id.as_deref() != Some(workflow.workflow_template_id.as_str())
        {
            return Err(AppError::Conflict(
                "Workflow import binding differs from canonical state".into(),
            ));
        }
    }
    let stage_id = if replay.is_some() {
        req.stage_id.clone()
    } else {
        req.stage_id.trim().to_string()
    };
    if stage_id.is_empty() {
        return Err(AppError::BadRequest("Workflow stage is required".into()));
    }
    if !workflow
        .stages
        .iter()
        .any(|stage| stage.id.as_deref() == Some(stage_id.as_str()))
    {
        return Err(AppError::BadRequest("Workflow stage not found".into()));
    }
    if !workflow_approver_allows_transition(
        &workflow.approvers,
        &stage_id,
        user_id,
        &effective_access,
    ) {
        return Err(AppError::Forbidden(
            "Workflow transition requires an assigned approver".into(),
        ));
    }

    let mut ordered_stages = workflow.stages.clone();
    ordered_stages.sort_by_key(|stage| stage.sort_order.unwrap_or(0));
    let status =
        if ordered_stages.last().and_then(|stage| stage.id.as_deref()) == Some(stage_id.as_str()) {
            "completed"
        } else {
            "active"
        };

    let updated_at = super::routes::import_timestamp_or_now(
        replay.as_ref().and_then(|i| i.updated_at.as_deref()),
    )?;
    let written=sqlx::query(
        "UPDATE document_workflows SET current_stage_id = $1, status = $2, updated_at = $3 WHERE id = $4",
    )
    .bind(&stage_id)
    .bind(status)
    .bind(&updated_at)
    .bind(&workflow.id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if written.rows_affected() != 1 {
        return Err(AppError::Conflict("Workflow write did not apply".into()));
    }

    if let Some(import) = &replay {
        if let Some(event) = &import.event {
            if event.action != "transition" || event.to_stage_id != stage_id {
                return Err(AppError::BadRequest(
                    "Workflow event does not match state mutation".into(),
                ));
            }
            if event.from_stage_id.as_deref() != Some(workflow.current_stage_id.as_str()) {
                let existing:Option<String>=sqlx::query_scalar("SELECT id FROM document_workflow_events WHERE id=$1 AND document_workflow_id=$2 AND document_id=$3").bind(&event.id).bind(&workflow.id).bind(&workflow.document_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
                if workflow.current_stage_id != stage_id || existing.is_none() {
                    return Err(AppError::Conflict(
                        "Workflow transition source stage changed; review queued transition".into(),
                    ));
                }
            }
            insert_imported_workflow_event(&mut tx, &workflow, event, user_id).await?;
        }
    } else {
        insert_workflow_event_on_connection(
            &mut tx,
            &workflow,
            Some(&workflow.current_stage_id),
            &stage_id,
            "transition",
            req.note
                .as_deref()
                .map(str::trim)
                .filter(|note| !note.is_empty()),
            user_id,
            None,
            None,
            None,
            false,
        )
        .await?;
    }

    let current_access = require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;
    let updated = fetch_document_workflow_on_connection(&mut tx, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to load document workflow".into()))?;
    if !workflow_approver_allows_transition(&updated.approvers, &stage_id, user_id, &current_access)
    {
        return Err(AppError::Forbidden(
            "Workflow transition requires an assigned approver".into(),
        ));
    }
    tx.commit().await.map_err(AppError::internal)?;
    // Historical replay imports retain their ledger/history without REST notifications.
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "document.workflow_transition",
            Some("document"),
            Some(&document_id),
        )
        .await;

        if let Err(error) =
            notify_workflow_approvers(&state, &space_id, &updated, &stage_id, user_id).await
        {
            tracing::warn!(error = ?error, "Workflow committed; notification delivery failed");
        }
    }
    Ok(updated)
}

pub(crate) async fn mutate_record_workflow_event(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    document_id: String,
    req: RecordDocumentWorkflowEventRequest,
    replay: Option<WorkflowReplayImport>,
) -> Result<Vec<DocumentWorkflowEventResponse>, AppError> {
    let user_id = workflow_http_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;

    let action = req.action.trim();
    let is_import_event = req.source_id.is_some();
    let allowed_import_action = matches!(
        action,
        "transition" | "enable" | "disable" | "delegate" | "escalate"
    );
    if (!is_import_event && action != "delegate" && action != "escalate")
        || (is_import_event && !allowed_import_action)
    {
        return Err(AppError::BadRequest(
            "Workflow event action is not supported".into(),
        ));
    }
    let note = if replay.is_some() {
        req.note.as_deref()
    } else {
        req.note
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    if !is_import_event && note.is_none() {
        return Err(AppError::BadRequest(
            "Workflow event note is required".into(),
        ));
    }
    if !is_import_event && matches!(action, "delegate" | "escalate") {
        let policy = configured_workflow_reassignment_policy()?;
        validate_workflow_reassignment_policy(
            &policy,
            action,
            note.unwrap_or_default(),
            chrono::Utc::now().date_naive(),
        )?;
    }

    let workflow = fetch_document_workflow_on_connection(&mut tx, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Document workflow not found".into()))?;
    if let Some(import) = &replay {
        if import.id != workflow.id {
            return Err(AppError::Conflict(
                "Workflow import binding differs from canonical state".into(),
            ));
        }
    }
    let to_stage_id = if replay.is_some() {
        req.to_stage_id
            .as_deref()
            .unwrap_or(&workflow.current_stage_id)
    } else {
        req.to_stage_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&workflow.current_stage_id)
    };
    let from_stage_id = if replay.is_some() {
        req.from_stage_id.as_deref()
    } else {
        req.from_stage_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };

    insert_workflow_event_on_connection(
        &mut tx,
        &workflow,
        from_stage_id,
        to_stage_id,
        action,
        note,
        user_id,
        req.source_id.as_deref(),
        req.created_by.as_deref(),
        req.created_at.as_deref(),
        replay.is_some(),
    )
    .await?;

    require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;
    let events = fetch_workflow_events_on_connection(&mut tx, &document_id).await?;
    tx.commit().await.map_err(AppError::internal)?;
    // Historical replay imports retain their ledger/history without REST notifications.
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            match action {
                "delegate" => "document.workflow_delegate",
                "escalate" => "document.workflow_escalate",
                "transition" => "document.workflow_transition_import",
                "enable" => "document.workflow_enable_import",
                "disable" => "document.workflow_disable_import",
                _ => "document.workflow_event_import",
            },
            Some("document"),
            Some(&document_id),
        )
        .await;

        if matches!(action, "delegate" | "escalate") {
            let Some(note) = note else {
                return Ok(events);
            };
            if let Err(error) =
                notify_workflow_handoff_targets(&state, &space_id, &workflow, action, note, user_id)
                    .await
            {
                tracing::warn!(error = ?error, "Workflow committed; notification delivery failed");
            }
        }
    }
    Ok(events)
}

pub(crate) async fn mutate_disable_workflow(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    document_id: String,
    expected_id: Option<String>,
    origin: WorkflowMutationOrigin,
) -> Result<(), AppError> {
    let user_id = workflow_http_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;

    let existing = fetch_document_workflow_on_connection(&mut tx, &space_id, &document_id).await?;
    let raw: Option<String> =
        sqlx::query_scalar("SELECT id FROM document_workflows WHERE document_id=$1")
            .bind(&document_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if raw.is_some() && existing.is_none() {
        return Err(AppError::Conflict(
            "Workflow association is inconsistent".into(),
        ));
    }
    if expected_id
        .as_deref()
        .is_some_and(|id| existing.as_ref().is_none_or(|w| w.id != id))
    {
        return Err(AppError::NotFound("Document workflow unavailable".into()));
    }
    let result = sqlx::query("DELETE FROM document_workflows WHERE document_id = $1")
        .bind(&document_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;

    require_workflow_page_edit(&mut tx, auth, &space_id, &document_id).await?;
    tx.commit().await.map_err(AppError::internal)?;
    if result.rows_affected() > 0 && origin == WorkflowMutationOrigin::Rest {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "document.workflow_disable",
            Some("document"),
            Some(&document_id),
        )
        .await;
    }

    Ok(())
}

pub(crate) async fn mutate_create_template(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    req: WorkflowTemplateRequest,
    replay: Option<WorkflowReplayImport>,
) -> Result<(String, bool), AppError> {
    let user_id = workflow_http_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_workflow_template_mutation(&mut tx, auth, &space_id).await?;
    let (stages, approvers) = normalize_request(&req)?;
    let template_id = workflow_mutation_id(req.source_id.as_deref(), replay.is_some())?;
    let creator = workflow_import_actor(
        &mut tx,
        replay.as_ref().and_then(|i| i.created_by.as_deref()),
        user_id,
    )
    .await?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    let stages_json = serde_json::to_string(&stages).map_err(AppError::internal)?;
    let approvers_json = serde_json::to_string(&approvers).map_err(AppError::internal)?;

    let existing:Option<(String,String,Option<String>,String,String,String,String,String)>=sqlx::query_as("SELECT space_id,name,description,stages,approvers,created_by,created_at,updated_at FROM workflow_templates WHERE id=$1").bind(&template_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if let Some((
        scope,
        name,
        description,
        stored_stages,
        stored_approvers,
        author,
        created,
        updated,
    )) = existing
    {
        let same_json = |a: &str, b: &str| {
            serde_json::from_str::<serde_json::Value>(a)
                .ok()
                .zip(serde_json::from_str::<serde_json::Value>(b).ok())
                .is_some_and(|(a, b)| a == b)
        };
        let same_time = |a: &str, b: &str| {
            chrono::DateTime::parse_from_rfc3339(a)
                .ok()
                .zip(chrono::DateTime::parse_from_rfc3339(b).ok())
                .is_some_and(|(a, b)| a == b)
        };
        if replay.is_none()
            || scope != space_id
            || name != req.name.trim()
            || description != req.description
            || !same_json(&stored_stages, &stages_json)
            || !same_json(&stored_approvers, &approvers_json)
            || replay.as_ref().is_some_and(|i| {
                i.created_by.is_some() && author != creator
                    || i.created_at.is_some() && !same_time(&created, &created_at)
                    || i.updated_at.is_some() && !same_time(&updated, &updated_at)
            })
        {
            return Err(AppError::Conflict(
                "Workflow template import conflicts with canonical metadata".into(),
            ));
        }
        require_workflow_template_mutation(&mut tx, auth, &space_id).await?;
        tx.commit().await.map_err(AppError::internal)?;
        return Ok((template_id, true));
    }
    let written=sqlx::query(
        "INSERT INTO workflow_templates (id, space_id, name, description, stages, approvers, created_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&template_id)
    .bind(&space_id)
    .bind(req.name.trim())
    .bind(&req.description)
    .bind(stages_json)
    .bind(approvers_json)
    .bind(&creator)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if written.rows_affected() != 1 {
        return Err(AppError::Conflict("Workflow write did not apply".into()));
    }

    require_workflow_template_mutation(&mut tx, auth, &space_id).await?;
    tx.commit().await.map_err(AppError::internal)?;
    // Historical replay imports retain their ledger/history without REST notifications.
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "workflow_template.create",
            Some("workflow_template"),
            Some(&template_id),
        )
        .await;
    }
    Ok((template_id, false))
}

pub(crate) async fn mutate_update_template(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    template_id: String,
    req: WorkflowTemplateRequest,
    replay: Option<WorkflowReplayImport>,
) -> Result<(), AppError> {
    let user_id = workflow_http_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_workflow_template_mutation(&mut tx, auth, &space_id).await?;
    let scoped_template: Option<(String,)> =
        sqlx::query_as("SELECT id FROM workflow_templates WHERE id = $1 AND space_id = $2")
            .bind(&template_id)
            .bind(&space_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if scoped_template.is_none() {
        return Err(AppError::NotFound("Workflow template not found".into()));
    }
    let corrupt_consumer: Option<i64> = sqlx::query_scalar("SELECT 1 FROM document_workflows dw LEFT JOIN documents d ON d.id = dw.document_id WHERE dw.workflow_template_id = $1 AND (d.id IS NULL OR d.space_id <> $2) LIMIT 1")
        .bind(&template_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if corrupt_consumer.is_some() {
        return Err(AppError::Conflict(
            "Workflow template page association is inconsistent".into(),
        ));
    }
    let corrupt_history: Option<i64> = sqlx::query_scalar("SELECT 1 FROM document_workflow_events e JOIN document_workflows dw ON dw.id = e.document_workflow_id WHERE dw.workflow_template_id = $1 AND e.document_id <> dw.document_id LIMIT 1").bind(&template_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if corrupt_history.is_some() {
        return Err(AppError::Conflict(
            "Workflow history page association is inconsistent".into(),
        ));
    }
    let bindings: Vec<(String, String, String)> = sqlx::query_as("SELECT dw.document_id, d.space_id, dw.current_stage_id FROM document_workflows dw JOIN documents d ON d.id = dw.document_id WHERE dw.workflow_template_id = $1 ORDER BY dw.document_id")
        .bind(&template_id).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    if bindings.iter().any(|row| row.1 != space_id) {
        return Err(AppError::Conflict(
            "Workflow template has an out-of-space page association".into(),
        ));
    }
    for (document_id, _, _) in &bindings {
        require_workflow_page_edit(&mut tx, auth, &space_id, document_id).await?;
    }
    let (stages, approvers) = normalize_request(&req)?;
    let history: Vec<(Option<String>, String)> = sqlx::query_as("SELECT from_stage_id, to_stage_id FROM document_workflow_events WHERE document_workflow_id IN (SELECT id FROM document_workflows WHERE workflow_template_id = $1)").bind(&template_id).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    let ids: HashSet<&str> = stages
        .iter()
        .filter_map(|stage| stage.id.as_deref())
        .collect();
    if bindings.iter().any(|row| !ids.contains(row.2.as_str()))
        || history.iter().any(|(from, to)| {
            !ids.contains(to.as_str()) || from.as_deref().is_some_and(|id| !ids.contains(id))
        })
    {
        return Err(AppError::Conflict(
            "Workflow stages are referenced by current state or history; preserve their IDs".into(),
        ));
    }
    let stages_json = serde_json::to_string(&stages).map_err(AppError::internal)?;
    let approvers_json = serde_json::to_string(&approvers).map_err(AppError::internal)?;

    let updated_at = super::routes::import_timestamp_or_now(
        replay.as_ref().and_then(|i| i.updated_at.as_deref()),
    )?;
    let result = sqlx::query(
        "UPDATE workflow_templates SET name = $1, description = $2, stages = $3, approvers = $4, \
         updated_at = $5 WHERE id = $6 AND space_id = $7",
    )
    .bind(req.name.trim())
    .bind(&req.description)
    .bind(stages_json)
    .bind(approvers_json)
    .bind(&updated_at)
    .bind(&template_id)
    .bind(&space_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Workflow template not found".into()));
    }

    require_workflow_template_mutation(&mut tx, auth, &space_id).await?;
    for (document_id, _, _) in &bindings {
        require_workflow_page_edit(&mut tx, auth, &space_id, document_id).await?;
    }
    tx.commit().await.map_err(AppError::internal)?;
    // Historical replay imports retain their ledger/history without REST notifications.
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "workflow_template.update",
            Some("workflow_template"),
            Some(&template_id),
        )
        .await;
    }
    Ok(())
}

pub(crate) async fn mutate_delete_template(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    template_id: String,
    replay: Option<WorkflowReplayImport>,
) -> Result<(), AppError> {
    let user_id = workflow_http_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_workflow_template_mutation(&mut tx, auth, &space_id).await?;

    let scoped_template: Option<(String,)> =
        sqlx::query_as("SELECT id FROM workflow_templates WHERE id = $1 AND space_id = $2")
            .bind(&template_id)
            .bind(&space_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if scoped_template.is_none() {
        return Err(AppError::NotFound("Workflow template not found".into()));
    }
    let corrupt_consumer: Option<i64> = sqlx::query_scalar("SELECT 1 FROM document_workflows dw LEFT JOIN documents d ON d.id = dw.document_id WHERE dw.workflow_template_id = $1 AND (d.id IS NULL OR d.space_id <> $2) LIMIT 1")
        .bind(&template_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if corrupt_consumer.is_some() {
        return Err(AppError::Conflict(
            "Workflow template page association is inconsistent".into(),
        ));
    }
    let corrupt_history: Option<i64> = sqlx::query_scalar("SELECT 1 FROM document_workflow_events e JOIN document_workflows dw ON dw.id = e.document_workflow_id WHERE dw.workflow_template_id = $1 AND e.document_id <> dw.document_id LIMIT 1").bind(&template_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if corrupt_history.is_some() {
        return Err(AppError::Conflict(
            "Workflow history page association is inconsistent".into(),
        ));
    }
    let bindings: Vec<(String, String, String)> = sqlx::query_as("SELECT dw.document_id, d.space_id, dw.current_stage_id FROM document_workflows dw JOIN documents d ON d.id = dw.document_id WHERE dw.workflow_template_id = $1 ORDER BY dw.document_id")
        .bind(&template_id).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    if bindings.iter().any(|row| row.1 != space_id) {
        return Err(AppError::Conflict(
            "Workflow template has an out-of-space page association".into(),
        ));
    }
    for (document_id, _, _) in &bindings {
        require_workflow_page_edit(&mut tx, auth, &space_id, document_id).await?;
    }
    let result = sqlx::query("DELETE FROM workflow_templates WHERE id = $1 AND space_id = $2")
        .bind(&template_id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Workflow template not found".into()));
    }

    require_workflow_template_mutation(&mut tx, auth, &space_id).await?;
    for (document_id, _, _) in &bindings {
        require_workflow_page_edit(&mut tx, auth, &space_id, document_id).await?;
    }
    tx.commit().await.map_err(AppError::internal)?;
    // Historical replay imports retain their ledger/history without REST notifications.
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "workflow_template.delete",
            Some("workflow_template"),
            Some(&template_id),
        )
        .await;
    }
    Ok(())
}

async fn enable_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<EnableDocumentWorkflowRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = mutate_enable_workflow(&state, &auth, space_id, document_id, req, None).await?;
    Ok(Json(result))
}

async fn transition_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<TransitionDocumentWorkflowRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result =
        mutate_transition_workflow(&state, &auth, space_id, document_id, req, None).await?;
    Ok(Json(result))
}

async fn record_document_workflow_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<RecordDocumentWorkflowEventRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result =
        mutate_record_workflow_event(&state, &auth, space_id, document_id, req, None).await?;
    Ok(Json(result))
}

async fn fetch_workflow_events_on_connection(
    conn: &mut AnyConnection,
    document_id: &str,
) -> Result<Vec<DocumentWorkflowEventResponse>, AppError> {
    Ok(sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            String,
            String,
        ),
    >(
        "SELECT id, document_workflow_id, document_id, from_stage_id, to_stage_id, action, note, created_by, created_at \
         FROM document_workflow_events WHERE document_id = $1 ORDER BY created_at DESC, id DESC",
    )
    .bind(document_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?
    .into_iter()
    .map(workflow_event_from_row)
    .collect::<Vec<_>>())
}

async fn list_document_workflow_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_workflow_page_view(&mut conn, &auth, &space_id, &document_id).await?;

    let events = fetch_workflow_events_on_connection(&mut conn, &document_id).await?;

    require_workflow_page_view(&mut conn, &auth, &space_id, &document_id).await?;
    Ok(Json(events))
}

async fn disable_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_disable_workflow(
        &state,
        &auth,
        space_id,
        document_id,
        None,
        WorkflowMutationOrigin::Rest,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_workflow_template_view(&mut conn, &auth, &space_id).await?;

    let template = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
        "SELECT id, space_id, name, description, stages, approvers, created_by, created_at, updated_at \
         FROM workflow_templates WHERE id = $1 AND space_id = $2",
    )
    .bind(&template_id)
    .bind(&space_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?
    .map(row_to_workflow_template)
    .ok_or_else(|| AppError::NotFound("Workflow template not found".into()))?;

    require_workflow_template_view(&mut conn, &auth, &space_id).await?;
    Ok(Json(template))
}

async fn create_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<WorkflowTemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (id, _) = mutate_create_template(&state, &auth, space_id, req, None).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id":id}))))
}

async fn update_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
    Json(req): Json<WorkflowTemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_update_template(&state, &auth, space_id, template_id, req, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_delete_template(&state, &auth, space_id, template_id, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::properties::tests::{property_test_state, test_auth as base_test_auth};
    fn test_auth(user: &str) -> AuthUser {
        let mut auth = base_test_auth(user);
        auth.claims.sid = format!("workflow-session-{user}");
        auth
    }
    fn approver_test_access(role: &str) -> super::super::access::SpaceAccess {
        super::super::access::SpaceAccess {
            actor_id: "user-1".into(),
            space_id: "space-a".into(),
            instance_admin: role == "instance_admin",
            roles: vec![super::super::access::AssignedRole {
                id: role.into(),
                source: super::super::access::RoleSource::Direct,
            }],
            permissions: Default::default(),
            recovery_audit_required: None,
        }
    }

    async fn workflow_fixture() -> AppState {
        let state = property_test_state().await;
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES ($1,$2,'test','2099-01-01T00:00:00Z')").bind(format!("workflow-session-{user}")).bind(user).execute(&state.db).await.unwrap();
        }

        let stages = r#"[{"id":"review","name":"Review","sort_order":0},{"id":"done","name":"Done","sort_order":1}]"#;
        sqlx::query("INSERT INTO workflow_templates (id,space_id,name,stages,approvers,created_by) VALUES ('template-a','space-a','Review',$1,'[]','owner-a')").bind(stages).execute(&state.db).await.unwrap();
        state
    }

    fn template_request() -> WorkflowTemplateRequest {
        serde_json::from_str(r#"{"name":"New","stages":[{"id":"review","name":"Review"},{"id":"done","name":"Done"}]}"#).unwrap()
    }
    async fn workflow_domain(state: &AppState) -> Vec<(String, Option<String>)> {
        super::super::documents::tests::move_database_snapshot(state).await
    }
    async fn assign_workflow_global_role(state: &AppState, role: &str, permissions: &str) {
        sqlx::query("INSERT INTO groups(id,name) VALUES ('workflow-group','Workflow group')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES ('workflow-member','workflow-group','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES ($1,$1,$2) ON CONFLICT(id) DO UPDATE SET permissions=$2").bind(role).bind(permissions).execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES ('workflow-group',$1)",
        )
        .bind(role)
        .execute(&state.db)
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn typed_workflow_rejects_missing_expired_sessions_and_service_keys_without_writes() {
        let state = workflow_fixture().await;
        let before = workflow_domain(&state).await;
        let mut absent = test_auth("editor-a");
        absent.claims.sid = "absent-session".into();
        let mut expired = test_auth("editor-a");
        expired.claims.exp = chrono::Utc::now().timestamp();
        let mut service = test_auth("editor-a");
        service.claims.sid = "service:workflow-key".into();
        service.claims.sub = "service:workflow-key".into();
        service.api_key_scopes = Some(vec!["admin".into()]);
        for auth in [absent, expired, service] {
            assert!(mutate_enable_workflow(
                &state,
                &auth,
                "space-a".into(),
                "target-a".into(),
                enable_request(),
                None
            )
            .await
            .is_err());
            assert_eq!(workflow_domain(&state).await, before);
        }
    }
    #[tokio::test]
    async fn typed_workflow_user_key_method_scopes_preserve_write_only_mutations() {
        let state = workflow_fixture().await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES ('workflow-key','editor-a','Workflow','hash','wf','[\"write\"]')").execute(&state.db).await.unwrap();
        let key_auth = |scope: &str| {
            let mut auth = test_auth("editor-a");
            auth.claims.sid = "api-key:workflow-key".into();
            auth.api_key_scopes = Some(vec![scope.into()]);
            auth
        };
        let auth = key_auth("write");
        let enabled = mutate_enable_workflow(
            &state,
            &auth,
            "space-a".into(),
            "target-a".into(),
            enable_request(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(enabled.current_stage_id, "review");
        let transitioned = mutate_transition_workflow(
            &state,
            &auth,
            "space-a".into(),
            "target-a".into(),
            TransitionDocumentWorkflowRequest {
                stage_id: "done".into(),
                note: None,
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(transitioned.status, "completed");
        assert!(get_document_workflow(
            State(state.clone()),
            key_auth("write"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        assert!(list_document_workflow_events(
            State(state.clone()),
            key_auth("write"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='workflow-key'")
            .execute(&state.db)
            .await
            .unwrap();
        let auth = key_auth("read");
        assert!(get_document_workflow(
            State(state.clone()),
            key_auth("read"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_ok());
        let before = workflow_domain(&state).await;
        assert!(mutate_transition_workflow(
            &state,
            &auth,
            "space-a".into(),
            "target-a".into(),
            TransitionDocumentWorkflowRequest {
                stage_id: "review".into(),
                note: None
            },
            None
        )
        .await
        .is_err());
        assert!(mutate_disable_workflow(
            &state,
            &auth,
            "space-a".into(),
            "target-a".into(),
            None,
            WorkflowMutationOrigin::Rest
        )
        .await
        .is_err());
        assert_eq!(workflow_domain(&state).await, before);
    }
    #[tokio::test]
    async fn typed_workflow_late_session_revocation_and_key_scope_downgrade_roll_back_state_history(
    ) {
        for key in [false, true] {
            let state = workflow_fixture().await;
            let mut auth = test_auth("editor-a");
            let trigger = if key {
                sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES ('workflow-key','editor-a','Workflow','hash','wf','[\"write\"]')").execute(&state.db).await.unwrap();
                auth.claims.sid = "api-key:workflow-key".into();
                auth.api_key_scopes = Some(vec!["write".into()]);
                "CREATE TRIGGER workflow_proof_change AFTER INSERT ON document_workflow_events BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='workflow-key'; END"
            } else {
                "CREATE TRIGGER workflow_proof_change AFTER INSERT ON document_workflow_events BEGIN DELETE FROM sessions WHERE id='workflow-session-editor-a'; END"
            };
            sqlx::query(trigger).execute(&state.db).await.unwrap();
            let before = workflow_domain(&state).await;
            assert!(mutate_enable_workflow(
                &state,
                &auth,
                "space-a".into(),
                "target-a".into(),
                enable_request(),
                None
            )
            .await
            .is_err());
            assert_eq!(workflow_domain(&state).await, before);
        }
    }
    #[tokio::test]
    async fn typed_workflow_wait_rechecks_session_revocation_and_jwt_expiry() {
        for expire in [false, true] {
            let state = workflow_fixture().await;
            let mut blocker = state.db.begin().await.unwrap();
            lock_spaces_for_mutation(&mut blocker, &["space-a"])
                .await
                .unwrap();
            let mut auth = test_auth("editor-a");
            if expire {
                auth.claims.exp = chrono::Utc::now().timestamp() + 2;
            }
            let worker = state.clone();
            let task = tokio::spawn(async move {
                mutate_enable_workflow(
                    &worker,
                    &auth,
                    "space-a".into(),
                    "target-a".into(),
                    enable_request(),
                    None,
                )
                .await
            });
            tokio::task::yield_now().await;
            if expire {
                tokio::time::sleep(Duration::from_millis(2100)).await;
            } else {
                sqlx::query("DELETE FROM sessions WHERE id='workflow-session-editor-a'")
                    .execute(&mut *blocker)
                    .await
                    .unwrap();
            }
            blocker.commit().await.unwrap();
            let before = workflow_domain(&state).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(workflow_domain(&state).await, before);
        }
    }
    #[tokio::test]
    async fn typed_workflow_matches_all_assigned_roles_without_global_admin_label_override() {
        for role in ["workflow-secondary", "admin"] {
            let state = workflow_fixture().await;
            enable_fixture(&state).await;
            assign_workflow_global_role(&state, role, r#"["view","edit_content"]"#).await;
            let assigned = if role == "admin" { "owner" } else { role };
            sqlx::query("UPDATE workflow_templates SET approvers=$1 WHERE id='template-a'").bind(format!(r#"[{{"id":"approval","approver_type":"role","approver_id":"{assigned}","stage_id":"done"}}]"#)).execute(&state.db).await.unwrap();
            let before = workflow_domain(&state).await;
            let result = mutate_transition_workflow(
                &state,
                &test_auth("editor-a"),
                "space-a".into(),
                "target-a".into(),
                TransitionDocumentWorkflowRequest {
                    stage_id: "done".into(),
                    note: None,
                },
                None,
            )
            .await;
            if role == "admin" {
                assert!(result.is_err());
                assert_eq!(workflow_domain(&state).await, before);
            } else {
                assert_eq!(result.unwrap().status, "completed");
            }
        }
    }
    #[tokio::test]
    async fn typed_workflow_templates_require_current_manage_templates_and_manage_space() {
        for permissions in [
            r#"["view","edit_content"]"#,
            r#"["view","manage_templates"]"#,
            r#"["view","manage_space"]"#,
        ] {
            let state = workflow_fixture().await;
            sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
                .execute(&state.db)
                .await
                .unwrap();
            assign_workflow_global_role(&state, "workflow-granular", permissions).await;
            let before = workflow_domain(&state).await;
            assert!(mutate_create_template(
                &state,
                &test_auth("editor-a"),
                "space-a".into(),
                template_request(),
                None
            )
            .await
            .is_err());
            assert!(mutate_delete_template(
                &state,
                &test_auth("editor-a"),
                "space-a".into(),
                "template-a".into(),
                None
            )
            .await
            .is_err());
            assert_eq!(workflow_domain(&state).await, before);
        }
    }
    #[tokio::test]
    async fn typed_workflow_final_tuple_lock_rechecks_changed_canonical_approvers() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        sqlx::query("CREATE TRIGGER workflow_final_lock AFTER UPDATE OF id ON documents WHEN EXISTS (SELECT 1 FROM document_workflow_events WHERE action='transition') BEGIN UPDATE workflow_templates SET approvers='[{\"id\":\"last-lock\",\"approver_type\":\"role\",\"approver_id\":\"owner\",\"stage_id\":\"done\"}]' WHERE id='template-a'; END").execute(&state.db).await.unwrap();
        let before = workflow_domain(&state).await;
        assert!(mutate_transition_workflow(
            &state,
            &test_auth("editor-a"),
            "space-a".into(),
            "target-a".into(),
            TransitionDocumentWorkflowRequest {
                stage_id: "done".into(),
                note: None
            },
            None
        )
        .await
        .is_err());
        assert_eq!(workflow_domain(&state).await, before);
    }

    #[tokio::test]
    async fn typed_workflow_private_reads_and_writes_require_participation_even_for_instance_admin()
    {
        for admin in [false, true] {
            let state = workflow_fixture().await;
            enable_fixture(&state).await;
            if admin {
                sqlx::query("UPDATE users SET role='admin' WHERE id='editor-a'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
                .execute(&state.db)
                .await
                .unwrap();
            let before = workflow_domain(&state).await;
            assert!(get_document_workflow(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "target-a".into()))
            )
            .await
            .is_err());
            assert!(list_document_workflow_events(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "target-a".into()))
            )
            .await
            .is_err());
            assert!(mutate_disable_workflow(
                &state,
                &test_auth("editor-a"),
                "space-a".into(),
                "target-a".into(),
                None,
                WorkflowMutationOrigin::Rest
            )
            .await
            .is_err());
            assert_eq!(workflow_domain(&state).await, before);
        }
    }

    fn enable_request() -> EnableDocumentWorkflowRequest {
        EnableDocumentWorkflowRequest {
            template_id: "template-a".into(),
            stage_id: None,
            source_id: None,
            event_source_id: None,
            event_created_by: None,
            event_created_at: None,
            created_at: None,
            updated_at: None,
        }
    }

    async fn enable_fixture(state: &AppState) {
        enable_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(enable_request()),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn workflow_history_failure_rolls_back_enable_and_transition() {
        let state = workflow_fixture().await;
        sqlx::query("CREATE TRIGGER fail_workflow_event BEFORE INSERT ON document_workflow_events BEGIN SELECT RAISE(ABORT, 'injected history failure'); END").execute(&state.db).await.unwrap();
        assert!(enable_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(enable_request())
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflows")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        sqlx::query("DROP TRIGGER fail_workflow_event")
            .execute(&state.db)
            .await
            .unwrap();
        enable_fixture(&state).await;
        sqlx::query("CREATE TRIGGER fail_workflow_event BEFORE INSERT ON document_workflow_events BEGIN SELECT RAISE(ABORT, 'injected history failure'); END").execute(&state.db).await.unwrap();
        assert!(transition_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(TransitionDocumentWorkflowRequest {
                stage_id: "done".into(),
                note: None
            })
        )
        .await
        .is_err());
        let row: (String, String) = sqlx::query_as(
            "SELECT current_stage_id,status FROM document_workflows WHERE document_id='target-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, ("review".into(), "active".into()));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn workflow_enable_requires_scoped_active_unlocked_page_and_private_participation() {
        for (user, document, change) in [
            ("viewer-a", "target-a", None),
            ("editor-a", "target-b", None),
            (
                "editor-a",
                "target-a",
                Some("UPDATE documents SET archived_at='old' WHERE id='target-a'"),
            ),
            (
                "editor-a",
                "target-a",
                Some("UPDATE documents SET deleted_at='old' WHERE id='target-a'"),
            ),
            (
                "editor-a",
                "target-a",
                Some("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'"),
            ),
            (
                "editor-a",
                "target-a",
                Some("UPDATE documents SET published_at=NULL WHERE id='target-a'"),
            ),
            (
                "editor-a",
                "target-a",
                Some("DELETE FROM space_members WHERE user_id='editor-a'"),
            ),
        ] {
            let state = workflow_fixture().await;
            if let Some(sql) = change {
                sqlx::query(sql).execute(&state.db).await.unwrap();
            }
            assert!(enable_document_workflow(
                State(state.clone()),
                test_auth(user),
                Path(("space-a".into(), document.into())),
                Json(enable_request())
            )
            .await
            .is_err());
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflows")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn workflow_event_rejects_invalid_history_stage_actor_and_foreign_workflow_id() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        for (stage, actor) in [("unknown", "editor-a"), ("review", "missing-actor")] {
            let req = RecordDocumentWorkflowEventRequest {
                action: "transition".into(),
                note: None,
                source_id: Some(Uuid::new_v4().to_string()),
                created_by: Some(actor.into()),
                created_at: None,
                from_stage_id: None,
                to_stage_id: Some(stage.into()),
            };
            assert!(record_document_workflow_event(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "target-a".into())),
                Json(req)
            )
            .await
            .is_err());
        }
        let mut req = enable_request();
        req.source_id = Some(Uuid::new_v4().to_string());
        assert!(enable_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(req)
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn template_update_preserves_history_stage_ids_and_delete_rejects_foreign_binding() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        let req: WorkflowTemplateRequest = serde_json::from_value(serde_json::json!({"name":"changed","stages":[{"id":"other","name":"Other"},{"id":"done","name":"Done"}]})).unwrap();
        assert!(update_workflow_template(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "template-a".into())),
            Json(req)
        )
        .await
        .is_err());
        sqlx::query(
            "UPDATE document_workflows SET document_id='target-b' WHERE document_id='target-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(delete_workflow_template(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "template-a".into()))
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn malformed_workflow_definition_cannot_waive_approvers() {
        let stages = r#"[{"id":"a","name":"A"},{"id":"b","name":"B"}]"#;
        assert!(validate_stored_workflow(stages, "{}").is_err());
        assert!(validate_stored_workflow(
            stages,
            r#"[{"approver_type":"user","approver_id":"u","stage_id":"missing"}]"#
        )
        .is_err());
        assert!(
            validate_stored_workflow(r#"[{"id":"a","name":"A"},{"id":"a","name":"B"}]"#, "[]")
                .is_err()
        );
    }

    async fn delegate_fixture(state: &AppState, id: &str, timestamp: &str) {
        let workflow = fetch_document_workflow(state, "space-a", "target-a")
            .await
            .unwrap()
            .unwrap();
        sqlx::query("INSERT INTO document_workflow_events (id,document_workflow_id,document_id,to_stage_id,action,note,created_by,created_at) VALUES ($1,$2,'target-a','review','delegate','Delegated to owner: due 2020-01-01','editor-a',$3)")
            .bind(id).bind(workflow.id).bind(timestamp).execute(&state.db).await.unwrap();
    }

    #[tokio::test]
    async fn deadline_rechecks_superseded_candidates_and_completed_state_then_commits_once() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        let before = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        delegate_fixture(&state, "delegation-a", "2020-01-01T00:00:00Z").await;
        let first = fetch_workflow_deadline_escalation_candidates(&state, before)
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        // Same timestamp, larger stable ID deterministically supersedes the candidate.
        delegate_fixture(&state, "delegation-z", "2020-01-01T00:00:00Z").await;
        assert!(commit_deadline_escalation_candidate(
            &state,
            DeadlineActor::BackgroundUser("editor-a"),
            "owner",
            before,
            &first[0]
        )
        .await
        .unwrap()
        .is_none());
        let fresh = fetch_workflow_deadline_escalation_candidates(&state, before)
            .await
            .unwrap();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].event_id, "delegation-z");
        sqlx::query(
            "UPDATE document_workflows SET status='completed' WHERE document_id='target-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(commit_deadline_escalation_candidate(
            &state,
            DeadlineActor::BackgroundUser("editor-a"),
            "owner",
            before,
            &fresh[0]
        )
        .await
        .unwrap()
        .is_none());
        sqlx::query("UPDATE document_workflows SET status='active' WHERE document_id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(commit_deadline_escalation_candidate(
            &state,
            DeadlineActor::BackgroundUser("editor-a"),
            "owner",
            before,
            &fresh[0]
        )
        .await
        .unwrap()
        .is_some());
        assert!(commit_deadline_escalation_candidate(
            &state,
            DeadlineActor::BackgroundUser("editor-a"),
            "owner",
            before,
            &fresh[0]
        )
        .await
        .unwrap()
        .is_none());
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_workflow_events WHERE action='escalate'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn waited_workflow_mutation_rechecks_revoked_membership() {
        let state = workflow_fixture().await;
        let mut tx = state.db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        let worker_state = state.clone();
        let worker = tokio::spawn(async move {
            enable_document_workflow(
                State(worker_state),
                test_auth("editor-a"),
                Path(("space-a".into(), "target-a".into())),
                Json(enable_request()),
            )
            .await
            .is_err()
        });
        tokio::task::yield_now().await;
        assert!(!worker.is_finished());
        sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflows")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn committed_workflow_returns_success_when_notification_insert_fails() {
        let state = workflow_fixture().await;
        sqlx::query("UPDATE workflow_templates SET approvers=$1 WHERE id='template-a'")
            .bind(r#"[{"approver_type":"user","approver_id":"owner-a","stage_id":"review"}]"#)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER fail_workflow_notification BEFORE INSERT ON notifications BEGIN SELECT RAISE(ABORT, 'injected notification failure'); END").execute(&state.db).await.unwrap();
        enable_fixture(&state).await;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn private_workflow_mutation_requires_owned_or_editor_shared_draft() {
        let state = workflow_fixture().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts (id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ('draft-a','target-a','space-a','owner-a','Private','body','old')").execute(&state.db).await.unwrap();
        assert!(enable_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(enable_request())
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO document_draft_access (draft_id,user_id,role,granted_by) VALUES ('draft-a','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        assert!(enable_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(enable_request())
        )
        .await
        .is_err());
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='draft-a'")
            .execute(&state.db)
            .await
            .unwrap();
        enable_fixture(&state).await;
    }

    #[tokio::test]
    async fn foreign_template_ids_return_not_found_and_corrupt_history_cannot_cascade() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        sqlx::query("INSERT INTO workflow_templates (id,space_id,name,stages,approvers,created_by) SELECT 'foreign-template','space-b',name,stages,approvers,created_by FROM workflow_templates WHERE id='template-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_workflows (id,document_id,workflow_template_id,current_stage_id,status,created_by) VALUES ('foreign-instance','target-b','foreign-template','review','active','owner-a')").execute(&state.db).await.unwrap();
        assert!(matches!(
            delete_workflow_template(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "foreign-template".into()))
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        sqlx::query("UPDATE document_workflow_events SET document_id='target-b'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(delete_workflow_template(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "template-a".into()))
        )
        .await
        .is_err());
        assert!(disable_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn workflow_late_history_deactivation_rolls_back_state_history_and_account_change() {
        let state = workflow_fixture().await;
        sqlx::query("CREATE TRIGGER deactivate_workflow_actor AFTER INSERT ON document_workflow_events BEGIN UPDATE users SET is_active=0 WHERE id=NEW.created_by; END").execute(&state.db).await.unwrap();
        assert!(enable_document_workflow(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(enable_request())
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflows")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let active: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='editor-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(active, 1);
    }

    #[tokio::test]
    async fn deadline_identity_collision_does_not_claim_success_or_overwrite_history() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        let before = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        delegate_fixture(&state, "delegation-a", "2020-01-01T00:00:00Z").await;
        let candidates = fetch_workflow_deadline_escalation_candidates(&state, before)
            .await
            .unwrap();
        let candidate = &candidates[0];
        let id = deadline_escalation_event_id(&candidate.document_workflow_id, &candidate.event_id);
        sqlx::query("INSERT INTO workflow_templates (id,space_id,name,stages,approvers,created_by) SELECT 'foreign-template','space-b',name,stages,approvers,created_by FROM workflow_templates WHERE id='template-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_workflows (id,document_id,workflow_template_id,current_stage_id,status,created_by) VALUES ('foreign-instance','target-b','foreign-template','review','active','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_workflow_events (id,document_workflow_id,document_id,to_stage_id,action,created_by,created_at) VALUES ($1,$2,'target-b','review','transition','owner-a','2019-01-01T00:00:00Z')")
            .bind(&id).bind("foreign-instance").execute(&state.db).await.unwrap();
        assert!(matches!(
            commit_deadline_escalation_candidate(
                &state,
                DeadlineActor::BackgroundUser("editor-a"),
                "owner",
                before,
                candidate
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        let row: (String, String) =
            sqlx::query_as("SELECT document_id,action FROM document_workflow_events WHERE id=$1")
                .bind(id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(row, ("target-b".into(), "transition".into()));
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_workflow_events WHERE action='escalate'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn orphan_workflow_consumer_blocks_template_update_and_delete_without_loss() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE document_workflows SET document_id='missing-page' WHERE document_id='target-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&state.db)
            .await
            .unwrap();
        let req: WorkflowTemplateRequest = serde_json::from_value(serde_json::json!({"name":"changed","stages":[{"id":"review","name":"Review"},{"id":"done","name":"Done"}]})).unwrap();
        assert!(matches!(
            update_workflow_template(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "template-a".into())),
                Json(req)
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            delete_workflow_template(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "template-a".into()))
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        let name: String =
            sqlx::query_scalar("SELECT name FROM workflow_templates WHERE id='template-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(name, "Review");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflows")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn transition_rechecks_assigned_role_after_history_write_and_rolls_back_demotion() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        sqlx::query("UPDATE workflow_templates SET approvers=$1 WHERE id='template-a'")
            .bind(r#"[{"approver_type":"role","approver_id":"owner","stage_id":"done"}]"#)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER demote_workflow_approver AFTER INSERT ON document_workflow_events WHEN NEW.action='transition' BEGIN UPDATE space_members SET role='editor' WHERE user_id=NEW.created_by AND space_id='space-a'; END").execute(&state.db).await.unwrap();
        assert!(matches!(
            transition_document_workflow(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "target-a".into())),
                Json(TransitionDocumentWorkflowRequest {
                    stage_id: "done".into(),
                    note: None
                })
            )
            .await,
            Err(AppError::Forbidden(_))
        ));
        let row: (String, String) = sqlx::query_as(
            "SELECT current_stage_id,status FROM document_workflows WHERE document_id='target-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, ("review".into(), "active".into()));
        let role: String = sqlx::query_scalar(
            "SELECT role FROM space_members WHERE user_id='owner-a' AND space_id='space-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(role, "owner");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn transition_rechecks_changed_approvers_after_history_write() {
        let state = workflow_fixture().await;
        enable_fixture(&state).await;
        let approvers = r#"[{"approver_type":"role","approver_id":"owner","stage_id":"done"}]"#;
        let trigger=format!("CREATE TRIGGER change_workflow_approvers AFTER INSERT ON document_workflow_events WHEN NEW.action='transition' BEGIN UPDATE workflow_templates SET approvers='{approvers}' WHERE id='template-a'; END");
        sqlx::query(&trigger).execute(&state.db).await.unwrap();
        assert!(matches!(
            transition_document_workflow(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "target-a".into())),
                Json(TransitionDocumentWorkflowRequest {
                    stage_id: "done".into(),
                    note: None
                })
            )
            .await,
            Err(AppError::Forbidden(_))
        ));
        let row: (String, String) = sqlx::query_as(
            "SELECT current_stage_id,status FROM document_workflows WHERE document_id='target-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, ("review".into(), "active".into()));
        let approvers: String =
            sqlx::query_scalar("SELECT approvers FROM workflow_templates WHERE id='template-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(approvers, "[]");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_workflow_events")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    fn role_approver(role: &str, stage_id: Option<&str>) -> WorkflowApprover {
        WorkflowApprover {
            id: Some("approver-role".to_string()),
            approver_type: "role".to_string(),
            approver_id: role.to_string(),
            stage_id: stage_id.map(str::to_string),
        }
    }

    fn user_approver(user_id: &str, stage_id: Option<&str>) -> WorkflowApprover {
        WorkflowApprover {
            id: Some("approver-user".to_string()),
            approver_type: "user".to_string(),
            approver_id: user_id.to_string(),
            stage_id: stage_id.map(str::to_string),
        }
    }

    #[test]
    fn workflow_transition_is_open_without_approvers() {
        assert!(workflow_approver_allows_transition(
            &[],
            "review",
            "user-1",
            &approver_test_access("editor"),
        ));
    }

    #[test]
    fn workflow_transition_allows_matching_stage_role() {
        assert!(workflow_approver_allows_transition(
            &[role_approver("owner", Some("review"))],
            "review",
            "user-1",
            &approver_test_access("owner"),
        ));
    }

    #[test]
    fn workflow_transition_blocks_non_matching_stage_role() {
        assert!(!workflow_approver_allows_transition(
            &[role_approver("owner", Some("review"))],
            "review",
            "user-1",
            &approver_test_access("editor"),
        ));
    }

    #[test]
    fn workflow_transition_uses_global_approvers_without_stage_match() {
        assert!(workflow_approver_allows_transition(
            &[role_approver("editor", None)],
            "review",
            "user-1",
            &approver_test_access("editor"),
        ));
    }

    #[test]
    fn workflow_transition_allows_matching_user() {
        assert!(workflow_approver_allows_transition(
            &[user_approver("user-1", Some("review"))],
            "review",
            "user-1",
            &approver_test_access("editor"),
        ));
    }

    #[test]
    fn workflow_transition_allows_instance_admin_override() {
        assert!(workflow_approver_allows_transition(
            &[role_approver("owner", Some("review"))],
            "review",
            "admin-user",
            &approver_test_access("instance_admin"),
        ));
    }

    #[test]
    fn workflow_handoff_target_is_extracted_from_standard_notes() {
        assert_eq!(
            extract_workflow_handoff_target("Delegated to editor@example.com: due 2026-01-12")
                .as_deref(),
            Some("editor@example.com"),
        );
        assert_eq!(
            extract_workflow_handoff_target("Escalated to owner").as_deref(),
            Some("owner"),
        );
        assert_eq!(extract_workflow_handoff_target("Needs review"), None);
    }

    #[test]
    fn workflow_due_date_is_extracted_from_standard_handoff_notes() {
        assert_eq!(
            extract_workflow_due_date("Delegated to editor@example.com: due 2026-01-12"),
            NaiveDate::from_ymd_opt(2026, 1, 12),
        );
        assert_eq!(
            extract_workflow_due_date("Escalated to owner: deadline 2026-02-03"),
            NaiveDate::from_ymd_opt(2026, 2, 3),
        );
        assert_eq!(extract_workflow_due_date("Delegated to owner"), None);
    }

    #[test]
    fn workflow_deadline_scan_before_date_is_validated() {
        assert_eq!(
            parse_deadline_scan_before(Some("2026-01-12")).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 12).unwrap(),
        );
        assert!(parse_deadline_scan_before(Some("01/12/2026")).is_err());
    }

    #[test]
    fn deadline_escalation_target_defaults_to_admin() {
        assert_eq!(normalize_deadline_escalation_target(None).unwrap(), "admin");
        assert_eq!(
            normalize_deadline_escalation_target(Some(" owner ")).unwrap(),
            "owner"
        );
    }

    #[test]
    fn deadline_escalation_note_preserves_prior_target() {
        let candidate = WorkflowDeadlineEscalationCandidateResponse {
            event_id: "event-1".into(),
            document_workflow_id: "workflow-1".into(),
            document_id: "doc-1".into(),
            document_title: "Review me".into(),
            space_id: "space-1".into(),
            workflow_template_name: "Editorial".into(),
            current_stage_id: "review".into(),
            target: "editor@example.com".into(),
            due_date: "2026-01-12".into(),
            note: "Delegated to editor@example.com: due 2026-01-12".into(),
            delegated_by: "owner-1".into(),
            delegated_at: "2026-01-10T00:00:00Z".into(),
        };

        assert_eq!(
            format_deadline_escalation_note("admin", &candidate),
            "Escalated to admin: deadline 2026-01-12 passed after delegation to editor@example.com"
        );
    }

    #[test]
    fn workflow_reassignment_policy_defaults_to_permissive() {
        let policy = WorkflowReassignmentPolicy::default();
        assert!(validate_workflow_reassignment_policy(
            &policy,
            "delegate",
            "Delegated to editor@example.com",
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        )
        .is_ok());
        assert!(validate_workflow_reassignment_policy(
            &policy,
            "escalate",
            "Escalated to admin",
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        )
        .is_ok());
    }

    #[test]
    fn workflow_reassignment_policy_restricts_targets_and_due_dates() {
        let policy = WorkflowReassignmentPolicy {
            allowed_targets: Some(vec!["role:owner".into(), "admin".into()]),
            require_due_date: true,
            max_due_days: Some(14),
            ..WorkflowReassignmentPolicy::default()
        };
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        assert!(validate_workflow_reassignment_policy(
            &policy,
            "delegate",
            "Delegated to owner: due 2026-01-12",
            today,
        )
        .is_ok());
        assert!(validate_workflow_reassignment_policy(
            &policy,
            "delegate",
            "Delegated to editor@example.com: due 2026-01-12",
            today,
        )
        .is_err());
        assert!(validate_workflow_reassignment_policy(
            &policy,
            "escalate",
            "Escalated to admin",
            today,
        )
        .is_err());
        assert!(validate_workflow_reassignment_policy(
            &policy,
            "delegate",
            "Delegated to owner: due 2026-02-01",
            today,
        )
        .is_err());
    }

    #[test]
    fn workflow_reassignment_policy_can_disable_actions() {
        let policy = WorkflowReassignmentPolicy {
            allow_delegate: false,
            allow_escalate: true,
            ..WorkflowReassignmentPolicy::default()
        };
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        assert!(validate_workflow_reassignment_policy(
            &policy,
            "delegate",
            "Delegated to owner",
            today,
        )
        .is_err());
        assert!(validate_workflow_reassignment_policy(
            &policy,
            "escalate",
            "Escalated to admin",
            today,
        )
        .is_ok());
    }    fn notification_workflow(approvers: &str) -> DocumentWorkflowResponse {
        DocumentWorkflowResponse {
            id: "notification-workflow".into(),
            document_id: "target-a".into(),
            workflow_template_id: "template-a".into(),
            workflow_template_name: "Review".into(),
            current_stage_id: "review".into(),
            current_stage_name: "Review".into(),
            status: "active".into(),
            stages: serde_json::from_str(
                r#"[{"id":"review","name":"Review"},{"id":"done","name":"Done"}]"#,
            )
            .unwrap(),
            approvers: serde_json::from_str(approvers).unwrap(),
            created_by: "owner-a".into(),
            created_at: "2026".into(),
            updated_at: "2026".into(),
        }
    }
    async fn notification_role_fixture() -> AppState {
        let state = workflow_fixture().await;
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('actual-admin','actual-admin@example.test','Actual admin','admin')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('notify-space','Space recipients'),('notify-global','Global recipients')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('notify-viewer','notify-space','viewer-a'),('notify-editor','notify-global','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('notify-group','space-a','notify-space','editor')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('approver-global','Admin display label','[\"view\"]')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('notify-global','approver-global')").execute(&state.db).await.unwrap();
        state
    }
    async fn notification_users(state: &AppState) -> Vec<String> {
        sqlx::query_scalar("SELECT user_id FROM notifications ORDER BY user_id")
            .fetch_all(&state.db)
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn workflow_notifications_match_exact_space_and_global_role_union_not_labels() {
        let s = notification_role_fixture().await;
        let workflow = notification_workflow(
            r#"[{"id":"approve-editor","approver_type":"role","approver_id":"editor"},{"id":"approve-global","approver_type":"role","approver_id":"approver-global"}]"#,
        );
        notify_workflow_approvers(&s, "space-a", &workflow, "review", "owner-a")
            .await
            .unwrap();
        assert_eq!(
            notification_users(&s).await,
            vec!["actual-admin", "editor-a", "viewer-a"]
        );
        sqlx::query("DELETE FROM notifications")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM space_group_permissions")
            .execute(&s.db)
            .await
            .unwrap();
        let workflow = notification_workflow(
            r#"[{"id":"approve-editor","approver_type":"role","approver_id":"editor"}]"#,
        );
        notify_workflow_approvers(&s, "space-a", &workflow, "review", "owner-a")
            .await
            .unwrap();
        assert_eq!(
            notification_users(&s).await,
            vec!["actual-admin", "editor-a"]
        );
        sqlx::query("DELETE FROM notifications")
            .execute(&s.db)
            .await
            .unwrap();
        let workflow = notification_workflow(
            r#"[{"id":"approve-owner","approver_type":"role","approver_id":"owner"}]"#,
        );
        notify_workflow_approvers(&s, "space-a", &workflow, "review", "owner-a")
            .await
            .unwrap();
        assert_eq!(notification_users(&s).await, vec!["actual-admin"]);
    }
    #[tokio::test]
    async fn workflow_private_notifications_require_current_participation_and_active_unblocked_account(
    ) {
        let s = notification_role_fixture().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('notify-private','target-a','space-a','editor-a','Private','base')").execute(&s.db).await.unwrap();
        let workflow = notification_workflow(
            r#"[{"id":"approve-editor","approver_type":"role","approver_id":"editor"}]"#,
        );
        notify_workflow_approvers(&s, "space-a", &workflow, "review", "owner-a")
            .await
            .unwrap();
        assert_eq!(notification_users(&s).await, vec!["editor-a"]);
        sqlx::query("DELETE FROM notifications")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('notify-private','viewer-a','viewer','editor-a')").execute(&s.db).await.unwrap();
        notify_workflow_approvers(&s, "space-a", &workflow, "review", "owner-a")
            .await
            .unwrap();
        assert_eq!(notification_users(&s).await, vec!["editor-a", "viewer-a"]);
        sqlx::query("DELETE FROM notifications")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')").execute(&s.db).await.unwrap();
        notify_workflow_approvers(&s, "space-a", &workflow, "review", "owner-a")
            .await
            .unwrap();
        assert!(notification_users(&s).await.is_empty());
    }
    #[tokio::test]
    async fn workflow_handoff_targets_use_current_role_ids_and_page_eligibility() {
        let s = notification_role_fixture().await;
        let workflow = notification_workflow("[]");
        notify_workflow_handoff_targets(
            &s,
            "space-a",
            &workflow,
            "delegate",
            "Delegated to approver-global",
            "owner-a",
        )
        .await
        .unwrap();
        assert_eq!(notification_users(&s).await, vec!["editor-a"]);
        sqlx::query("DELETE FROM notifications")
            .execute(&s.db)
            .await
            .unwrap();
        notify_workflow_handoff_targets(
            &s,
            "space-a",
            &workflow,
            "escalate",
            "Escalated to editor",
            "owner-a",
        )
        .await
        .unwrap();
        assert_eq!(notification_users(&s).await, vec!["editor-a", "viewer-a"]);
        sqlx::query("DELETE FROM notifications")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        notify_workflow_handoff_targets(
            &s,
            "space-a",
            &workflow,
            "delegate",
            "Delegated to editor-a@example.test",
            "owner-a",
        )
        .await
        .unwrap();
        notify_workflow_handoff_targets(
            &s,
            "space-a",
            &workflow,
            "escalate",
            "Escalated to admin",
            "owner-a",
        )
        .await
        .unwrap();
        assert!(notification_users(&s).await.is_empty());
    }
    async fn deadline_fixture(wal: bool) -> AppState {
        let state = if wal {
            let state = super::super::routes::catalog_tests::catalog_state(true).await;
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,published_at,created_by,last_updated_by) VALUES('target-a','space-a','CANONICAL SECRET','','2020-01-01','owner-a','owner-a')").execute(&state.db).await.unwrap();
            for user in ["owner-a", "editor-a", "viewer-a"] {
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2999-01-01T00:00:00Z')").bind(format!("workflow-session-{user}")).bind(user).execute(&state.db).await.unwrap();
            }
            sqlx::query("INSERT INTO workflow_templates(id,space_id,name,stages,approvers,created_by) VALUES('template-a','space-a','Review','[{\"id\":\"review\",\"name\":\"Review\"},{\"id\":\"done\",\"name\":\"Done\"}]','[]','owner-a')").execute(&state.db).await.unwrap();
            state
        } else {
            workflow_fixture().await
        };
        enable_fixture(&state).await;
        delegate_fixture(&state, "deadline-delegate", "2020-01-01T00:00:00Z").await;
        state
    }
    fn deadline_before() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
    }
    async fn deadline_admin(state: &AppState) -> AdminUser {
        deadline_admin_with_expiry(state, None).await
    }
    async fn deadline_admin_with_expiry(state: &AppState, expiry: Option<i64>) -> AdminUser {
        sqlx::query("UPDATE users SET role='admin' WHERE id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let mut claims = test_auth("editor-a").claims;
        claims.role = "admin".into();
        if let Some(expiry) = expiry {
            claims.exp = expiry;
        }
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        <AdminUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
            &mut parts, state,
        )
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn deadline_typed_discovery_hides_private_restricted_ancestor_and_blocked_pages_even_from_admin(
    ) {
        for change in [
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('deadline-parent','space-a','PRIVATE PARENT','','owner-a','owner-a'); INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('parent-draft','deadline-parent','space-a','owner-a','PRIVATE TITLE','SECRET','old'); UPDATE documents SET parent_id='deadline-parent' WHERE id='target-a'",
            "INSERT INTO documents(id,space_id,title,content_path,published_at,created_by,last_updated_by) VALUES('deadline-parent','space-a','RESTRICTED','','old','owner-a','owner-a'); INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('deadline-parent','space-a','restricted',1,'owner-a'); UPDATE documents SET parent_id='deadline-parent' WHERE id='target-a'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')"
        ] {
            let state=deadline_fixture(false).await;
            let admin=deadline_admin(&state).await; let auth=deadline_http_auth(&admin);
            for sql in change.split(';').filter(|sql|!sql.trim().is_empty()) {sqlx::query(sql).execute(&state.db).await.unwrap();}
            let visible=visible_deadline_candidates(&state,DeadlineActor::Http{admin:&admin,auth:&auth},deadline_before()).await.unwrap();
            assert!(visible.is_empty(),"{change}");
            let before=workflow_domain(&state).await;
            let response=run_deadline_escalations(&state,DeadlineActor::Http{admin:&admin,auth:&auth},"owner",deadline_before()).await.unwrap();
            assert_eq!((response.scanned,response.escalated),(0,0));assert!(response.candidates.is_empty());
            assert_eq!(workflow_domain(&state).await,before);
        }
    }
    #[tokio::test]
    async fn deadline_private_title_uses_exact_owned_or_shared_draft_and_viewer_cannot_escalate() {
        for role in ["own", "viewer", "editor"] {
            let state = deadline_fixture(false).await;
            let admin = deadline_admin(&state).await;
            let auth = deadline_http_auth(&admin);
            sqlx::query("UPDATE documents SET published_at=NULL,title='UNSHARED CANONICAL SECRET' WHERE id='target-a'").execute(&state.db).await.unwrap();
            let owner = if role == "own" { "editor-a" } else { "owner-a" };
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('selected','target-a','space-a',$1,'EXACT READABLE TITLE','BODY','old')").bind(owner).execute(&state.db).await.unwrap();
            if role != "own" {
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('selected','editor-a',$1,'owner-a')").bind(role).execute(&state.db).await.unwrap();
            }
            let actor = DeadlineActor::Http {
                admin: &admin,
                auth: &auth,
            };
            let visible = visible_deadline_candidates(&state, actor, deadline_before())
                .await
                .unwrap();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].response().document_title, "EXACT READABLE TITLE");
            let before = workflow_domain(&state).await;
            let result = commit_deadline_escalation_candidate(
                &state,
                actor,
                "owner",
                deadline_before(),
                &visible[0].raw,
            )
            .await;
            if role == "viewer" {
                assert!(result.is_err());
                assert_eq!(workflow_domain(&state).await, before);
            } else {
                assert!(result.unwrap().is_some());
            }
        }
    }
    #[tokio::test]
    async fn deadline_background_user_is_explicit_sessionless_and_http_never_falls_back() {
        let state = deadline_fixture(false).await;
        let admin = deadline_admin(&state).await;
        let auth = deadline_http_auth(&admin);
        sqlx::query("DELETE FROM sessions WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = workflow_domain(&state).await;
        assert!(run_deadline_escalations(
            &state,
            DeadlineActor::Http {
                admin: &admin,
                auth: &auth
            },
            "owner",
            deadline_before()
        )
        .await
        .is_err());
        assert_eq!(workflow_domain(&state).await, before);
        let result = run_workflow_deadline_escalations_for_actor(
            &state,
            "editor-a",
            "owner",
            deadline_before(),
        )
        .await
        .unwrap();
        assert_eq!((result.scanned, result.escalated), (1, 1));
        let second = run_workflow_deadline_escalations_for_actor(
            &state,
            "editor-a",
            "owner",
            deadline_before(),
        )
        .await
        .unwrap();
        assert_eq!((second.scanned, second.escalated), (0, 0));
        sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(run_workflow_deadline_escalations_for_actor(
            &state,
            "editor-a",
            "owner",
            deadline_before()
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn deadline_http_router_keeps_real_admin_key_and_service_boundaries() {
        use tower::ServiceExt;
        let state = deadline_fixture(false).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for (kind, scope) in [
            ("user", "admin"),
            ("user", "read"),
            ("user", "write"),
            ("service", "admin"),
        ] {
            let id = format!("deadline-{kind}-{scope}");
            let raw = format!("stln_{id}");
            let hash = hex::encode(Sha256::digest(raw.as_bytes()));
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,$2,$1,$3,'stln_',$4,$5)").bind(&id).bind((kind=="user").then_some("editor-a")).bind(hash).bind(format!("[\"{scope}\"]")).bind(if kind=="service"{1_i64}else{0_i64}).execute(&state.db).await.unwrap();
            for method in ["GET", "POST"] {
                let response=router().with_state(state.clone()).oneshot(axum::http::Request::builder().method(method).uri("/admin/workflows/deadline-escalations?dry_run=true&before=2026-01-01").header("X-API-Key",raw.clone()).body(axum::body::Body::empty()).unwrap()).await.unwrap();
                if kind == "user" && scope == "admin" {
                    assert_eq!(response.status(), StatusCode::OK);
                } else {
                    assert_ne!(response.status(), StatusCode::OK);
                }
            }
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM document_workflow_events WHERE action='escalate'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn deadline_final_event_policy_and_source_drift_roll_back_entire_candidate() {
        for change in [
            "DELETE FROM sessions WHERE user_id='editor-a'",
            "UPDATE users SET role='member' WHERE id='editor-a'",
            "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'",
            "UPDATE document_workflows SET current_stage_id='done' WHERE document_id='target-a'",
            "UPDATE document_workflow_events SET note='Delegated to owner: due 2099-01-01' WHERE id='deadline-delegate'",
            "UPDATE document_workflow_events SET created_by='owner-a' WHERE id=NEW.id",
            "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-a','space-a','restricted',1,'owner-a')"
        ] {
            let state=deadline_fixture(false).await;let admin=deadline_admin(&state).await;let auth=deadline_http_auth(&admin);let actor=DeadlineActor::Http{admin:&admin,auth:&auth};
            let candidate=fetch_workflow_deadline_escalation_candidates(&state,deadline_before()).await.unwrap().remove(0);
            sqlx::query(&format!("CREATE TRIGGER late_deadline AFTER INSERT ON document_workflow_events WHEN NEW.action='escalate' BEGIN {change}; END")).execute(&state.db).await.unwrap();
            let before=workflow_domain(&state).await;
            assert!(commit_deadline_escalation_candidate(&state,actor,"owner",deadline_before(),&candidate).await.is_err(),"{change}");
            assert_eq!(workflow_domain(&state).await,before,"{change}");
        }
    }
    #[tokio::test]
    async fn deadline_final_read_rechecks_real_wal_session_publication_and_exact_draft_share() {
        for change in [
            "DELETE FROM sessions WHERE id='workflow-session-editor-a'",
            "UPDATE documents SET title='NEW CANONICAL TITLE' WHERE id='target-a'",
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "DELETE FROM document_draft_access WHERE draft_id='selected' AND user_id='editor-a'",
        ] {
            let state = deadline_fixture(true).await;
            let admin = deadline_admin(&state).await;
            let auth = deadline_http_auth(&admin);
            if change.contains("draft_id") {
                sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                for (id, title, time) in [
                    ("selected", "READABLE OLD", "2021-01-01"),
                    ("other", "OTHER STILL READABLE", "2020-01-01"),
                ] {
                    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,updated_at,base_updated_at) VALUES($1,'target-a','space-a','owner-a',$2,'SECRET',$3,'old')").bind(id).bind(title).bind(time).execute(&state.db).await.unwrap();
                    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'editor-a','viewer','owner-a')").bind(id).execute(&state.db).await.unwrap();
                }
            } else if change.contains("published_at") {
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('other','target-a','space-a','editor-a','OTHER STILL READABLE','BODY','old')").execute(&state.db).await.unwrap();
            }
            let db = state.db.clone();
            DEADLINE_READ_PAUSE.with(|slot| {
                *slot.borrow_mut() = Some(Box::pin(async move {
                    sqlx::query(change).execute(&db).await.unwrap();
                }))
            });
            assert!(
                visible_deadline_candidates(
                    &state,
                    DeadlineActor::Http {
                        admin: &admin,
                        auth: &auth
                    },
                    deadline_before()
                )
                .await
                .is_err(),
                "{change}"
            );
        }
    }
    #[tokio::test]
    async fn deadline_real_wal_space_guard_wait_retains_actual_admin_session() {
        let state = deadline_fixture(true).await;
        let admin = deadline_admin(&state).await;
        let auth = deadline_http_auth(&admin);
        let candidate = fetch_workflow_deadline_escalation_candidates(&state, deadline_before())
            .await
            .unwrap()
            .remove(0);
        let mut blocker = state.db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut blocker, &["space-a"])
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='workflow-session-editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let (before_tx, before_rx) = tokio::sync::oneshot::channel();
        let copied = state.clone();
        let mut waiting = tokio::spawn(async move {
            before_tx.send(()).unwrap();
            commit_deadline_escalation_candidate(
                &copied,
                DeadlineActor::Http {
                    admin: &admin,
                    auth: &auth,
                },
                "owner",
                deadline_before(),
                &candidate,
            )
            .await
        });
        before_rx.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut waiting)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        let before = workflow_domain(&state).await;
        assert!(waiting.await.unwrap().is_err());
        assert_eq!(workflow_domain(&state).await, before);
    }
    #[tokio::test]
    async fn deadline_last_source_boundary_rechecks_exact_share_with_another_readable_draft_wal() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let state = deadline_fixture(true).await;
        let admin = deadline_admin(&state).await;
        let auth = deadline_http_auth(&admin);
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for (id, title, time) in [
            ("selected", "SELECTED SECRET", "2021-01-01"),
            ("other", "OTHER READABLE", "2020-01-01"),
        ] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,updated_at,base_updated_at) VALUES($1,'target-a','space-a','owner-a',$2,'BODY',$3,'old')").bind(id).bind(title).bind(time).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'editor-a','viewer','owner-a')").bind(id).execute(&state.db).await.unwrap();
        }
        let db = state.db.clone();
        let reached = Arc::new(AtomicBool::new(false));
        let flag = reached.clone();
        DEADLINE_FINAL_RESOURCE_PAUSE.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
            sqlx::query("DELETE FROM document_draft_access WHERE draft_id='selected' AND user_id='editor-a'").execute(&db).await.unwrap();
            flag.store(true,Ordering::SeqCst);
        })));
        let actor = DeadlineActor::Http {
            admin: &admin,
            auth: &auth,
        };
        assert!(
            visible_deadline_candidates(&state, actor, deadline_before())
                .await
                .is_err()
        );
        assert!(reached.load(Ordering::SeqCst));
        let mut conn = state.db.acquire().await.unwrap();
        super::super::access::resolve_page_access(
            &mut conn,
            actor.principal().unwrap(),
            "space-a",
            "target-a",
            super::super::access::PageOperation::View,
            super::super::access::Purpose::Routine,
        )
        .await
        .unwrap();
        super::super::access::resolve_draft_access(
            &mut conn,
            actor.principal().unwrap(),
            "space-a",
            "target-a",
            "other",
            super::super::access::DraftOperation::View,
            super::super::access::Purpose::Routine,
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn deadline_original_http_jwt_expiry_after_last_source_does_not_become_background_authority(
    ) {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let state = deadline_fixture(false).await;
        let expiry = chrono::Utc::now().timestamp() + 2;
        let admin = deadline_admin_with_expiry(&state, Some(expiry)).await;
        let auth = deadline_http_auth(&admin);
        let before = workflow_domain(&state).await;
        let reached = Arc::new(AtomicBool::new(false));
        let flag = reached.clone();
        DEADLINE_FINAL_IDENTITY_PAUSE.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                flag.store(true, Ordering::SeqCst);
                while chrono::Utc::now().timestamp() < expiry {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }))
        });
        assert!(matches!(
            visible_deadline_candidates(
                &state,
                DeadlineActor::Http {
                    admin: &admin,
                    auth: &auth
                },
                deadline_before()
            )
            .await,
            Err(AppError::Unauthorized(_))
        ));
        assert!(reached.load(Ordering::SeqCst));
        assert_eq!(workflow_domain(&state).await, before);
        let session: String = sqlx::query_scalar(
            "SELECT expires_at FROM sessions WHERE id='workflow-session-editor-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(
            chrono::DateTime::parse_from_rfc3339(&session)
                .unwrap()
                .timestamp()
                > expiry
        );
    }
    #[tokio::test]
    async fn workflow_current_private_notification_title_uses_exact_owned_and_shared_sources() {
        let state = notification_role_fixture().await;
        sqlx::query("UPDATE documents SET published_at=NULL,title='CANONICAL UNRECOGNIZED' WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('workflow-readable','target-a','space-a','editor-a','WORKFLOW READABLE','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('workflow-readable','viewer-a','viewer','editor-a')").execute(&state.db).await.unwrap();
        for user in ["editor-a", "viewer-a"] {
            let (_, title) = workflow_notification_recipient(&state, "space-a", "target-a", user)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(title, "WORKFLOW READABLE");
        }
        for user in ["owner-a", "actual-admin"] {
            assert!(
                workflow_notification_recipient(&state, "space-a", "target-a", user)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        let workflow = notification_workflow(
            r#"[{"id":"notify-user","approver_type":"user","approver_id":"viewer-a"}]"#,
        );
        notify_workflow_approvers(&state, "space-a", &workflow, "review", "owner-a")
            .await
            .unwrap();
        let titles: Vec<String> =
            sqlx::query_scalar("SELECT title FROM notifications WHERE user_id='viewer-a'")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(titles.len(), 1);
        assert!(titles[0].contains("WORKFLOW READABLE"));
        assert!(!titles[0].contains("CANONICAL"));
    }
}
