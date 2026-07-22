use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize)]
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
    effective_role: &str,
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
                && (approver.approver_id == effective_role || effective_role == "admin"))
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

    let document_title =
        sqlx::query_as::<_, (String,)>("SELECT title FROM documents WHERE id = ?1")
            .bind(&workflow.document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?
            .map(|row| row.0)
            .unwrap_or_else(|| "Untitled".to_string());
    let stage_name = workflow
        .stages
        .iter()
        .find(|stage| stage.id.as_deref() == Some(stage_id))
        .map(|stage| stage.name.clone())
        .unwrap_or_else(|| "approval".to_string());

    let mut recipient_ids = HashSet::new();
    for approver in approvers {
        if approver.approver_type == "user" {
            let active_user = sqlx::query_as::<_, (String,)>(
                "SELECT id FROM users WHERE id = ?1 AND is_active = 1",
            )
            .bind(&approver.approver_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
            if let Some((user_id,)) = active_user {
                recipient_ids.insert(user_id);
            }
        } else if approver.approver_type == "role" {
            let mut role_users = if approver.approver_id == "admin" {
                sqlx::query_as::<_, (String,)>(
                    "SELECT id FROM users WHERE role = 'admin' AND is_active = 1",
                )
                .fetch_all(&state.db)
                .await
                .map_err(AppError::internal)?
            } else {
                sqlx::query_as::<_, (String,)>(
                    "SELECT sm.user_id FROM space_members sm \
                     JOIN users u ON u.id = sm.user_id \
                     WHERE sm.space_id = ?1 AND sm.role = ?2 AND u.is_active = 1",
                )
                .bind(space_id)
                .bind(&approver.approver_id)
                .fetch_all(&state.db)
                .await
                .map_err(AppError::internal)?
            };

            for (user_id,) in role_users.drain(..) {
                recipient_ids.insert(user_id);
            }
        }
    }

    for recipient_id in recipient_ids {
        if recipient_id == actor_id {
            continue;
        }
        crate::notifications::notify(
            &state.db,
            &recipient_id,
            "workflow_approval",
            &format!("Workflow approval requested: {}", document_title),
            Some(&format!(
                "{} is ready for {} in {}.",
                document_title, stage_name, workflow.workflow_template_name
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
    let recipient_ids = resolve_handoff_target_users(state, space_id, &target).await?;
    if recipient_ids.is_empty() {
        return Ok(());
    }

    let document_title =
        sqlx::query_as::<_, (String,)>("SELECT title FROM documents WHERE id = ?1")
            .bind(&workflow.document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?
            .map(|row| row.0)
            .unwrap_or_else(|| "Untitled".to_string());
    let verb = if action == "delegate" {
        "delegated"
    } else {
        "escalated"
    };

    for recipient_id in recipient_ids {
        if recipient_id == actor_id {
            continue;
        }
        crate::notifications::notify(
            &state.db,
            &recipient_id,
            if action == "delegate" {
                "workflow_delegate"
            } else {
                "workflow_escalate"
            },
            &format!("Workflow {}: {}", verb, document_title),
            Some(&format!(
                "{} was {} to {} in {}. {}",
                document_title, verb, target, workflow.workflow_template_name, note
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

    let mut recipient_ids = HashSet::new();
    let matched_users = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE is_active = 1 AND (id = ?1 OR email = ?1)",
    )
    .bind(normalized)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;
    for (user_id,) in matched_users {
        recipient_ids.insert(user_id);
    }

    let role_users = if normalized == "admin" {
        sqlx::query_as::<_, (String,)>(
            "SELECT id FROM users WHERE role = 'admin' AND is_active = 1",
        )
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?
    } else {
        sqlx::query_as::<_, (String,)>(
            "SELECT sm.user_id FROM space_members sm \
             JOIN users u ON u.id = sm.user_id \
             WHERE sm.space_id = ?1 AND sm.role = ?2 AND u.is_active = 1",
        )
        .bind(space_id)
        .bind(normalized)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?
    };
    for (user_id,) in role_users {
        recipient_ids.insert(user_id);
    }

    Ok(recipient_ids)
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

async fn require_document_in_space(
    state: &AppState,
    space_id: &str,
    document_id: &str,
) -> Result<(), AppError> {
    let exists =
        sqlx::query_as::<_, (String,)>("SELECT id FROM documents WHERE id = ?1 AND space_id = ?2")
            .bind(document_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?
            .is_some();

    if exists {
        Ok(())
    } else {
        Err(AppError::NotFound("Document not found".into()))
    }
}

async fn list_workflow_deadline_escalations(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<WorkflowDeadlineEscalationQuery>,
) -> Result<impl IntoResponse, AppError> {
    let before = parse_deadline_scan_before(query.before.as_deref())?;
    let candidates = fetch_workflow_deadline_escalation_candidates(&state, before).await?;

    Ok(Json(candidates))
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
           AND dwe.created_at = ( \
             SELECT MAX(latest.created_at) FROM document_workflow_events latest \
             WHERE latest.document_workflow_id = dwe.document_workflow_id \
               AND latest.action IN ('delegate', 'escalate') \
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
    let candidates = fetch_workflow_deadline_escalation_candidates(&state, before).await?;

    if dry_run {
        return Ok(Json(WorkflowDeadlineEscalationRunResponse {
            scanned: candidates.len(),
            escalated: 0,
            dry_run,
            target,
            candidates,
        }));
    }

    let result =
        run_workflow_deadline_escalations_for_actor(&state, &admin.claims.sub, &target, before)
            .await?;
    Ok(Json(result))
}

pub async fn run_workflow_deadline_escalations_for_actor(
    state: &AppState,
    actor_id: &str,
    target: &str,
    before: NaiveDate,
) -> Result<WorkflowDeadlineEscalationRunResponse, AppError> {
    let actor_exists =
        sqlx::query_as::<_, (String,)>("SELECT id FROM users WHERE id = ?1 AND is_active = 1")
            .bind(actor_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?
            .is_some();
    if !actor_exists {
        return Err(AppError::BadRequest(
            "workflow deadline escalation actor must be an active user".into(),
        ));
    }
    let target = normalize_deadline_escalation_target(Some(target))?;
    let candidates = fetch_workflow_deadline_escalation_candidates(state, before).await?;

    for candidate in &candidates {
        let workflow = fetch_document_workflow(state, &candidate.space_id, &candidate.document_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Document workflow not found".into()))?;
        let note = format_deadline_escalation_note(&target, candidate);
        insert_workflow_event(
            state,
            &workflow,
            None,
            &workflow.current_stage_id,
            "escalate",
            Some(&note),
            actor_id,
            None,
            None,
            None,
        )
        .await?;
        notify_workflow_handoff_targets(
            state,
            &candidate.space_id,
            &workflow,
            "escalate",
            &note,
            actor_id,
        )
        .await?;
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

    Ok(WorkflowDeadlineEscalationRunResponse {
        scanned: candidates.len(),
        escalated: candidates.len(),
        dry_run: false,
        target,
        candidates,
    })
}

async fn fetch_document_workflow(
    state: &AppState,
    space_id: &str,
    document_id: &str,
) -> Result<Option<DocumentWorkflowResponse>, AppError> {
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
         WHERE dw.document_id = ?1 AND wt.space_id = ?2",
    )
    .bind(document_id)
    .bind(space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)
    .map(|row| row.map(document_workflow_response_from_row))
}

async fn insert_workflow_event(
    state: &AppState,
    workflow: &DocumentWorkflowResponse,
    from_stage_id: Option<&str>,
    to_stage_id: &str,
    action: &str,
    note: Option<&str>,
    user_id: &str,
    source_id: Option<&str>,
    created_by: Option<&str>,
    created_at: Option<&str>,
) -> Result<(), AppError> {
    let event_id = super::routes::import_id_or_uuid(source_id)?;
    let created_at = super::routes::import_timestamp_or_now(created_at)?;
    let created_by = if source_id.is_some() {
        match created_by.map(str::trim).filter(|value| !value.is_empty()) {
            Some(candidate) => {
                let existing_user: Option<(String,)> =
                    sqlx::query_as("SELECT id FROM users WHERE id = ?1")
                        .bind(candidate)
                        .fetch_optional(&state.db)
                        .await
                        .map_err(AppError::internal)?;
                existing_user
                    .map(|row| row.0)
                    .unwrap_or_else(|| user_id.to_string())
            }
            None => user_id.to_string(),
        }
    } else {
        user_id.to_string()
    };
    sqlx::query(
        "INSERT INTO document_workflow_events \
             (id, document_workflow_id, document_id, from_stage_id, to_stage_id, action, note, created_by, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(())
}

async fn list_workflow_templates(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let templates = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
        "SELECT id, space_id, name, description, stages, approvers, created_by, created_at, updated_at \
         FROM workflow_templates WHERE space_id = ?1 ORDER BY name",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?
    .into_iter()
    .map(row_to_workflow_template)
    .collect::<Vec<_>>();

    Ok(Json(templates))
}

async fn get_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    require_document_in_space(&state, &space_id, &document_id).await?;

    Ok(Json(
        fetch_document_workflow(&state, &space_id, &document_id).await?,
    ))
}

async fn enable_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<EnableDocumentWorkflowRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;
    require_document_in_space(&state, &space_id, &document_id).await?;

    let template = sqlx::query_as::<_, (String, String, String)>(
        "SELECT name, stages, approvers FROM workflow_templates WHERE id = ?1 AND space_id = ?2",
    )
    .bind(req.template_id.trim())
    .bind(&space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?
    .ok_or_else(|| AppError::NotFound("Workflow template not found".into()))?;

    let stages: Vec<WorkflowStage> = parse_json_vec(template.1.clone());
    let stage_id = req
        .stage_id
        .as_deref()
        .map(str::trim)
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

    let workflow_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    sqlx::query(
        "INSERT INTO document_workflows \
             (id, document_id, workflow_template_id, current_stage_id, status, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7) \
         ON CONFLICT(document_id) DO UPDATE SET \
             workflow_template_id = excluded.workflow_template_id, \
             current_stage_id = excluded.current_stage_id, \
             status = 'active', \
             updated_at = excluded.updated_at",
    )
    .bind(&workflow_id)
    .bind(&document_id)
    .bind(req.template_id.trim())
    .bind(&stage_id)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.workflow_enable",
        Some("document"),
        Some(&document_id),
    )
    .await;

    let workflow = fetch_document_workflow(&state, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to load document workflow".into()))?;
    insert_workflow_event(
        &state,
        &workflow,
        None,
        &stage_id,
        "enable",
        None,
        &auth.claims.sub,
        req.event_source_id.as_deref(),
        req.event_created_by.as_deref(),
        req.event_created_at
            .as_deref()
            .or_else(|| req.created_at.as_deref()),
    )
    .await?;
    notify_workflow_approvers(&state, &space_id, &workflow, &stage_id, &auth.claims.sub).await?;

    Ok(Json(workflow))
}

async fn transition_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<TransitionDocumentWorkflowRequest>,
) -> Result<impl IntoResponse, AppError> {
    let effective_role = super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;
    require_document_in_space(&state, &space_id, &document_id).await?;

    let workflow = fetch_document_workflow(&state, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Document workflow not found".into()))?;
    let stage_id = req.stage_id.trim().to_string();
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
        &auth.claims.sub,
        &effective_role,
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

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE document_workflows SET current_stage_id = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
    )
    .bind(&stage_id)
    .bind(status)
    .bind(&updated_at)
    .bind(&workflow.id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    insert_workflow_event(
        &state,
        &workflow,
        Some(&workflow.current_stage_id),
        &stage_id,
        "transition",
        req.note
            .as_deref()
            .map(str::trim)
            .filter(|note| !note.is_empty()),
        &auth.claims.sub,
        None,
        None,
        None,
    )
    .await?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.workflow_transition",
        Some("document"),
        Some(&document_id),
    )
    .await;

    let updated = fetch_document_workflow(&state, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to load document workflow".into()))?;
    notify_workflow_approvers(&state, &space_id, &updated, &stage_id, &auth.claims.sub).await?;

    Ok(Json(updated))
}

async fn record_document_workflow_event(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<RecordDocumentWorkflowEventRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;
    require_document_in_space(&state, &space_id, &document_id).await?;

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
    let note = req
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
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

    let workflow = fetch_document_workflow(&state, &space_id, &document_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Document workflow not found".into()))?;
    let to_stage_id = req
        .to_stage_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&workflow.current_stage_id);
    let from_stage_id = req
        .from_stage_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    insert_workflow_event(
        &state,
        &workflow,
        from_stage_id,
        to_stage_id,
        action,
        note,
        &auth.claims.sub,
        req.source_id.as_deref(),
        req.created_by.as_deref(),
        req.created_at.as_deref(),
    )
    .await?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
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
            return list_document_workflow_events(
                State(state),
                auth,
                Path((space_id, document_id)),
            )
            .await;
        };
        notify_workflow_handoff_targets(
            &state,
            &space_id,
            &workflow,
            action,
            note,
            &auth.claims.sub,
        )
        .await?;
    }

    list_document_workflow_events(State(state), auth, Path((space_id, document_id))).await
}

async fn list_document_workflow_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    require_document_in_space(&state, &space_id, &document_id).await?;

    let events = sqlx::query_as::<
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
         FROM document_workflow_events WHERE document_id = ?1 ORDER BY created_at DESC",
    )
    .bind(&document_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?
    .into_iter()
    .map(workflow_event_from_row)
    .collect::<Vec<_>>();

    Ok(Json(events))
}

async fn disable_document_workflow(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;
    require_document_in_space(&state, &space_id, &document_id).await?;

    let result = sqlx::query("DELETE FROM document_workflows WHERE document_id = ?1")
        .bind(&document_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if result.rows_affected() > 0 {
        crate::audit::log_action(
            &state.db,
            Some(&auth.claims.sub),
            None,
            "document.workflow_disable",
            Some("document"),
            Some(&document_id),
        )
        .await;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn get_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let template = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, String, String, String)>(
        "SELECT id, space_id, name, description, stages, approvers, created_by, created_at, updated_at \
         FROM workflow_templates WHERE id = ?1 AND space_id = ?2",
    )
    .bind(&template_id)
    .bind(&space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?
    .map(row_to_workflow_template)
    .ok_or_else(|| AppError::NotFound("Workflow template not found".into()))?;

    Ok(Json(template))
}

async fn create_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<WorkflowTemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;
    let (stages, approvers) = normalize_request(&req)?;
    let template_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    let stages_json = serde_json::to_string(&stages).map_err(AppError::internal)?;
    let approvers_json = serde_json::to_string(&approvers).map_err(AppError::internal)?;

    let existing_id: Option<(String,)> =
        sqlx::query_as("SELECT id FROM workflow_templates WHERE id = ?1")
            .bind(&template_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A workflow template with this import id already exists".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO workflow_templates (id, space_id, name, description, stages, approvers, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&template_id)
    .bind(&space_id)
    .bind(req.name.trim())
    .bind(&req.description)
    .bind(stages_json)
    .bind(approvers_json)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "workflow_template.create",
        Some("workflow_template"),
        Some(&template_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": template_id })),
    ))
}

async fn update_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
    Json(req): Json<WorkflowTemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;
    let (stages, approvers) = normalize_request(&req)?;
    let stages_json = serde_json::to_string(&stages).map_err(AppError::internal)?;
    let approvers_json = serde_json::to_string(&approvers).map_err(AppError::internal)?;

    let updated_at = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE workflow_templates SET name = ?1, description = ?2, stages = ?3, approvers = ?4, \
         updated_at = ?5 WHERE id = ?6 AND space_id = ?7",
    )
    .bind(req.name.trim())
    .bind(&req.description)
    .bind(stages_json)
    .bind(approvers_json)
    .bind(&updated_at)
    .bind(&template_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Workflow template not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "workflow_template.update",
        Some("workflow_template"),
        Some(&template_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_workflow_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let result = sqlx::query("DELETE FROM workflow_templates WHERE id = ?1 AND space_id = ?2")
        .bind(&template_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Workflow template not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "workflow_template.delete",
        Some("workflow_template"),
        Some(&template_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "editor",
        ));
    }

    #[test]
    fn workflow_transition_allows_matching_stage_role() {
        assert!(workflow_approver_allows_transition(
            &[role_approver("owner", Some("review"))],
            "review",
            "user-1",
            "owner",
        ));
    }

    #[test]
    fn workflow_transition_blocks_non_matching_stage_role() {
        assert!(!workflow_approver_allows_transition(
            &[role_approver("owner", Some("review"))],
            "review",
            "user-1",
            "editor",
        ));
    }

    #[test]
    fn workflow_transition_uses_global_approvers_without_stage_match() {
        assert!(workflow_approver_allows_transition(
            &[role_approver("editor", None)],
            "review",
            "user-1",
            "editor",
        ));
    }

    #[test]
    fn workflow_transition_allows_matching_user() {
        assert!(workflow_approver_allows_transition(
            &[user_approver("user-1", Some("review"))],
            "review",
            "user-1",
            "editor",
        ));
    }

    #[test]
    fn workflow_transition_allows_admin_role_override() {
        assert!(workflow_approver_allows_transition(
            &[role_approver("owner", Some("review"))],
            "review",
            "admin-user",
            "admin",
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
    }
}
