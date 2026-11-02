//! Reviewed whole-subtree server relocation using the shared scoped writer protocol.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommitRequest {
    pub fingerprint: String,
    pub decisions: Vec<MetadataDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identifier_decisions: Vec<IdentifierDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintenance_decisions: Vec<MaintenanceDecision>,
    #[serde(default)]
    pub incident_relations: Vec<IncidentRelationDecision>,
    pub public_links: PublicLinkDisposition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub viewer_grant_decisions: Vec<ViewerGrantDecision>,
    pub confirm_audience_change: bool,
    pub confirm_draft_space_change: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IdentifierDecision {
    document_id: String,
    choice: IdentifierChoice,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewerGrantDecision {
    pub grant_id: String,
    pub revision: String,
    pub choice: ViewerGrantChoice,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ViewerGrantChoice {
    Preserve,
    Remove { confirm: bool },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum IdentifierChoice {
    Preserve,
    Rename { value: String },
    Clear,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaintenanceDecision {
    document_id: String,
    choice: MaintenanceChoice,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum MaintenanceChoice {
    Preserve,
    Clear,
    User { user_id: String },
    Group { group_id: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MetadataDecision {
    pub family: MetadataFamily,
    pub source_id: String,
    pub choice: MetadataChoice,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(super) enum MetadataFamily {
    Tag,
    Property,
    Workflow,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum MetadataChoice {
    Map {
        destination_id: String,
        #[serde(default)]
        values: BTreeMap<String, String>,
        #[serde(default)]
        stages: BTreeMap<String, String>,
    },
    Create {
        definition: Value,
        #[serde(default)]
        values: BTreeMap<String, String>,
        #[serde(default)]
        stages: BTreeMap<String, String>,
    },
    Preserve,
    Remove {
        confirm: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncidentRelationDecision {
    pub assignment_id: String,
    pub choice: IncidentRelationChoice,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum IncidentRelationChoice {
    Map { target_id: String },
    Preserve,
    Remove { confirm: bool },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PublicLinkDisposition {
    Preserve,
    Disable,
}
/// All live server subtree writers now share audience/space mutation guards.
/// Connected local/provider moves remain a separate unavailable contract.
pub(super) fn writers_ready() -> bool { true }
pub(super) async fn commit_preview(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<CommitRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_preview_identity(&mut conn, &auth, MovePreviewCredential::Write).await?;
    let historical:Option<(String,String,String,String,String)>=sqlx::query_as("SELECT op_id,request_fingerprint,source_fingerprint,manifest_json,actor_id FROM move_commit_receipts WHERE preview_id=$1").bind(&id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if let Some((op, request_fp, source_fp, raw, actor)) = historical {
        let actual_request =
            digest(&json!({"preview_id":id,"actor":auth.claims.sub,"request":req}));
        let m: crate::storage::relocation::PreparedRelocation =
            serde_json::from_str(&raw).map_err(|_| review_conflict())?;
        if actor != auth.claims.sub
            || request_fp != actual_request
            || m.op_id != op
            || m.request_fingerprint != request_fp
            || m.source_fingerprint != source_fp
            || source_fp != req.fingerprint
            || !crate::storage::relocation::valid_manifest(&m)
        {
            return Err(review_conflict());
        }
        let receipt = immutable_receipt(&mut conn, &op).await?;
        crate::storage::relocation::verify_escrow(&mut conn, &receipt).await?;
        require_preview_identity(&mut conn, &auth, MovePreviewCredential::Write).await?;
        crate::storage::relocation::verify_sealed_receipt(&crate::storage::data_dir(), &receipt)?;
        return Ok((
            StatusCode::OK,
            Json(json!({"operation_id":op,"committed":true,"historical_receipt":true})),
        ));
    }
    let record = owned_preview(&mut conn, &id, &auth.claims.sub).await?;
    if req.fingerprint != text(&record, "fingerprint") {
        return Err(AppError::Conflict(
            "Move preview changed; review again".into(),
        ));
    }
    let request: MovePreviewRequest =
        serde_json::from_str(text(&record, "request_json")).map_err(|_| unavailable())?;
    if text(&record, "root_kind") != request.root.kind
        || text(&record, "root_id") != request.root.id
    {
        return Err(unavailable());
    }
    let prepared = review_move_current(
        &mut conn,
        &auth,
        text(&record, "source_space_id"),
        &request,
        MovePreviewCredential::Write,
    )
    .await?;
    validate_decisions(&prepared, &req)?;
    verify_review(
        &mut conn,
        &auth,
        text(&record, "source_space_id"),
        &request,
        &prepared,
        MovePreviewCredential::Write,
    )
    .await?;
    // Relocation acquires its own transaction and revalidates source/authority.
    // Release the preliminary pooled reader before acquiring that connection.
    drop(conn);
    let result = relocate(&state, &auth, &id, &req).await?;
    Ok((StatusCode::OK, Json(result)))
}
/// Resolve all identifiers before preparing any relocation files. No namespace label escapes.
fn plan_identifiers(
    prepared: &PreparedReview,
    req: &CommitRequest,
) -> Result<BTreeMap<String, (Value, Value)>, AppError> {
    let pages = prepared.manifest["pages"]
        .as_array()
        .ok_or_else(review_conflict)?;
    let mut decisions = BTreeMap::new();
    for decision in &req.identifier_decisions {
        if !pages
            .iter()
            .any(|page| text(page, "id") == decision.document_id)
            || decisions
                .insert(decision.document_id.clone(), &decision.choice)
                .is_some()
        {
            return Err(AppError::BadRequest(
                "Invalid move identifier decisions".into(),
            ));
        }
    }
    let mut keys = BTreeSet::new();
    let mut result = BTreeMap::new();
    let destination = prepared.manifest["identifier_destination_rows"]
        .as_array()
        .ok_or_else(review_conflict)?;
    for page in pages {
        let old = page.get("human_identifier").and_then(Value::as_str);
        let normalized = super::super::page_metadata::normalize_identifier(old)?;
        if json!(normalized.0) != page.get("human_identifier").cloned().unwrap_or(Value::Null)
            || json!(normalized.1)
                != page
                    .get("human_identifier_key")
                    .cloned()
                    .unwrap_or(Value::Null)
        {
            return Err(review_conflict());
        }
        let pair = match decisions.get(text(page, "id")) {
            Some(IdentifierChoice::Rename { value }) => {
                let pair = super::super::page_metadata::normalize_identifier(Some(value))?;
                if pair.0.is_none() {
                    return Err(AppError::BadRequest(
                        "Use the explicit clear identifier choice".into(),
                    ));
                }
                pair
            }
            Some(IdentifierChoice::Clear) => (None, None),
            _ => normalized,
        };
        if let Some(key) = &pair.1 {
            if !keys.insert(key.clone())
                || destination.iter().any(|row| {
                    !pages.iter().any(|moving| moving["id"] == row["id"])
                        && row["human_identifier_key"].as_str() == Some(key)
                })
            {
                return Err(AppError::Conflict("Resolve move identifier conflicts by explicitly renaming or clearing the affected identifiers".into()));
            }
        }
        result.insert(text(page, "id").into(), (json!(pair.0), json!(pair.1)));
    }
    Ok(result)
}
pub(super) fn maintenance_pair(page: &Value) -> Result<(Option<String>, Option<String>), AppError> {
    let parse = |key: &str| match page.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if !id.is_empty() && !id.trim().is_empty() => Ok(Some(id.clone())),
        _ => Err(review_conflict()),
    };
    let pair = (
        parse("maintenance_user_id")?,
        parse("maintenance_group_id")?,
    );
    if pair.0.is_some() && pair.1.is_some() {
        return Err(review_conflict());
    }
    Ok(pair)
}
fn plan_maintenance(
    prepared: &PreparedReview,
    req: &CommitRequest,
) -> Result<BTreeMap<String, (Value, Value)>, AppError> {
    let pages = prepared.manifest["pages"]
        .as_array()
        .ok_or_else(review_conflict)?;
    let mut decisions = BTreeMap::new();
    for decision in &req.maintenance_decisions {
        if !pages.iter().any(|p| text(p, "id") == decision.document_id)
            || decisions
                .insert(decision.document_id.clone(), &decision.choice)
                .is_some()
        {
            return Err(AppError::BadRequest(
                "Invalid move maintenance decisions".into(),
            ));
        }
    }
    let mut planned = BTreeMap::new();
    for page in pages {
        let old = maintenance_pair(page)?;
        let choice = decisions.get(text(page, "id"));
        let projection = prepared.review.inventory["pages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"] == page["id"]);
        if matches!(choice, None | Some(MaintenanceChoice::Preserve))
            && projection.and_then(|p| p["maintenance_resolution_required"].as_bool()) == Some(true)
        {
            return Err(AppError::Conflict(
                "Resolve maintenance assignments unavailable in the destination".into(),
            ));
        }
        let pair = match choice {
            Some(MaintenanceChoice::Clear) => (None, None),
            Some(MaintenanceChoice::User { user_id })
                if !user_id.is_empty() && !user_id.trim().is_empty() =>
            {
                (Some(user_id.clone()), None)
            }
            Some(MaintenanceChoice::Group { group_id })
                if !group_id.is_empty() && !group_id.trim().is_empty() =>
            {
                (None, Some(group_id.clone()))
            }
            Some(MaintenanceChoice::User { .. }) | Some(MaintenanceChoice::Group { .. }) => {
                return Err(AppError::BadRequest(
                    "Maintenance target identity must be explicit".into(),
                ))
            }
            _ => old,
        };
        planned.insert(text(page, "id").to_string(), (json!(pair.0), json!(pair.1)));
    }
    Ok(planned)
}
async fn prove_maintenance(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    req: &CommitRequest,
    before: &PreparedReview,
    planned: &BTreeMap<String, (Value, Value)>,
) -> Result<BTreeMap<String, Value>, AppError> {
    let mut witnesses = BTreeMap::new();
    for page in before.manifest["pages"].as_array().into_iter().flatten() {
        let pair = &planned[text(page, "id")];
        let choice = req
            .maintenance_decisions
            .iter()
            .find(|d| d.document_id == text(page, "id"))
            .map(|d| &d.choice);
        let mode = if matches!(choice, None | Some(MaintenanceChoice::Preserve)) {
            super::super::maintenance::TargetUse::PreserveMove
        } else {
            super::super::maintenance::TargetUse::Select
        };
        let witness = super::super::maintenance::validate_assignment(
            conn,
            auth,
            space,
            pair.0.as_str(),
            pair.1.as_str(),
            mode,
        )
        .await
        .map_err(conceal)?;
        witnesses.insert(text(page, "id").into(), witness);
    }
    Ok(witnesses)
}
async fn require_maintenance_edits(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    before: &PreparedReview,
    planned: &BTreeMap<String, (Value, Value)>,
) -> Result<(), AppError> {
    for page in before.manifest["pages"].as_array().into_iter().flatten() {
        let old = maintenance_pair(page)?;
        let pair = &planned[text(page, "id")];
        if pair != &(json!(old.0), json!(old.1)) {
            access::resolve_page_access(
                conn,
                access::principal_from_auth(auth)?,
                space,
                text(page, "id"),
                access::PageOperation::Edit,
                access::Purpose::Routine,
            )
            .await
            .map_err(conceal)?;
        }
    }
    Ok(())
}
async fn require_identifier_edits(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    source: &str,
    before: &PreparedReview,
    planned: &BTreeMap<String, (Value, Value)>,
) -> Result<(), AppError> {
    for page in before.manifest["pages"].as_array().into_iter().flatten() {
        let pair = &planned[text(page, "id")];
        if pair.0 != page.get("human_identifier").cloned().unwrap_or(Value::Null)
            || pair.1
                != page
                    .get("human_identifier_key")
                    .cloned()
                    .unwrap_or(Value::Null)
        {
            access::resolve_page_access(
                conn,
                access::principal_from_auth(auth)?,
                source,
                text(page, "id"),
                access::PageOperation::Edit,
                access::Purpose::Routine,
            )
            .await
            .map_err(conceal)?;
        }
    }
    Ok(())
}
#[derive(Clone)]
struct ViewerGrantPlan {
    original: Value,
    expected: Option<Value>,
}
fn validate_viewer_grant_decisions(prepared: &PreparedReview, req: &CommitRequest) -> Result<(), AppError> {
    let required: BTreeSet<(String, String)> = prepared.manifest["viewer_grants"].as_array().into_iter().flatten()
        .map(|g| (text(g, "id").into(), text(g, "revision").into())).collect();
    let mut supplied = BTreeSet::new();
    for decision in &req.viewer_grant_decisions {
        if decision.grant_id.is_empty() || decision.revision.is_empty()
            || !supplied.insert((decision.grant_id.clone(), decision.revision.clone()))
            || matches!(&decision.choice, ViewerGrantChoice::Remove { confirm: false }) {
            return Err(AppError::BadRequest("Review an exact preserve or confirmed remove choice for each page viewer grant".into()));
        }
    }
    if supplied != required { return Err(AppError::BadRequest("Review every current page viewer grant and its original revision".into())); }
    Ok(())
}
async fn plan_viewer_grants(
    conn: &mut AnyConnection, auth: &AuthUser, request: &MovePreviewRequest,
    before: &PreparedReview, original: &BTreeMap<String, Vec<Value>>, req: &CommitRequest,
    now: &str, escrow: &mut Vec<EscrowEntry>,
) -> Result<Vec<ViewerGrantPlan>, AppError> {
    validate_viewer_grant_decisions(before, req)?;
    let mut plans = Vec::new();
    for decision in &req.viewer_grant_decisions {
        let grant = before.manifest["viewer_grants"].as_array().into_iter().flatten()
            .find(|g| g["id"] == decision.grant_id && g["revision"] == decision.revision)
            .ok_or_else(review_conflict)?.clone();
        if !table(original, "document_viewer_grants").contains(&grant) { return Err(review_conflict()); }
        let source = text(&grant, "space_id");
        super::super::page_viewer_grant_management::authority(conn, auth, source, text(&grant, "document_id"), true, false).await?;
        let expected = match &decision.choice {
            ViewerGrantChoice::Preserve => {
                require_viewer_grant_preservation(conn, auth, source, request, &grant, true).await?;
                let mut next = grant.clone();
                next["space_id"] = json!(request.destination.space_id);
                next["revision"] = json!(uuid::Uuid::new_v4().to_string());
                next["updated_at"] = json!(now);
                Some(next)
            }
            ViewerGrantChoice::Remove { confirm: true } => None,
            ViewerGrantChoice::Remove { confirm: false } => return Err(review_conflict()),
        };
        escrow.push(EscrowEntry { document_id: text(&grant, "document_id").into(), family: "page_viewer_grant".into(), source_id: text(&grant, "id").into(),
            disposition: if expected.is_some() { "preserve" } else { "remove" }.into(), original: grant.clone(), expected: json!(expected) });
        plans.push(ViewerGrantPlan { original: grant, expected });
    }
    Ok(plans)
}
/// Grant policy dependencies are unchanged by relocation. Keep full account,
/// block and account-role union evidence after all final typed-policy waits.
async fn viewer_move_dependencies(conn: &mut AnyConnection, source: &str, destination: &str) -> Result<Vec<RawReviewProof>, AppError> {
    let mut proofs = Vec::new();
    for table in ["users", "groups", "group_members", "group_global_roles", "global_roles"] {
        let query = if table == "users" { "SELECT id,display_name,role,is_active,account_kind FROM users".to_string() } else { format!("SELECT * FROM {table}") };
        proofs.push(RawReviewProof { rows: rows_raw(conn, &query, &[]).await?, query, parameters: Vec::new() });
    }
    for (table, key) in [("spaces", "id"), ("space_members", "space_id"), ("space_group_permissions", "space_id"), ("space_roles", "space_id"), ("space_user_blocks", "space_id")] {
        let query = format!("SELECT * FROM {table} WHERE {key}=$1 OR {key}=$2");
        proofs.push(RawReviewProof { rows: rows_raw(conn, &query, &[source, destination]).await?, query, parameters: vec![source.into(), destination.into()] });
    }
    Ok(proofs)
}
async fn verify_final_viewer_grants(
    conn: &mut AnyConnection, auth: &AuthUser, source: &str, destination: &str,
    plans: &[ViewerGrantPlan], dependencies: &[RawReviewProof],
) -> Result<(), AppError> {
    if plans.is_empty() { return Ok(()); }
    super::super::page_viewer_grant_management::move_space_authority(conn, auth, source, true, plans.iter().any(|p| p.expected.is_some())).await?;
    for plan in plans.iter().filter(|p| p.expected.is_some()) {
        let grant = plan.expected.as_ref().ok_or_else(review_conflict)?;
        super::super::page_viewer_grant_management::authority(conn, auth, destination, text(grant, "document_id"), true, true).await?;
        super::super::page_viewer_grant_management::recipient(conn, destination, text(grant, "user_id"), true).await?;
    }
    for proof in dependencies {
        let params: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if rows_raw(conn, &proof.query, &params).await? != proof.rows { return Err(review_conflict()); }
    }
    Ok(())
}
fn viewer_grant_projection_tuple(raw: &Value, recipient: &Value, available: bool) -> Value {
    json!({"grant_id":raw["id"],"document_id":raw["document_id"],"space_id":raw["space_id"],"revision":raw["revision"],"role":"viewer","scope":"page",
        "recipient":{"available":available,"display_name":if available {recipient["display_name"].clone()} else {Value::Null}}})
}

fn validate_decisions(prepared: &PreparedReview, req: &CommitRequest) -> Result<(), AppError> {
    if !req.confirm_audience_change || !req.confirm_draft_space_change {
        return Err(AppError::BadRequest(
            "Review and confirm move audience and draft space consequences".into(),
        ));
    }
    if prepared.review.fingerprint != req.fingerprint {
        return Err(AppError::Conflict(
            "Move preview changed; review again".into(),
        ));
    }
    validate_viewer_grant_decisions(prepared, req)?;
    plan_identifiers(prepared, req)?;
    plan_maintenance(prepared, req)?;
    let mut required = BTreeSet::new();
    let metadata = &prepared.review.inventory["metadata"];
    if let Some(items) = metadata.as_array() {
        for item in items {
            let family = match text(item, "kind") {
                "tag" => MetadataFamily::Tag,
                "property" => MetadataFamily::Property,
                "workflow" => MetadataFamily::Workflow,
                _ => {
                    return Err(AppError::Conflict(
                        "Move metadata inventory needs review".into(),
                    ))
                }
            };
            let id = text(&item["source_definition"], "id");
            if id.is_empty() {
                return Err(AppError::Conflict(
                    "Move metadata inventory needs review".into(),
                ));
            }
            required.insert((family, id.to_string()));
        }
    }
    let mut supplied = BTreeSet::new();
    for decision in &req.decisions {
        if !supplied.insert((decision.family.clone(), decision.source_id.clone())) {
            return Err(AppError::BadRequest(
                "Duplicate move metadata decision".into(),
            ));
        }
        match &decision.choice {
            MetadataChoice::Remove { confirm: false } => {
                return Err(AppError::BadRequest(
                    "Confirm metadata removal; original evidence will remain recoverable".into(),
                ))
            }
            MetadataChoice::Map { destination_id, .. } if destination_id.is_empty() => {
                return Err(AppError::BadRequest(
                    "Choose an exact destination metadata definition".into(),
                ))
            }
            MetadataChoice::Create { definition, .. } if !definition.is_object() => {
                return Err(AppError::BadRequest(
                    "Supply an explicit destination definition".into(),
                ))
            }
            _ => {}
        }
    }
    if supplied != required {
        return Err(AppError::BadRequest("A map, create, preserve or confirmed remove decision is required for every source definition".into()));
    }
    Ok(())
}
/// Preserved metadata is inactive historical evidence, never an editable workflow
/// or a reconstructed property value. No restore/write API exists without fresh mapping proof.
/// Fresh POST is an inspection transport, not a Write request. Retain the
/// original credential and use the same Read admission/core as canonical GET.
pub(super) struct EvidenceReadAuth(AuthUser);
impl<S> axum::extract::FromRequestParts<S> for EvidenceReadAuth
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = crate::auth::middleware::AuthError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        crate::auth::middleware::authenticate_request(
            parts,
            &<AppState as axum::extract::FromRef<S>>::from_ref(state),
            crate::auth::middleware::ApiKeyRequestOperation::Read,
        )
        .await
        .map(Self)
    }
}
pub(super) async fn inspect_preserved(
    state: State<AppState>,
    EvidenceReadAuth(auth): EvidenceReadAuth,
    path: Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    read_preserved(state, auth, path).await
}

const PRESERVED_ESCROW_QUERY: &str = "SELECT op_id,document_id,family,source_id,disposition,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1";
const PRESERVED_RECEIPT_COLUMNS: &str = "op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at";
fn preserved_document_bound(expected: &Value, escrow: &[Value], document: &str) -> bool {
    expected["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|page| page["id"].as_str() == Some(document))
        || escrow.iter().any(|row| {
            row["document_id"].as_str() == Some(document) && row["family"] == "incident_relation"
        })
}
fn verify_captured_preserved_escrow(expected: &Value, escrow: &[Value]) -> Result<(), AppError> {
    let binding = crate::storage::relocation::escrow_binding(escrow)?;
    if expected["escrow_count"] != binding["escrow_count"]
        || expected["escrow_fingerprint"] != binding["escrow_fingerprint"]
    {
        return Err(review_conflict());
    }
    Ok(())
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreservedIndexQuery {
    cursor: Option<String>,
}
struct PreservedIndexWitness {
    receipt: Value,
    escrow: Vec<Value>,
}
async fn bound_index_witness(
    conn: &mut AnyConnection,
    receipt: Value,
    document: &str,
) -> Result<Option<PreservedIndexWitness>, AppError> {
    let op = text(&receipt, "op_id");
    // A SQL label alone cannot establish ownership of historical evidence.
    // Prove the complete sealed receipt before trusting its page bindings or
    // allowing corruption to be distinguished from an unrelated operation.
    match crate::storage::relocation::verify_sealed_receipt(&crate::storage::data_dir(), &receipt) {
        Ok(_) => {}
        Err(AppError::Conflict(_) | AppError::NotFound(_)) => return Ok(None),
        Err(error) => return Err(error),
    }
    let expected: Value = match serde_json::from_str(text(&receipt, "expected_state_json")) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let escrow = rows_raw(conn, PRESERVED_ESCROW_QUERY, &[op]).await?;
    let moved_page = expected["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|page| page["id"].as_str() == Some(document));
    if let Err(error) = verify_captured_preserved_escrow(&expected, &escrow) {
        // Only sealed moved-page membership authorizes a corruption response.
        // An unverified incident row cannot invent a page/operation binding.
        return if moved_page { Err(error) } else { Ok(None) };
    }
    if !preserved_document_bound(&expected, &escrow, document) {
        return Ok(None);
    }
    Ok(Some(PreservedIndexWitness { receipt, escrow }))
}
/// A fresh, page-bound catalog of sealed evidence references. No body, actor,
/// destination labels, hidden counts, or restoration authority is returned.
pub(super) async fn inspect_preserved_index(
    State(state): State<AppState>,
    EvidenceReadAuth(auth): EvidenceReadAuth,
    Path(document_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<PreservedIndexQuery>,
) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_read(&mut conn, &auth).await?;
    let space: String = sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1")
        .bind(&document_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(unavailable)?;
    access::resolve_page_access(
        &mut conn,
        access::principal_from_auth(&auth)?,
        &space,
        &document_id,
        access::PageOperation::View,
        access::Purpose::Routine,
    )
    .await
    .map_err(conceal)?;
    let pages = BTreeSet::from([document_id.clone()]);
    let proofs = preserved_page_proofs(&mut conn, &auth, &pages).await?;
    let mut witnesses = Vec::new();
    let mut anchor: Option<(String, String)> = None;
    if let Some(cursor) = query.cursor {
        if cursor.is_empty() {
            return Err(unavailable());
        }
        // Missing and unrelated cursors are uniformly unavailable. Binding is
        // required before its timestamp or immutable receipt status is trusted.
        let rows = rows_raw(
            &mut conn,
            &format!("SELECT {PRESERVED_RECEIPT_COLUMNS} FROM move_commit_receipts WHERE op_id=$1"),
            &[&cursor],
        )
        .await?;
        let row = rows.into_iter().next().ok_or_else(unavailable)?;
        let witness = bound_index_witness(&mut conn, row, &document_id)
            .await?
            .ok_or_else(unavailable)?;
        anchor = Some((text(&witness.receipt, "created_at").into(), cursor));
        witnesses.push(witness);
    }
    let mut operations = Vec::new();
    let mut has_more = false;
    let mut scans: Vec<RawReviewProof> = Vec::new();
    loop {
        let (sql, parameters) = if let Some((stamp, op)) = &anchor {
            (format!("SELECT {PRESERVED_RECEIPT_COLUMNS} FROM move_commit_receipts WHERE created_at<$1 OR (created_at=$1 AND op_id<$2) ORDER BY created_at DESC,op_id DESC LIMIT 128"),vec![stamp.clone(),op.clone()])
        } else {
            (format!("SELECT {PRESERVED_RECEIPT_COLUMNS} FROM move_commit_receipts ORDER BY created_at DESC,op_id DESC LIMIT 128"),Vec::new())
        };
        let args: Vec<&str> = parameters.iter().map(String::as_str).collect();
        let raw = rows_raw(&mut conn, &sql, &args).await?;
        scans.push(RawReviewProof {
            query: sql,
            parameters,
            rows: raw.clone(),
        });
        if raw.is_empty() {
            break;
        }
        let full_chunk = raw.len() == 128;
        let mut ordered = raw;
        ordered.sort_by(|a, b| {
            text(b, "created_at")
                .cmp(text(a, "created_at"))
                .then_with(|| text(b, "op_id").cmp(text(a, "op_id")))
        });
        for receipt in ordered {
            anchor = Some((
                text(&receipt, "created_at").into(),
                text(&receipt, "op_id").into(),
            ));
            if let Some(witness) = bound_index_witness(&mut conn, receipt, &document_id).await? {
                if operations.len() == 20 {
                    witnesses.push(witness);
                    has_more = true;
                    break;
                }
                operations.push(json!({"operation_id":witness.receipt["op_id"],"created_at":witness.receipt["created_at"],"kind":"immutable_metadata_evidence"}));
                witnesses.push(witness);
            }
        }
        if has_more || !full_chunk {
            break;
        }
        // Continue keyset scanning hidden/unbound rows; never silently cut off
        // history at an arbitrary global receipt count.
    }
    #[cfg(test)]
    if let Some(hook) = BEFORE_INDEX_FINAL.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    for proof in scans.iter() {
        let args: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if rows_raw(&mut conn, &proof.query, &args).await? != proof.rows {
            return Err(unavailable());
        }
    }
    #[cfg(test)]
    if let Some(hook) = BEFORE_INDEX_WITNESSES.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    for witness in &witnesses {
        let op = text(&witness.receipt, "op_id");
        if immutable_receipt(&mut conn, op).await? != witness.receipt
            || rows_raw(&mut conn, PRESERVED_ESCROW_QUERY, &[op]).await? != witness.escrow
        {
            return Err(review_conflict());
        }
    }
    // Receipt and escrow equality can wait on SQL. Final page/ancestor policy
    // proof belongs AFTER that inventory boundary; identity alone is not View.
    let current = preserved_page_proofs(&mut conn, &auth, &pages).await?;
    if current.len() != proofs.len() {
        return Err(unavailable());
    }
    for (before, after) in proofs.iter().zip(&current) {
        if before.query != after.query
            || before.parameters != after.parameters
            || before.rows != after.rows
        {
            return Err(unavailable());
        }
    }
    for proof in &proofs {
        let args: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if rows_raw(&mut conn, &proof.query, &args).await? != proof.rows {
            return Err(unavailable());
        }
    }
    let next_cursor = if has_more {
        operations
            .last()
            .map(|row| row["operation_id"].clone())
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let checked_at = chrono::Utc::now().to_rfc3339();
    #[cfg(test)]
    if let Some(hook) = AFTER_INDEX_PROOFS.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    access::require_current_human_read(&mut conn, &auth).await?;
    for witness in &witnesses {
        crate::storage::relocation::verify_sealed_receipt(
            &crate::storage::data_dir(),
            &witness.receipt,
        )?;
    }
    Ok(Json(
        json!({"schema_version":1,"space_id":space,"document_id":document_id,"checked_at":checked_at,"operations":operations,"next_cursor":next_cursor}),
    ))
}
#[cfg(test)]
thread_local! {static BEFORE_INDEX_WITNESSES:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);static AFTER_INDEX_PROOFS:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);static BEFORE_INDEX_FINAL:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);}

// Capture only internal witnesses. Neither policy rows nor draft bodies are
// emitted or cached; equality closes the later SQL-wait projection boundary.
async fn preserved_page_proofs(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    pages: &BTreeSet<String>,
) -> Result<Vec<RawReviewProof>, AppError> {
    let mut ctx = ReviewContext {
        conn,
        auth,
        mode: MovePreviewCredential::Read,
        proofs: Vec::new(),
        pages: Vec::new(),
        metadata_spaces: BTreeSet::new(),
        page_authority: BTreeMap::new(),
        metadata_authority: BTreeMap::new(),
    };
    let mut spaces = BTreeSet::new();
    for id in pages {
        let rows = ctx
            .rows("SELECT * FROM documents WHERE id=$1", &[id])
            .await?;
        let page = rows.first().ok_or_else(unavailable)?;
        if !page_available(&mut ctx, page).await.map_err(conceal)? {
            return Err(unavailable());
        }
        spaces.insert(text(page, "space_id").to_owned());
    }
    ctx.rows(
        "SELECT id,role,is_active,account_kind FROM users WHERE id=$1",
        &[&auth.claims.sub],
    )
    .await?;
    for space in spaces {
        for (table, key) in [
            ("spaces", "id"),
            ("space_members", "space_id"),
            ("space_group_permissions", "space_id"),
            ("space_user_blocks", "space_id"),
            ("space_roles", "space_id"),
        ] {
            ctx.rows(&format!("SELECT * FROM {table} WHERE {key}=$1"), &[&space])
                .await?;
        }
    }
    // Group definitions/memberships can independently alter global role or
    // ancestor allowlist eligibility. Keep these witnesses internal.
    for table in [
        "groups",
        "group_members",
        "group_global_roles",
        "global_roles",
    ] {
        ctx.rows(&format!("SELECT * FROM {table}"), &[]).await?;
    }
    Ok(ctx.proofs)
}
fn preserved_reference_ids(family: &str, original: &Value, expected: &Value) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for value in [original, expected] {
        if family == "incident_relation" {
            if let Some(id) = value["assignment"]["value"]
                .as_str()
                .filter(|id| !id.is_empty())
            {
                ids.insert(id.to_owned());
            }
        } else if family == "property"
            && [original, expected]
                .iter()
                .any(|v| v["definition"]["property_type"] == "relation")
        {
            for row in value["assignments"].as_array().into_iter().flatten() {
                if let Some(id) = row["value"].as_str().filter(|id| !id.is_empty()) {
                    ids.insert(id.to_owned());
                }
            }
        }
    }
    ids
}

pub(super) async fn read_preserved(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, document_id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_read(&mut conn, &auth).await?;
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT destination_space_id,manifest_json FROM move_commit_receipts WHERE op_id=$1",
    )
    .bind(&id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let (receipt_space, manifest) = row.ok_or_else(unavailable)?;
    let space: String = sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1")
        .bind(&document_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(unavailable)?;
    access::resolve_page_access(
        &mut conn,
        access::principal_from_auth(&auth)?,
        &space,
        &document_id,
        access::PageOperation::View,
        access::Purpose::Routine,
    )
    .await
    .map_err(conceal)?;
    // Project only from the exact full operation snapshot whose digest is bound
    // to the sealed receipt below, never from a separate unbound early query.
    let escrow_snapshot = rows_raw(&mut conn, "SELECT op_id,document_id,family,source_id,disposition,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1", &[&id]).await?;
    let mut rows: Vec<Value> = escrow_snapshot.iter()
        .filter(|row| row["document_id"].as_str() == Some(document_id.as_str()) && ["tag","property","workflow","incident_relation","identifier","maintenance","page_viewer_grant"].contains(&text(row,"family")))
        .map(|row| json!({"family":row["family"],"source_id":row["source_id"],"disposition":row["disposition"],"original_json":row["original_json"],"expected_json":row["expected_json"]})).collect();
    rows.sort_by_cached_key(Value::to_string);
    // No draft/page body is stored in this metadata projection. Exact current page
    // proof is followed by final receipt+escrow tuple equality before projection.
    access::resolve_page_access(
        &mut conn,
        access::principal_from_auth(&auth)?,
        &space,
        &document_id,
        access::PageOperation::View,
        access::Purpose::Routine,
    )
    .await
    .map_err(conceal)?;
    let current: Option<(String, String)> = sqlx::query_as(
        "SELECT destination_space_id,manifest_json FROM move_commit_receipts WHERE op_id=$1",
    )
    .bind(&id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    if current!=Some((receipt_space,manifest))||rows_raw(&mut conn,"SELECT family,source_id,disposition,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1 AND document_id=$2 AND family IN ('tag','property','workflow','incident_relation','identifier','maintenance','page_viewer_grant')",&[&id,&document_id]).await?!=rows{return Err(unavailable())}
    access::resolve_page_access(
        &mut conn,
        access::principal_from_auth(&auth)?,
        &space,
        &document_id,
        access::PageOperation::View,
        access::Purpose::Routine,
    )
    .await
    .map_err(conceal)?;
    #[cfg(test)]
    if let Some(hook) = BEFORE_EVIDENCE_REFERENCES.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    let full_receipt = immutable_receipt(&mut conn, &id).await?;
    // Receipt membership is not authority until the logical receipt itself is
    // sealed. A foreign corrupted receipt must remain indistinguishable from a
    // missing operation, including forged expected-page membership.
    crate::storage::relocation::verify_sealed_receipt(&crate::storage::data_dir(), &full_receipt)
        .map_err(|_| unavailable())?;
    let expected: Value = serde_json::from_str(text(&full_receipt, "expected_state_json"))
        .map_err(|_| unavailable())?;
    let moved_page = expected["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|page| page["id"].as_str() == Some(document_id.as_str()));
    // External incoming relation owners have legitimate document-scoped sealed
    // escrow even though they are not moved pages. An unrelated readable page
    // must not turn an arbitrary operation ID into an existence oracle.
    // Binding must survive hidden relation projection: use the original scoped
    // row membership, whose complete operation binding is verified below.
    let incident_owner = escrow_snapshot.iter().any(|row| {
        row["document_id"].as_str() == Some(document_id.as_str())
            && row["family"] == "incident_relation"
    });
    if !moved_page && !incident_owner {
        return Err(unavailable());
    }
    let snapshot_binding = crate::storage::relocation::escrow_binding(&escrow_snapshot)?;
    if expected["escrow_count"] != snapshot_binding["escrow_count"]
        || expected["escrow_fingerprint"] != snapshot_binding["escrow_fingerprint"]
    {
        // A sealed moved-page proves audience even when its metadata is corrupt.
        // Incident ownership comes only from escrow, so corrupted or forged raw
        // incident rows cannot establish an audience for this error either.
        return Err(if moved_page {
            review_conflict()
        } else {
            unavailable()
        });
    }
    let maintenance_catalog = if rows.iter().any(|row| text(row, "family") == "maintenance") {
        Some(
            super::super::maintenance::inspect_targets(
                &mut conn,
                &auth,
                &space,
                super::super::maintenance::TargetCredential::Read,
            )
            .await?,
        )
    } else {
        None
    };
    let mut evidence = Vec::new();
    let mut viewer_evidence_witnesses: Vec<(String, Value, bool)> = Vec::new();
    let mut projected_pages = BTreeSet::from([document_id.clone()]);
    for row in rows {
        let original: Value = serde_json::from_str(text(&row, "original_json"))
            .map_err(|_| AppError::Conflict("Preserved metadata needs review".into()))?;
        let expected: Value = serde_json::from_str(text(&row, "expected_json"))
            .map_err(|_| AppError::Conflict("Preserved metadata needs review".into()))?;
        if text(&row, "family") == "page_viewer_grant" {
            // Current page viewing alone does not entitle recipient/catalog labels.
            // Omit this family, without a hidden count, for ordinary non-managers.
            if !grant_move_eligible(super::super::page_viewer_grant_management::authority(
                &mut conn, &auth, &space, &document_id, false, false,
            ).await)? { continue; }
            if original["document_id"] != document_id || original["id"] != row["source_id"]
                || original["role"] != "viewer" || original["scope"] != "page"
                || (!expected.is_null() && (expected["document_id"] != document_id || expected["id"] != original["id"] || expected["user_id"] != original["user_id"] || expected["role"] != "viewer" || expected["scope"] != "page")) {
                return Err(review_conflict());
            }
            let user = text(&original, "user_id").to_owned();
            let recipient = super::super::page_viewer_grant_management::recipient(&mut conn, &space, &user, false).await?;
            let available = grant_move_eligible(super::super::page_viewer_grant_management::recipient(&mut conn, &space, &user, true).await)?;
            let projected = viewer_grant_projection_tuple(&original, &recipient, available);
            let projected_expected = if expected.is_null() { Value::Null } else { viewer_grant_projection_tuple(&expected, &recipient, available) };
            evidence.push(json!({"family":row["family"],"source_id":row["source_id"],"disposition":row["disposition"],"original":projected,"expected_active":projected_expected}));
            viewer_evidence_witnesses.push((user, recipient, available));
            continue;
        }
        if text(&row, "family") == "maintenance" {
            let catalog = &maintenance_catalog
                .as_ref()
                .ok_or_else(unavailable)?
                .catalog;
            let visible = [&original, &expected].iter().all(|pair| {
                let user = pair["maintenance_user_id"].as_str();
                let group = pair["maintenance_group_id"].as_str();
                user.is_none_or(|id| {
                    catalog["users"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|u| u["id"].as_str() == Some(id))
                }) && group.is_none_or(|id| {
                    catalog["groups"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|g| g["id"].as_str() == Some(id))
                })
            });
            if !visible {
                continue;
            }
        }
        if !preserved_references_visible(
            &mut conn,
            &auth,
            text(&row, "family"),
            &original,
            &expected,
        )
        .await?
        {
            continue;
        }
        projected_pages.extend(preserved_reference_ids(
            text(&row, "family"),
            &original,
            &expected,
        ));
        evidence.push(json!({"family":row["family"],"source_id":row["source_id"],"disposition":row["disposition"],"original":original,"expected_active":expected}));
    }
    let mut proofs = preserved_page_proofs(&mut conn, &auth, &projected_pages).await?;
    for (user, recipient, _) in &viewer_evidence_witnesses {
        proofs.push(RawReviewProof { query: "SELECT id,display_name,email,role,is_active,account_kind FROM users WHERE id=$1".into(), parameters: vec![user.clone()], rows: vec![recipient.clone()] });
    }
    // Related targets and source proof can wait. Reprove the actual source and
    // original Read credential after those waits; no SQL follows before projection.
    access::resolve_page_access(
        &mut conn,
        access::principal_from_auth(&auth)?,
        &space,
        &document_id,
        access::PageOperation::View,
        access::Purpose::Routine,
    )
    .await
    .map_err(conceal)?;
    #[cfg(test)]
    if let Some(hook) = BEFORE_EVIDENCE_IDENTITY.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    crate::storage::relocation::verify_escrow(&mut conn, &full_receipt).await?;
    if let Some(expected) = maintenance_catalog {
        if super::super::maintenance::inspect_targets(
            &mut conn,
            &auth,
            &space,
            super::super::maintenance::TargetCredential::Read,
        )
        .await?
            != expected
        {
            return Err(unavailable());
        }
    }
    // All escrow/catalog reads have completed. Reprove source and projected
    // targets, then compare their exact policy/source witnesses after those
    // awaited resolver calls. LAST Read remains the original credential.
    let mut current_proofs = preserved_page_proofs(&mut conn, &auth, &projected_pages).await?;
    for (user, _, _) in &viewer_evidence_witnesses {
        let query = "SELECT id,display_name,email,role,is_active,account_kind FROM users WHERE id=$1";
        current_proofs.push(RawReviewProof { query: query.into(), parameters: vec![user.clone()], rows: rows_raw(&mut conn, query, &[user]).await? });
    }
    if current_proofs.len() != proofs.len() {
        return Err(unavailable());
    }
    for (before, after) in proofs.iter().zip(&current_proofs) {
        if before.query != after.query
            || before.parameters != after.parameters
            || before.rows != after.rows
        {
            return Err(unavailable());
        }
    }
    if immutable_receipt(&mut conn, &id).await? != full_receipt {
        return Err(unavailable());
    }
    for (user, expected_recipient, expected_available) in &viewer_evidence_witnesses {
        super::super::page_viewer_grant_management::authority(&mut conn, &auth, &space, &document_id, false, false).await?;
        let current = super::super::page_viewer_grant_management::recipient(&mut conn, &space, user, false).await?;
        let available = grant_move_eligible(super::super::page_viewer_grant_management::recipient(&mut conn, &space, user, true).await)?;
        if current != *expected_recipient || available != *expected_available { return Err(unavailable()); }
    }
    for proof in &proofs {
        let parameters: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if rows_raw(&mut conn, &proof.query, &parameters).await? != proof.rows {
            return Err(unavailable());
        }
    }
    // Bind the emitted projection and page-membership proof to the exact sealed
    // set even if an early tamper was restored before fresh whole-set validation.
    if rows_raw(&mut conn, "SELECT op_id,document_id,family,source_id,disposition,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1", &[&id]).await? != escrow_snapshot {
        return Err(review_conflict());
    }
    access::require_current_human_read(&mut conn, &auth).await?;
    crate::storage::relocation::verify_sealed_receipt(&crate::storage::data_dir(), &full_receipt)?;
    Ok(Json(
        json!({"operation_id":id,"document_id":document_id,"space_id":space,"kind":"immutable_metadata_evidence","editable":false,"restoration_available":false,"entries":evidence}),
    ))
}

async fn preserved_references_visible(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    family: &str,
    original: &Value,
    expected: &Value,
) -> Result<bool, AppError> {
    let mut targets = BTreeSet::new();
    if family == "incident_relation" {
        for evidence in [original, expected] {
            if let Some(assignment) = evidence.get("assignment").filter(|a| !a.is_null()) {
                let Some(value) = assignment.get("value").and_then(Value::as_str) else {
                    return Ok(false);
                };
                if !value.is_empty() {
                    targets.insert(value.to_string());
                }
            }
        }
    } else if family == "property"
        && [original, expected]
            .iter()
            .any(|e| text(&e["definition"], "property_type") == "relation")
    {
        for evidence in [original, expected] {
            let Some(assignments) = evidence.get("assignments").and_then(Value::as_array) else {
                return Ok(false);
            };
            for assignment in assignments {
                let Some(value) = assignment.get("value").and_then(Value::as_str) else {
                    return Ok(false);
                };
                if !value.is_empty() {
                    targets.insert(value.to_string());
                }
            }
        }
    }
    for target in targets {
        let space: Option<String> =
            sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1")
                .bind(&target)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        let Some(space) = space else { return Ok(false) };
        match access::resolve_page_access(
            conn,
            access::principal_from_auth(auth)?,
            &space,
            &target,
            access::PageOperation::View,
            access::Purpose::Routine,
        )
        .await
        {
            Ok(_) => {}
            Err(AppError::NotFound(_) | AppError::Forbidden(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}
#[cfg(test)]
thread_local! {static BEFORE_EVIDENCE_REFERENCES:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);static BEFORE_EVIDENCE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);}

#[derive(Clone)]
struct EscrowEntry {
    document_id: String,
    family: String,
    source_id: String,
    disposition: String,
    original: Value,
    expected: Value,
}
#[derive(Clone)]
struct MetadataPlan {
    table: String,
    source: Value,
    target: Option<Value>,
    assignments: Vec<Value>,
    events: Vec<Value>,
    choice: MetadataChoice,
}
fn review_conflict() -> AppError {
    AppError::Conflict(
        "Move source or metadata needs review; no canonical state was changed".into(),
    )
}
const DOMAIN_TABLES: &[&str] = &[
    "documents",
    "folders",
    "tags",
    "properties_schema",
    "property_values",
    "doc_tags",
    "workflow_templates",
    "document_workflows",
    "document_workflow_events",
    "doc_versions",
    "document_drafts",
    "document_draft_access",
    "attachments",
    "excalidraw_files",
    "document_access_policies",
    "document_viewer_grants",
    "document_access_allowlist",
    "comments",
    "reactions",
    "starred_documents",
    "doc_subscriptions",
    "draft_birth_receipts",
    "draft_transfer_receipts",
    "draft_save_replay_receipts",
    "publication_receipts",
    "canonical_revision_receipts",
    "attachment_upload_receipts",
    "attachment_families",
    "attachment_version_records",
    "attachment_version_changes",
    "attachment_retirements",
    "attachment_audience_origins",
    "attachment_audience_bindings",
];
const RETAINED_RECEIPT_TABLES: &[&str] = &[
    "draft_birth_receipts", "draft_transfer_receipts", "draft_save_replay_receipts",
    "publication_receipts", "canonical_revision_receipts",
    "attachment_upload_receipts", "attachment_families",
    "attachment_version_records", "attachment_version_changes", "attachment_retirements",
    "attachment_audience_origins", "attachment_audience_bindings",
];
async fn domain_state(conn: &mut AnyConnection) -> Result<BTreeMap<String, Vec<Value>>, AppError> {
    let mut state = BTreeMap::new();
    for table in DOMAIN_TABLES {
        state.insert(
            (*table).into(),
            rows_raw(conn, &format!("SELECT * FROM {table}"), &[]).await?,
        );
    }
    state.insert("viewer_grant_recipients".into(),rows_raw(conn,"SELECT u.id,u.role,u.is_active,u.account_kind FROM users u WHERE EXISTS(SELECT 1 FROM document_viewer_grants g WHERE g.user_id=u.id)",&[]).await?);
    Ok(state)
}
fn table<'a>(state: &'a BTreeMap<String, Vec<Value>>, name: &str) -> &'a [Value] {
    state.get(name).map(Vec::as_slice).unwrap_or_default()
}
fn rewrite(
    state: &mut BTreeMap<String, Vec<Value>>,
    name: &str,
    old: &Value,
    new: Option<Value>,
) -> Result<(), AppError> {
    let rows = state.get_mut(name).ok_or_else(review_conflict)?;
    let Some(i) = rows.iter().position(|r| r == old) else {
        return Err(review_conflict());
    };
    rows.remove(i);
    if let Some(row) = new {
        rows.push(row)
    }
    rows.sort_by_cached_key(Value::to_string);
    Ok(())
}
fn insert_expected(
    state: &mut BTreeMap<String, Vec<Value>>,
    name: &str,
    new: Value,
) -> Result<(), AppError> {
    let rows = state.get_mut(name).ok_or_else(review_conflict)?;
    rows.push(new);
    rows.sort_by_cached_key(Value::to_string);
    Ok(())
}

fn relation_conflict() -> AppError {
    AppError::Conflict("Move relation choices unavailable; no canonical state was changed".into())
}
fn crossing_relations(manifest: &Value) -> Vec<&Value> {
    let pages: BTreeSet<&str> = manifest["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| text(p, "id"))
        .collect();
    manifest["relations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            !text(r, "value").is_empty()
                && (pages.contains(text(r, "document_id")) != pages.contains(text(r, "value")))
        })
        .collect()
}
async fn incident_spaces(
    conn: &mut AnyConnection,
    manifest: &Value,
    source: &str,
    destination: &str,
) -> Result<Vec<String>, AppError> {
    let mut spaces = BTreeSet::from([source.to_string(), destination.to_string()]);
    for relation in crossing_relations(manifest) {
        let space: String = sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1")
            .bind(text(relation, "document_id"))
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(relation_conflict)?;
        spaces.insert(space);
    }
    Ok(spaces.into_iter().collect())
}
#[derive(Clone)]
struct IncidentPlan {
    source_page: Value,
    original: Value,
    expected: Option<Value>,
    outgoing: bool,
}
async fn incident_page_edit(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    page: &Value,
) -> Result<(), AppError> {
    access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        text(page, "space_id"),
        text(page, "id"),
        access::PageOperation::Edit,
        access::Purpose::Routine,
    )
    .await
    .map(|_| ())
    .map_err(|e| match e {
        AppError::NotFound(_) | AppError::Forbidden(_) => relation_conflict(),
        other => other,
    })
}
async fn plan_incident_relations(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    manifest: &Value,
    state: &BTreeMap<String, Vec<Value>>,
    metadata: &[MetadataPlan],
    req: &CommitRequest,
    destination: &str,
    escrow: &mut Vec<EscrowEntry>,
) -> Result<Vec<IncidentPlan>, AppError> {
    let mut plans = Vec::new();
    let pages: BTreeSet<&str> = manifest["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| text(p, "id"))
        .collect();
    let relations = crossing_relations(manifest);
    // Authorization precedes choice-set errors; hidden sources never emit labels/IDs.
    for relation in &relations {
        if !pages.contains(text(relation, "document_id")) {
            let page = table(state, "documents")
                .iter()
                .find(|p| text(p, "id") == text(relation, "document_id"))
                .ok_or_else(relation_conflict)?;
            incident_page_edit(conn, auth, page).await?;
        }
    }
    let required: BTreeSet<&str> = relations.iter().map(|r| text(r, "id")).collect();
    let mut supplied = BTreeMap::new();
    for decision in &req.incident_relations {
        if decision.assignment_id.is_empty()
            || supplied
                .insert(decision.assignment_id.as_str(), &decision.choice)
                .is_some()
        {
            return Err(relation_conflict());
        }
    }
    if required != supplied.keys().copied().collect() {
        return Err(relation_conflict());
    }
    for relation in relations {
        let original = table(state, "property_values")
            .iter()
            .find(|v| text(v, "id") == text(relation, "id"))
            .cloned()
            .ok_or_else(relation_conflict)?;
        if original
            .as_object()
            .ok_or_else(relation_conflict)?
            .iter()
            .any(|(k, v)| relation.get(k) != Some(v))
        {
            return Err(relation_conflict());
        }
        let source_page = table(state, "documents")
            .iter()
            .find(|p| text(p, "id") == text(&original, "document_id"))
            .cloned()
            .ok_or_else(relation_conflict)?;
        let definition = table(state, "properties_schema")
            .iter()
            .find(|p| text(p, "id") == text(&original, "property_id"))
            .cloned()
            .ok_or_else(relation_conflict)?;
        if text(&definition, "property_type") != "relation"
            || text(&definition, "space_id") != text(&source_page, "space_id")
            || text(relation, "schema_space_id") != text(&definition, "space_id")
        {
            return Err(relation_conflict());
        }
        let outgoing = pages.contains(text(&original, "document_id"));
        let choice = supplied[text(&original, "id")];
        let mut expected = match choice {
            IncidentRelationChoice::Map { target_id } if !target_id.is_empty() => {
                let mut row = original.clone();
                row["value"] = json!(target_id);
                Some(row)
            }
            IncidentRelationChoice::Preserve | IncidentRelationChoice::Remove { confirm: true } => {
                None
            }
            _ => return Err(relation_conflict()),
        };
        if outgoing {
            let plan = metadata
                .iter()
                .find(|p| {
                    p.table == "properties_schema"
                        && text(&p.source, "id") == text(&original, "property_id")
                })
                .ok_or_else(relation_conflict)?;
            match (&plan.choice, choice) {
                (
                    MetadataChoice::Map { values, .. } | MetadataChoice::Create { values, .. },
                    IncidentRelationChoice::Map { target_id },
                ) => {
                    if values.get(text(&original, "id")) != Some(target_id) {
                        return Err(relation_conflict());
                    }
                    let target = plan.target.as_ref().ok_or_else(relation_conflict)?;
                    expected.as_mut().unwrap()["property_id"] = target["id"].clone();
                }
                (
                    MetadataChoice::Map { values, .. } | MetadataChoice::Create { values, .. },
                    IncidentRelationChoice::Preserve
                    | IncidentRelationChoice::Remove { confirm: true },
                ) => {
                    if values.contains_key(text(&original, "id")) {
                        return Err(relation_conflict());
                    }
                }
                (MetadataChoice::Preserve, IncidentRelationChoice::Preserve)
                | (
                    MetadataChoice::Remove { confirm: true },
                    IncidentRelationChoice::Remove { confirm: true },
                ) => {}
                _ => return Err(relation_conflict()),
            }
            let existing = escrow
                .iter_mut()
                .find(|e| {
                    e.family == "property"
                        && e.source_id == text(&original, "property_id")
                        && e.document_id == text(&original, "document_id")
                })
                .ok_or_else(relation_conflict)?;
            let assignments = existing.expected["assignments"]
                .as_array_mut()
                .ok_or_else(relation_conflict)?;
            assignments.retain(|v| text(v, "id") != text(&original, "id"));
            if let Some(row) = &expected {
                assignments.push(row.clone());
                assignments.sort_by_cached_key(Value::to_string);
            }
        } else if let Some(row) = &expected {
            let assignments = [(text(row, "property_id"), row["value"].as_str())];
            let targets = super::super::property_validation::validate_assignment_batch(
                conn,
                text(&source_page, "space_id"),
                access::principal_from_auth(auth)?,
                &assignments,
            )
            .await
            .map_err(|e| match e {
                AppError::BadRequest(_) | AppError::NotFound(_) | AppError::Forbidden(_) => {
                    relation_conflict()
                }
                other => other,
            })?;
            super::super::property_validation::revalidate_relation_targets(
                conn,
                text(&source_page, "space_id"),
                access::principal_from_auth(auth)?,
                &targets,
            )
            .await?;
        }
        let disposition = match choice {
            IncidentRelationChoice::Map { .. } => "map",
            IncidentRelationChoice::Preserve => "preserve",
            IncidentRelationChoice::Remove { .. } => "remove",
        };
        escrow.push(EscrowEntry{document_id:text(&original,"document_id").into(),family:"incident_relation".into(),source_id:text(&original,"id").into(),disposition:disposition.into(),original:json!({"definition":definition,"assignment":original}),expected:json!({"assignment":expected,"space_id":if outgoing{destination}else{text(&source_page,"space_id")}})});
        plans.push(IncidentPlan {
            source_page,
            original,
            expected,
            outgoing,
        });
    }
    Ok(plans)
}
fn incident_changes(plans: &[IncidentPlan]) -> Vec<RowChange> {
    plans
        .iter()
        .filter(|p| !p.outgoing)
        .map(|p| RowChange {
            table: "property_values".into(),
            old: Some(p.original.clone()),
            new: p.expected.clone(),
        })
        .collect()
}
async fn validate_incident_outcomes(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    plans: &[IncidentPlan],
) -> Result<(), AppError> {
    for plan in plans.iter().filter(|p| !p.outgoing) {
        incident_page_edit(conn, auth, &plan.source_page).await?;
        let actual = rows_raw(
            conn,
            "SELECT * FROM property_values WHERE id=$1",
            &[text(&plan.original, "id")],
        )
        .await?;
        if actual != plan.expected.clone().into_iter().collect::<Vec<_>>() {
            return Err(relation_conflict());
        }
        if let Some(row) = &plan.expected {
            let assignments = [(text(row, "property_id"), row["value"].as_str())];
            let targets = super::super::property_validation::validate_assignment_batch(
                conn,
                text(&plan.source_page, "space_id"),
                access::principal_from_auth(auth)?,
                &assignments,
            )
            .await?;
            super::super::property_validation::revalidate_relation_targets(
                conn,
                text(&plan.source_page, "space_id"),
                access::principal_from_auth(auth)?,
                &targets,
            )
            .await?;
        }
    }
    Ok(())
}
async fn require_folder_move_spaces(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    source: &str,
    destination: &str,
    manifest: &Value,
) -> Result<(), AppError> {
    if manifest["folders"].as_array().map_or(true, Vec::is_empty) {
        return Ok(());
    }
    for space in [source, destination] {
        let current = access::resolve_space_access(
            conn,
            access::principal_from_auth(auth)?,
            space,
            access::Purpose::Routine,
        )
        .await
        .map_err(conceal)?;
        if !current.allows(access::Permission::ManageFolders) {
            return Err(unavailable());
        }
    }
    Ok(())
}
pub(super) async fn require_create_metadata(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    destination: &str,
    workflow: bool,
) -> Result<(), AppError> {
    let current = access::resolve_space_access(
        conn,
        access::principal_from_auth(auth)?,
        destination,
        access::Purpose::Routine,
    )
    .await?;
    let required = if workflow {
        vec![
            access::Permission::ManageTemplates,
            access::Permission::ManageSpace,
        ]
    } else {
        vec![access::Permission::EditContent]
    };
    if required.iter().any(|p| !current.allows(*p)) {
        return Err(unavailable());
    }
    Ok(())
}
fn field_str<'a>(row: &'a Value, key: &str) -> Result<&'a str, AppError> {
    row.get(key)
        .and_then(Value::as_str)
        .ok_or_else(review_conflict)
}
fn normalize_created(
    family: &MetadataFamily,
    definition: &Value,
    destination: &str,
    actor: &str,
    now: &str,
) -> Result<Value, AppError> {
    let object = definition.as_object().ok_or_else(review_conflict)?;
    let allowed: &[&str] = match family {
        MetadataFamily::Tag => &["id", "name", "color"],
        MetadataFamily::Property => &["id", "name", "property_type", "options", "sort_order"],
        MetadataFamily::Workflow => &["id", "name", "description", "stages", "approvers"],
    };
    if object.keys().any(|k| !allowed.contains(&k.as_str()))
        || uuid::Uuid::parse_str(field_str(definition, "id")?).is_err()
    {
        return Err(AppError::BadRequest(
            "Supply exact new definition fields and a fresh UUID".into(),
        ));
    }
    let name = field_str(definition, "name")?;
    if name.trim().is_empty() || name.len() > 500 {
        return Err(AppError::BadRequest(
            "Metadata name must contain 1 to 500 bytes".into(),
        ));
    }
    let mut row = definition.clone();
    row["space_id"] = json!(destination);
    row["created_at"] = json!(now);
    match family {
        MetadataFamily::Tag => {
            if !definition.get("color").is_some_and(Value::is_string) {
                return Err(review_conflict());
            }
        }
        MetadataFamily::Property => {
            let options = definition.get("options").filter(|v| !v.is_null());
            super::super::property_validation::validate_schema(
                name,
                field_str(definition, "property_type")?,
                options,
            )?;
            row["options"] = options.map(|v| json!(v.to_string())).unwrap_or(Value::Null);
            row["sort_order"] = json!(definition
                .get("sort_order")
                .and_then(Value::as_i64)
                .ok_or_else(review_conflict)?);
        }
        MetadataFamily::Workflow => {
            if !definition
                .get("description")
                .is_some_and(|v| v.is_string() || v.is_null())
                || !definition.get("stages").is_some_and(Value::is_array)
                || !definition.get("approvers").is_some_and(Value::is_array)
            {
                return Err(review_conflict());
            }
            row["stages"] = json!(definition["stages"].to_string());
            row["approvers"] = json!(definition["approvers"].to_string());
            row["created_by"] = json!(actor);
            row["updated_at"] = json!(now);
            if !valid_workflow_stages(&row) {
                return Err(review_conflict());
            }
        }
    }
    Ok(row)
}
fn validate_stage_mapping(
    source: &Value,
    target: &Value,
    stages: &BTreeMap<String, String>,
) -> Result<(), AppError> {
    if !valid_workflow_stages(source) || !valid_workflow_stages(target) {
        return Err(review_conflict());
    }
    let mut expected = source.clone();
    let Some(Value::Array(mut src)) = parse_json_field(source, "stages") else {
        return Err(review_conflict());
    };
    let Some(Value::Array(dst)) = parse_json_field(target, "stages") else {
        return Err(review_conflict());
    };
    let original: BTreeSet<String> = src.iter().map(|s| text(s, "id").to_string()).collect();
    if !stages.is_empty() && stages.keys().cloned().collect::<BTreeSet<_>>() != original {
        return Err(AppError::BadRequest(
            "Map every source workflow stage explicitly".into(),
        ));
    }
    for stage in &mut src {
        let id = text(stage, "id").to_string();
        stage["id"] = json!(stages.get(&id).unwrap_or(&id));
    }
    if src != dst {
        return Err(review_conflict());
    }
    expected["stages"] = json!(Value::Array(src).to_string());
    let Some(Value::Array(mut approvers)) = parse_json_field(source, "approvers") else {
        return Err(review_conflict());
    };
    for approver in &mut approvers {
        if nonnull(approver, "stage_id") {
            let id = text(approver, "stage_id").to_string();
            approver["stage_id"] = json!(stages.get(&id).unwrap_or(&id));
        }
    }
    if Some(Value::Array(approvers)) != parse_json_field(target, "approvers") {
        return Err(review_conflict());
    }
    Ok(())
}
async fn plan_metadata(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    destination: &str,
    state: &BTreeMap<String, Vec<Value>>,
    pages: &BTreeSet<String>,
    req: &CommitRequest,
    now: &str,
) -> Result<Vec<MetadataPlan>, AppError> {
    let mut result = Vec::new();
    let mut targets = BTreeSet::new();
    for decision in &req.decisions {
        let (def_table, assignment_table, key) = match decision.family {
            MetadataFamily::Tag => ("tags", "doc_tags", "tag_id"),
            MetadataFamily::Property => ("properties_schema", "property_values", "property_id"),
            MetadataFamily::Workflow => (
                "workflow_templates",
                "document_workflows",
                "workflow_template_id",
            ),
        };
        let source = table(state, def_table)
            .iter()
            .find(|r| text(r, "id") == decision.source_id)
            .cloned()
            .ok_or_else(review_conflict)?;
        let assignments: Vec<Value> = table(state, assignment_table)
            .iter()
            .filter(|r| {
                pages.contains(text(r, "document_id")) && text(r, key) == decision.source_id
            })
            .cloned()
            .collect();
        if assignments.is_empty() {
            return Err(review_conflict());
        }
        let events: Vec<Value> = if decision.family == MetadataFamily::Workflow {
            table(state, "document_workflow_events")
                .iter()
                .filter(|e| {
                    assignments.iter().any(|a| {
                        text(a, "id") == text(e, "document_workflow_id")
                            || text(a, "document_id") == text(e, "document_id")
                    })
                })
                .cloned()
                .collect()
        } else {
            vec![]
        };
        if events.iter().any(|e| {
            !assignments.iter().any(|a| {
                text(a, "id") == text(e, "document_workflow_id")
                    && text(a, "document_id") == text(e, "document_id")
            })
        }) {
            return Err(review_conflict());
        }
        let target = match &decision.choice {
            MetadataChoice::Map { destination_id, .. } => Some(
                table(state, def_table)
                    .iter()
                    .find(|r| text(r, "id") == destination_id && text(r, "space_id") == destination)
                    .cloned()
                    .ok_or_else(review_conflict)?,
            ),
            MetadataChoice::Create { definition, .. } => {
                require_create_metadata(
                    conn,
                    auth,
                    destination,
                    decision.family == MetadataFamily::Workflow,
                )
                .await?;
                let row = normalize_created(
                    &decision.family,
                    definition,
                    destination,
                    &auth.claims.sub,
                    now,
                )?;
                if table(state, def_table).iter().any(|r| {
                    text(r, "id") == text(&row, "id")
                        || text(r, "space_id") == destination
                            && text(r, "name") == text(&row, "name")
                }) {
                    return Err(review_conflict());
                }
                Some(row)
            }
            MetadataChoice::Preserve | MetadataChoice::Remove { confirm: true } => None,
            _ => return Err(review_conflict()),
        };
        if let Some(target) = &target {
            if !targets.insert((def_table.to_string(), text(target, "id").to_string())) {
                return Err(AppError::BadRequest(
                    "Two source definitions cannot silently merge into one destination definition"
                        .into(),
                ));
            }
            match &decision.choice {
                MetadataChoice::Map { values, stages, .. }
                | MetadataChoice::Create { values, stages, .. } => {
                    if decision.family != MetadataFamily::Property && !values.is_empty()
                        || decision.family != MetadataFamily::Workflow && !stages.is_empty()
                    {
                        return Err(review_conflict());
                    }
                    if decision.family == MetadataFamily::Property {
                        let kind = text(&source, "property_type");
                        if kind != text(target, "property_type") {
                            return Err(review_conflict());
                        }
                        let options = parse_json_field(target, "options");
                        if nonnull(target, "options") && options.is_none() {
                            return Err(review_conflict());
                        }
                        super::super::property_validation::validate_schema(
                            text(target, "name"),
                            kind,
                            options.as_ref(),
                        )?;
                        if values
                            .keys()
                            .any(|id| !assignments.iter().any(|a| text(a, "id") == id))
                        {
                            return Err(review_conflict());
                        }
                        for a in &assignments {
                            let value = values
                                .get(text(a, "id"))
                                .map(String::as_str)
                                .or_else(|| a["value"].as_str());
                            super::super::property_validation::validate_value(
                                kind,
                                options.as_ref(),
                                value,
                            )?;
                        }
                    }
                    if decision.family == MetadataFamily::Workflow {
                        validate_stage_mapping(&source, target, stages)?;
                        let mut ctx = ReviewContext {
                            conn,
                            auth,
                            mode: MovePreviewCredential::Write,
                            proofs: vec![],
                            pages: vec![],
                            metadata_spaces: BTreeSet::new(),
                            page_authority: BTreeMap::new(),
                            metadata_authority: BTreeMap::new(),
                        };
                        if !workflow_approvers_valid(&mut ctx, target, destination).await? {
                            return Err(review_conflict());
                        }
                    }
                }
                _ => return Err(review_conflict()),
            }
        }
        result.push(MetadataPlan {
            table: def_table.into(),
            source,
            target,
            assignments,
            events,
            choice: decision.choice.clone(),
        });
    }
    Ok(result)
}
fn bind_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Any, sqlx::any::AnyArguments<'q>>,
    value: &Value,
) -> Result<sqlx::query::Query<'q, sqlx::Any, sqlx::any::AnyArguments<'q>>, AppError> {
    Ok(match value {
        Value::Null => query.bind(Option::<String>::None),
        Value::String(v) => query.bind(v.clone()),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                query.bind(i)
            } else {
                query.bind(v.as_f64().ok_or_else(review_conflict)?)
            }
        }
        Value::Object(v) if v.len() == 1 && v.contains_key("binary_hex") => query.bind(
            hex::decode(v["binary_hex"].as_str().ok_or_else(review_conflict)?)
                .map_err(|_| review_conflict())?,
        ),
        _ => return Err(review_conflict()),
    })
}
/// Only real table columns from our captured canonical row or validated created
/// definitions reach this primitive. Identifiers are also strictly ASCII names.
fn identifiers(table: &str, row: &Value) -> Result<Vec<String>, AppError> {
    if !DOMAIN_TABLES.contains(&table) {
        return Err(review_conflict());
    }
    let obj = row.as_object().ok_or_else(review_conflict)?;
    let mut keys: Vec<String> = obj.keys().cloned().collect();
    keys.sort();
    if keys
        .iter()
        .any(|k| k.is_empty() || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
    {
        return Err(review_conflict());
    }
    Ok(keys)
}
async fn insert_row(
    conn: &mut AnyConnection,
    table_name: &str,
    row: &Value,
) -> Result<(), AppError> {
    let keys = identifiers(table_name, row)?;
    let query = format!(
        "INSERT INTO {table_name} ({}) VALUES({})",
        keys.join(","),
        (1..=keys.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut q = sqlx::query(&query);
    for k in &keys {
        q = bind_value(q, &row[k])?
    }
    if q.execute(conn)
        .await
        .map_err(AppError::internal)?
        .rows_affected()
        != 1
    {
        return Err(review_conflict());
    }
    Ok(())
}
async fn change_row(
    conn: &mut AnyConnection,
    table_name: &str,
    old: &Value,
    new: Option<&Value>,
) -> Result<(), AppError> {
    let keys = identifiers(table_name, old)?;
    let mut values = Vec::new();
    let prefix = if let Some(new) = new {
        if identifiers(table_name, new)? != keys {
            return Err(review_conflict());
        }
        let changed: Vec<_> = keys.iter().filter(|k| old[*k] != new[*k]).collect();
        if changed.is_empty() {
            return Ok(());
        }
        let mut set = Vec::new();
        for k in changed {
            values.push(new[k].clone());
            set.push(format!("{k}=${}", values.len()))
        }
        format!("UPDATE {table_name} SET {}", set.join(","))
    } else {
        format!("DELETE FROM {table_name}")
    };
    let mut clauses = Vec::new();
    for k in &keys {
        if old[k].is_null() {
            // Exact NULL CAS without a TEXT-typed parameter (PG BYTEA/INTEGER).
            clauses.push(format!("{k} IS NULL"));
        } else {
            values.push(old[k].clone());
            clauses.push(format!("{k} IS NOT DISTINCT FROM ${}", values.len()));
        }
    }
    let statement = format!("{prefix} WHERE {}", clauses.join(" AND "));
    let mut q = sqlx::query(&statement);
    for v in &values {
        q = bind_value(q, v)?
    }
    if q.execute(conn)
        .await
        .map_err(AppError::internal)?
        .rows_affected()
        != 1
    {
        return Err(review_conflict());
    }
    Ok(())
}
fn planned_escrow(
    plans: &[MetadataPlan],
    manifest: &Value,
    destination: &str,
) -> Result<Vec<EscrowEntry>, AppError> {
    let mut out = Vec::new();
    for plan in plans {
        let (family, key) = match plan.table.as_str() {
            "tags" => ("tag", "tag_id"),
            "properties_schema" => ("property", "property_id"),
            "workflow_templates" => ("workflow", "workflow_template_id"),
            _ => return Err(review_conflict()),
        };
        let docs: BTreeSet<&str> = plan
            .assignments
            .iter()
            .map(|a| text(a, "document_id"))
            .collect();
        for doc in docs {
            let original: Vec<Value> = plan
                .assignments
                .iter()
                .filter(|a| text(a, "document_id") == doc)
                .cloned()
                .collect();
            let events: Vec<Value> = plan
                .events
                .iter()
                .filter(|e| text(e, "document_id") == doc)
                .cloned()
                .collect();
            let mut assignments = Vec::new();
            let mut new_events = Vec::new();
            if let Some(target) = &plan.target {
                for old in &original {
                    let mut row = old.clone();
                    row[key] = target["id"].clone();
                    if let MetadataChoice::Map { values, stages, .. }
                    | MetadataChoice::Create { values, stages, .. } = &plan.choice
                    {
                        if family == "property" {
                            if let Some(v) = values.get(text(old, "id")) {
                                row["value"] = json!(v)
                            }
                        }
                        if family == "workflow" {
                            let stage = text(old, "current_stage_id");
                            row["current_stage_id"] =
                                json!(stages.get(stage).map(String::as_str).unwrap_or(stage));
                        }
                    }
                    assignments.push(row);
                }
                for old in &events {
                    let mut row = old.clone();
                    if let MetadataChoice::Map { stages, .. }
                    | MetadataChoice::Create { stages, .. } = &plan.choice
                    {
                        for field in ["from_stage_id", "to_stage_id"] {
                            if nonnull(old, field) {
                                let id = text(old, field);
                                row[field] =
                                    json!(stages.get(id).map(String::as_str).unwrap_or(id));
                            }
                        }
                    }
                    new_events.push(row);
                }
            }
            let disposition = match plan.choice {
                MetadataChoice::Map { .. } => "map",
                MetadataChoice::Create { .. } => "create",
                MetadataChoice::Preserve => "preserve",
                MetadataChoice::Remove { .. } => "remove",
            };
            out.push(EscrowEntry{document_id:doc.into(),family:family.into(),source_id:text(&plan.source,"id").into(),disposition:disposition.into(),original:json!({"definition":plan.source,"assignments":original,"events":events}),expected:json!({"definition":plan.target,"assignments":assignments,"events":new_events})});
        }
    }
    // Internal private evidence is retained but never projected by the metadata API.
    if let Some(drafts) = manifest["metadata"]["document_drafts"].as_array() {
        for draft in drafts {
            let mut expected = draft.clone();
            expected["space_id"] = json!(destination);
            let grants: Vec<Value> = manifest["metadata"]["document_draft_access"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| text(a, "draft_id") == text(draft, "id"))
                .cloned()
                .collect();
            out.push(EscrowEntry {
                document_id: text(draft, "document_id").into(),
                family: "private_draft".into(),
                source_id: text(draft, "id").into(),
                disposition: "map".into(),
                original: json!({"draft":draft,"access":grants}),
                expected: json!({"draft":expected,"access":grants}),
            });
        }
    }
    for policy in manifest["metadata"]["document_access_policies"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let doc = text(policy, "document_id");
        let original: Vec<Value> = manifest["metadata"]["document_access_allowlist"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| text(a, "document_id") == doc)
            .cloned()
            .collect();
        let mut expected_policy = policy.clone();
        expected_policy["space_id"] = json!(destination);
        let expected: Vec<Value> = original
            .iter()
            .map(|a| {
                let mut a = a.clone();
                a["space_id"] = json!(destination);
                a
            })
            .collect();
        out.push(EscrowEntry {
            document_id: doc.into(),
            family: "restriction".into(),
            source_id: doc.into(),
            disposition: "map".into(),
            original: json!({"policy":policy,"allowlist":original}),
            expected: json!({"policy":expected_policy,"allowlist":expected}),
        });
    }
    Ok(out)
}
async fn published_source_digest(
    conn: &mut AnyConnection,
    page: &Value,
) -> Result<String, AppError> {
    let args = (
        text(page, "space_id"),
        text(page, "id"),
        text(page, "content_path"),
        text(page, "updated_at"),
        text(page, "published_at"),
    );
    let mut hashes = Vec::new();
    if let Some(hash) = crate::storage::publication::committed_source_digest(
        conn, args.0, args.1, args.2, args.3, args.4,
    )
    .await?
    {
        hashes.push(hash)
    }
    if let Some(hash) = crate::storage::canonical_rewrite::committed_source_digest(
        conn, args.0, args.1, args.2, args.3, args.4,
    )
    .await?
    {
        hashes.push(hash)
    }
    let prior=rows_raw(conn,"SELECT op_id,manifest_json,expected_state_json,source_fingerprint,request_fingerprint FROM move_commit_receipts WHERE destination_space_id=$1",&[args.0]).await?;
    for receipt in prior {
        let receipt = immutable_receipt(conn, text(&receipt, "op_id")).await?;
        crate::storage::relocation::verify_escrow(conn, &receipt).await?;
        crate::storage::relocation::verify_sealed_receipt(&crate::storage::data_dir(), &receipt)?;
        let m: crate::storage::relocation::PreparedRelocation =
            serde_json::from_str(text(&receipt, "manifest_json")).map_err(|_| review_conflict())?;
        let expected: Value = serde_json::from_str(text(&receipt, "expected_state_json"))
            .map_err(|_| review_conflict())?;
        if !crate::storage::relocation::valid_manifest(&m)
            || m.op_id != text(&receipt, "op_id")
            || m.source_fingerprint != text(&receipt, "source_fingerprint")
            || m.request_fingerprint != text(&receipt, "request_fingerprint")
        {
            return Err(review_conflict());
        }
        let matches = expected["pages"].as_array().into_iter().flatten().any(|r| {
            [
                "id",
                "space_id",
                "content_path",
                "updated_at",
                "published_at",
            ]
            .iter()
            .all(|k| r[*k] == page[*k])
        });
        if matches {
            for f in m.files.iter().filter(|f| {
                f.kind == "canonical" && f.subject_id == args.1 && f.destination == args.2
            }) {
                hashes.push(f.sha256.clone());
            }
        }
    }
    if hashes.len() != 1 {
        return Err(AppError::Conflict(
            "Published move source has no exact committed immutable revision proof".into(),
        ));
    }
    Ok(hashes.remove(0))
}
async fn file_plans(
    conn: &mut AnyConnection,
    manifest: &Value,
    destination_slug: &str,
) -> Result<Vec<crate::storage::relocation::FilePlan>, AppError> {
    use crate::storage::relocation::FilePlan;
    let mut plans = Vec::new();
    for page in manifest["pages"].as_array().into_iter().flatten() {
        let source = text(page, "content_path");
        let source_slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
            .bind(text(page, "space_id"))
            .fetch_one(&mut *conn)
            .await
            .map_err(AppError::internal)?;
        if !source.starts_with(&format!("spaces/{source_slug}/docs/")) {
            return Err(review_conflict());
        }
        let hash = if nonnull(page, "published_at") {
            // A current logical published source needs immutable committed revision evidence,
            // not a mutable filename merely matching updated_at or an index fingerprint.
            Some(published_source_digest(conn, page).await?)
        } else {
            None
        };
        plans.push(FilePlan {
            subject_id: text(page, "id").into(),
            kind: "canonical".into(),
            source: source.into(),
            destination: format!(
                "spaces/{destination_slug}/docs/revisions/{}.md",
                uuid::Uuid::new_v4()
            ),
            sha256: hash,
            maximum: 6_000_000,
        });
    }
    for (table_name, folder, kind) in [
        ("attachments", "attachments", "attachment"),
        ("excalidraw_files", "excalidraw", "drawing"),
    ] {
        for asset in manifest["metadata"][table_name]
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = text(asset, "document_id");
            if id.is_empty() || id.contains('/') || id.contains('\\') || matches!(id, "." | "..") {
                return Err(review_conflict());
            }
            let source_slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
                .bind(text(asset, "space_id"))
                .fetch_one(&mut *conn)
                .await
                .map_err(AppError::internal)?;
            let prefix = if table_name == "attachments" {
                format!("spaces/{source_slug}/{folder}/{id}/")
            } else {
                format!("spaces/{source_slug}/{folder}/")
            };
            if !text(asset, "storage_path").starts_with(&prefix) {
                return Err(review_conflict());
            }
            let leaf = format!(
                "{}.{}",
                uuid::Uuid::new_v4(),
                if kind == "drawing" {
                    "excalidraw"
                } else {
                    "blob"
                }
            );
            let path = if table_name == "attachments" {
                format!("spaces/{destination_slug}/{folder}/{id}/{leaf}")
            } else {
                format!("spaces/{destination_slug}/{folder}/{leaf}")
            };
            plans.push(FilePlan {
                subject_id: text(asset, "id").into(),
                kind: kind.into(),
                source: text(asset, "storage_path").into(),
                destination: path,
                sha256: None,
                maximum: 104_857_600,
            });
        }
    }
    Ok(plans)
}
fn planned_escrow_rows(op: &str, entries: &[EscrowEntry]) -> Vec<Value> {
    let mut rows:Vec<Value>=entries.iter().map(|e|json!({"op_id":op,"document_id":e.document_id,"family":e.family,"source_id":e.source_id,"disposition":e.disposition,"original_json":e.original.to_string(),"expected_json":e.expected.to_string()})).collect();
    rows.sort_by_cached_key(Value::to_string);
    rows
}
async fn write_escrow(conn: &mut AnyConnection, rows: &[Value]) -> Result<(), AppError> {
    for row in rows {
        let affected=sqlx::query("INSERT INTO move_metadata_escrow(op_id,document_id,family,source_id,disposition,original_json,expected_json) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(text(row,"op_id")).bind(text(row,"document_id")).bind(text(row,"family")).bind(text(row,"source_id")).bind(text(row,"disposition")).bind(text(row,"original_json")).bind(text(row,"expected_json")).execute(&mut *conn).await.map_err(AppError::internal)?.rows_affected();
        if affected != 1 {
            return Err(review_conflict());
        }
    }
    Ok(())
}

#[derive(Clone)]
struct RowChange {
    table: String,
    old: Option<Value>,
    new: Option<Value>,
}
fn metadata_changes(
    plans: &[MetadataPlan],
    escrow: &[EscrowEntry],
) -> Result<Vec<RowChange>, AppError> {
    let mut result = Vec::new();
    for plan in plans {
        if matches!(plan.choice, MetadataChoice::Create { .. }) {
            result.push(RowChange {
                table: plan.table.clone(),
                old: None,
                new: plan.target.clone(),
            });
        }
        let (family, name) = match plan.table.as_str() {
            "tags" => ("tag", "doc_tags"),
            "properties_schema" => ("property", "property_values"),
            "workflow_templates" => ("workflow", "document_workflows"),
            _ => return Err(review_conflict()),
        };
        for event in &plan.events {
            let e = escrow
                .iter()
                .find(|e| {
                    e.family == family
                        && e.source_id == text(&plan.source, "id")
                        && e.document_id == text(event, "document_id")
                })
                .ok_or_else(review_conflict)?;
            let new = e.expected["events"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| text(r, "id") == text(event, "id"))
                .cloned();
            result.push(RowChange {
                table: "document_workflow_events".into(),
                old: Some(event.clone()),
                new,
            });
        }
        for old in &plan.assignments {
            let e = escrow
                .iter()
                .find(|e| {
                    e.family == family
                        && e.source_id == text(&plan.source, "id")
                        && e.document_id == text(old, "document_id")
                })
                .ok_or_else(review_conflict)?;
            let new = e.expected["assignments"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| {
                    if family == "tag" {
                        text(r, "document_id") == text(old, "document_id")
                    } else {
                        text(r, "id") == text(old, "id")
                    }
                })
                .cloned();
            result.push(RowChange {
                table: name.into(),
                old: Some(old.clone()),
                new,
            });
        }
    }
    Ok(result)
}
fn moved_changes(
    manifest: &Value,
    request: &MovePreviewRequest,
    files: &crate::storage::relocation::PreparedRelocation,
    public: &PublicLinkDisposition,
    identifiers: &BTreeMap<String, (Value, Value)>,
    maintenance: &BTreeMap<String, (Value, Value)>,
) -> Result<Vec<RowChange>, AppError> {
    let mut changes = Vec::new();
    let folder_ids: BTreeSet<&str> = manifest["folders"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| text(r, "id"))
        .collect();
    for old in manifest["folders"].as_array().into_iter().flatten() {
        let mut new = old.clone();
        new["space_id"] = json!(request.destination.space_id);
        if text(old, "id") == request.root.id && request.root.kind == "folder" {
            new["parent_id"] = json!(request.destination.parent_id)
        }
        changes.push(RowChange {
            table: "folders".into(),
            old: Some(old.clone()),
            new: Some(new),
        });
    }
    for old in manifest["pages"].as_array().into_iter().flatten() {
        let mut new = old.clone();
        new["space_id"] = json!(request.destination.space_id);
        let f = files
            .files
            .iter()
            .find(|f| {
                f.kind == "canonical"
                    && f.subject_id == text(old, "id")
                    && f.source == text(old, "content_path")
            })
            .ok_or_else(review_conflict)?;
        new["content_path"] = json!(f.destination);
        let identifier = &identifiers[text(old, "id")];
        new["human_identifier"] = identifier.0.clone();
        new["human_identifier_key"] = identifier.1.clone();
        let responsibility = &maintenance[text(old, "id")];
        if old.get("maintenance_user_id").is_some() {
            new["maintenance_user_id"] = responsibility.0.clone();
            new["maintenance_group_id"] = responsibility.1.clone();
        }
        if request.root.kind == "document" {
            new["folder_id"] = json!(request.destination.folder_id);
            if text(old, "id") == request.root.id {
                new["parent_id"] = json!(request.destination.parent_id)
            }
        } else if nonnull(old, "folder_id") && !folder_ids.contains(text(old, "folder_id")) {
            return Err(review_conflict());
        }
        if matches!(public, PublicLinkDisposition::Disable) {
            new["public"] = json!(0);
            new["public_slug"] = Value::Null;
        }
        changes.push(RowChange {
            table: "documents".into(),
            old: Some(old.clone()),
            new: Some(new),
        });
    }
    for name in ["document_drafts", "attachments", "excalidraw_files"] {
        for old in manifest["metadata"][name].as_array().into_iter().flatten() {
            let mut new = old.clone();
            new["space_id"] = json!(request.destination.space_id);
            if name != "document_drafts" {
                let f = files
                    .files
                    .iter()
                    .find(|f| {
                        f.subject_id == text(old, "id") && f.source == text(old, "storage_path")
                    })
                    .ok_or_else(review_conflict)?;
                new["storage_path"] = json!(f.destination);
            }
            changes.push(RowChange {
                table: name.into(),
                old: Some(old.clone()),
                new: Some(new),
            });
        }
    }
    // Composite restriction FK is immediate: exact allowlist rows are retained in
    // immutable escrow, removed/reinserted with their same IDs/times in one TX.
    for old in manifest["metadata"]["document_access_allowlist"]
        .as_array()
        .into_iter()
        .flatten()
    {
        changes.push(RowChange {
            table: "document_access_allowlist".into(),
            old: Some(old.clone()),
            new: None,
        });
    }
    for old in manifest["metadata"]["document_access_policies"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let mut new = old.clone();
        new["space_id"] = json!(request.destination.space_id);
        changes.push(RowChange {
            table: "document_access_policies".into(),
            old: Some(old.clone()),
            new: Some(new),
        });
    }
    for old in manifest["metadata"]["document_access_allowlist"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let mut new = old.clone();
        new["space_id"] = json!(request.destination.space_id);
        changes.push(RowChange {
            table: "document_access_allowlist".into(),
            old: None,
            new: Some(new),
        });
    }
    Ok(changes)
}
fn apply_expected(
    state: &mut BTreeMap<String, Vec<Value>>,
    changes: &[RowChange],
) -> Result<(), AppError> {
    for c in changes {
        if let Some(old) = &c.old {
            rewrite(state, &c.table, old, c.new.clone())?
        } else {
            insert_expected(state, &c.table, c.new.clone().ok_or_else(review_conflict)?)?
        }
    }
    Ok(())
}
async fn apply_changes(conn: &mut AnyConnection, changes: &[RowChange]) -> Result<(), AppError> {
    for c in changes {
        if let Some(old) = &c.old {
            change_row(conn, &c.table, old, c.new.as_ref()).await?
        } else {
            insert_row(conn, &c.table, c.new.as_ref().ok_or_else(review_conflict)?).await?
        }
    }
    Ok(())
}
async fn validate_current_values(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    destination: &str,
    expected: &BTreeMap<String, Vec<Value>>,
    pages: &BTreeSet<String>,
) -> Result<(), AppError> {
    let assignments: Vec<(&str, Option<&str>)> = table(expected, "property_values")
        .iter()
        .filter(|r| pages.contains(text(r, "document_id")))
        .map(|r| (text(r, "property_id"), r["value"].as_str()))
        .collect();
    let targets = super::super::property_validation::validate_assignment_batch(
        conn,
        destination,
        access::principal_from_auth(auth)?,
        &assignments,
    )
    .await?;
    super::super::property_validation::revalidate_relation_targets(
        conn,
        destination,
        access::principal_from_auth(auth)?,
        &targets,
    )
    .await
}
#[cfg(test)]
tokio::task_local! { static AFTER_VIEWER_GRANT_FINAL: (std::sync::Arc<tokio::sync::Notify>, std::sync::Arc<tokio::sync::Notify>); }
#[cfg(test)]
async fn after_viewer_grant_final() {
    if let Ok((reached, release)) = AFTER_VIEWER_GRANT_FINAL.try_with(|g| g.clone()) {
        reached.notify_one(); release.notified().await;
    }
}
#[cfg(test)]
fn move_process_fault(phase: &str) {
    if std::env::var("STARLANE_MOVE_COMMIT_CHILD_PHASE")
        .ok()
        .as_deref()
        == Some(phase)
    {
        std::process::exit(77)
    }
}
#[cfg(test)]
thread_local! {static MOVE_COMMIT_OUTCOME:std::cell::RefCell<Option<&'static str>>=std::cell::RefCell::new(None);static MOVE_BEFORE_GUARD:std::cell::RefCell<Option<std::sync::Arc<tokio::sync::Notify>>>=std::cell::RefCell::new(None);}
async fn commit_move_transaction(tx: sqlx::Transaction<'_, sqlx::Any>) -> Result<(), sqlx::Error> {
    #[cfg(test)]
    match MOVE_COMMIT_OUTCOME.with(|slot| slot.borrow_mut().take()) {
        Some("lost_ack") => {
            tx.commit().await?;
            return Err(sqlx::Error::Protocol(
                "fixture lost acknowledgement after real commit".into(),
            ));
        }
        Some("rollback") => {
            tx.rollback().await?;
            return Err(sqlx::Error::Protocol("fixture actual rollback".into()));
        }
        _ => {}
    }
    tx.commit().await
}
async fn relocate(
    state: &AppState,
    auth: &AuthUser,
    id: &str,
    req: &CommitRequest,
) -> Result<Value, AppError> {
    relocate_in(&crate::storage::data_dir(), state, auth, id, req).await
}
async fn relocate_in(
    root: &std::path::Path,
    state: &AppState,
    auth: &AuthUser,
    id: &str,
    req: &CommitRequest,
) -> Result<Value, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)]
    if let Some(notify) = MOVE_BEFORE_GUARD.with(|slot| slot.borrow_mut().take()) {
        notify.notify_one();
    }
    super::super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    require_preview_identity(&mut tx, auth, MovePreviewCredential::Write).await?;
    let record = owned_preview(&mut tx, id, &auth.claims.sub).await?;
    let source = text(&record, "source_space_id").to_string();
    let request: MovePreviewRequest =
        serde_json::from_str(text(&record, "request_json")).map_err(|_| unavailable())?;
    if request.destination.space_id == source || req.fingerprint != text(&record, "fingerprint") {
        return Err(review_conflict());
    }
    let preliminary = review_move_current(
        &mut tx,
        auth,
        &source,
        &request,
        MovePreviewCredential::Write,
    )
    .await?;
    let affected = incident_spaces(
        &mut tx,
        &preliminary.manifest,
        &source,
        &request.destination.space_id,
    )
    .await?;
    let space_refs: Vec<&str> = affected.iter().map(String::as_str).collect();
    super::super::mutation_guards::lock_spaces_for_mutation(&mut tx, &space_refs)
        .await
        .map_err(conceal)?;
    if sqlx::query("UPDATE move_previews SET id=id WHERE id=$1 AND actor_id=$2 AND cancelled_at IS NULL AND expires_at>$3").bind(id).bind(&auth.claims.sub).bind(chrono::Utc::now().to_rfc3339()).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(unavailable())}
    let before = review_move_current(
        &mut tx,
        auth,
        &source,
        &request,
        MovePreviewCredential::Write,
    )
    .await?;
    validate_decisions(&before, req)?;
    require_folder_move_spaces(
        &mut tx,
        auth,
        &source,
        &request.destination.space_id,
        &before.manifest,
    )
    .await?;
    // External relation changes cannot be attributed to subtree Move authority.
    // The explicit incident-edge adapter remains required; no hidden endpoint IDs leak.
    if before
        .review
        .blockers
        .iter()
        .any(|b| b != "external_relations_require_resolution")
    {
        return Err(review_conflict());
    }

    for name in ["folders", "documents", "document_drafts"] {
        let rows = if name == "folders" {
            before.manifest["folders"].as_array()
        } else if name == "documents" {
            before.manifest["pages"].as_array()
        } else {
            before.manifest["metadata"][name].as_array()
        };
        let mut bindings: BTreeMap<String, String> = rows
            .into_iter()
            .flatten()
            .map(|r| (text(r, "id").into(), source.clone()))
            .collect();
        if name == "documents" {
            for relation in crossing_relations(&before.manifest) {
                let doc = text(relation, "document_id");
                if !before.manifest["pages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|p| text(p, "id") == doc)
                {
                    let space: String =
                        sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1")
                            .bind(doc)
                            .fetch_optional(&mut *tx)
                            .await
                            .map_err(AppError::internal)?
                            .ok_or_else(relation_conflict)?;
                    if !affected.contains(&space) {
                        return Err(relation_conflict());
                    }
                    bindings.insert(doc.into(), space);
                }
            }
        }
        for (row_id, row_space) in bindings {
            if sqlx::query(&format!(
                "UPDATE {name} SET id=id WHERE id=$1 AND space_id=$2"
            ))
            .bind(row_id)
            .bind(row_space)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected()
                != 1
            {
                return Err(relation_conflict());
            }
        }
    }
    verify_review(
        &mut tx,
        auth,
        &source,
        &request,
        &before,
        MovePreviewCredential::Write,
    )
    .await?;
    let original = domain_state(&mut tx).await?;
    let mut expected_aliases =
        rows_raw(&mut tx, "SELECT * FROM move_asset_path_aliases", &[]).await?;
    let pages: BTreeSet<String> = before.manifest["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| text(p, "id").to_string())
        .collect();
    if matches!(req.public_links, PublicLinkDisposition::Disable) {
        for page in before.manifest["pages"].as_array().into_iter().flatten() {
            access::resolve_page_access(
                &mut tx,
                access::principal_from_auth(auth)?,
                &source,
                text(page, "id"),
                access::PageOperation::PublicLinkDisable,
                access::Purpose::Routine,
            )
            .await
            .map_err(conceal)?;
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    let metadata = plan_metadata(
        &mut tx,
        auth,
        &request.destination.space_id,
        &original,
        &pages,
        req,
        &now,
    )
    .await?;
    let mut escrow = planned_escrow(&metadata, &before.manifest, &request.destination.space_id)?;
    let incident = plan_incident_relations(
        &mut tx,
        auth,
        &before.manifest,
        &original,
        &metadata,
        req,
        &request.destination.space_id,
        &mut escrow,
    )
    .await?;
    let identifiers = plan_identifiers(&before, req)?;
    require_identifier_edits(&mut tx, auth, &source, &before, &identifiers).await?;
    for page in before.manifest["pages"].as_array().into_iter().flatten() {
        let pair = &identifiers[text(page, "id")];
        if page
            .get("human_identifier")
            .unwrap_or(&Value::Null)
            .is_null()
            && pair.0.is_null()
        {
            continue;
        }
        escrow.push(EscrowEntry { document_id:text(page,"id").into(),family:"identifier".into(),source_id:text(page,"id").into(),disposition: match req.identifier_decisions.iter().find(|d|d.document_id==text(page,"id")).map(|d|&d.choice){Some(IdentifierChoice::Rename{..})=>"map",Some(IdentifierChoice::Clear)=>"remove",_=>"preserve"}.into(),original:json!({"human_identifier":page.get("human_identifier").unwrap_or(&Value::Null),"human_identifier_key":page.get("human_identifier_key").unwrap_or(&Value::Null)}),expected:json!({"human_identifier":pair.0,"human_identifier_key":pair.1}) });
    }
    let maintenance = plan_maintenance(&before, req)?;
    require_maintenance_edits(&mut tx, auth, &source, &before, &maintenance).await?;
    let maintenance_witnesses = prove_maintenance(
        &mut tx,
        auth,
        &request.destination.space_id,
        req,
        &before,
        &maintenance,
    )
    .await?;
    for page in before.manifest["pages"].as_array().into_iter().flatten() {
        let original = maintenance_pair(page)?;
        let pair = &maintenance[text(page, "id")];
        if original.0.is_none() && original.1.is_none() && pair.0.is_null() && pair.1.is_null() {
            continue;
        }
        escrow.push(EscrowEntry {
            document_id: text(page, "id").into(),
            family: "maintenance".into(),
            source_id: text(page, "id").into(),
            disposition: if pair.0.is_null() && pair.1.is_null() {
                "remove"
            } else if pair == &(json!(original.0.clone()), json!(original.1.clone())) {
                "preserve"
            } else {
                "map"
            }
            .into(),
            original: json!({"maintenance_user_id":original.0,"maintenance_group_id":original.1}),
            expected: json!({"maintenance_user_id":pair.0,"maintenance_group_id":pair.1}),
        });
    }
    let viewer_plans = plan_viewer_grants(&mut tx, auth, &request, &before, &original, req, &now, &mut escrow).await?;
    let viewer_dependencies = if viewer_plans.is_empty() { Vec::new() } else {
        viewer_move_dependencies(&mut tx, &source, &request.destination.space_id).await?
    };
    let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(&request.destination.space_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let file_plan = file_plans(&mut tx, &before.manifest, &slug).await?;
    let op = uuid::Uuid::new_v4().to_string();
    let request_fingerprint =
        digest(&json!({"preview_id":id,"actor":auth.claims.sub,"request":req}));
    let files = crate::storage::relocation::prepare(
        root,
        &op,
        &before.review.fingerprint,
        &request_fingerprint,
        &file_plan,
    )?;
    // Files are installed without overwriting existing paths, still unassociated.
    // No source detach or metadata cascade has happened before exact final source proof.
    verify_review(
        &mut tx,
        auth,
        &source,
        &request,
        &before,
        MovePreviewCredential::Write,
    )
    .await?;
    if domain_state(&mut tx).await? != original {
        return Err(review_conflict());
    }
    let mut changes: Vec<RowChange> = viewer_plans.iter().map(|p| RowChange {
        table: "document_viewer_grants".into(), old: Some(p.original.clone()), new: None,
    }).collect();
    changes.extend(metadata_changes(&metadata, &escrow)?);
    changes.extend(incident_changes(&incident));
    changes.extend(moved_changes(
        &before.manifest,
        &request,
        &files,
        &req.public_links,
        &identifiers,
        &maintenance,
    )?);
    changes.extend(viewer_plans.iter().filter_map(|p| p.expected.clone().map(|new| RowChange {
        table: "document_viewer_grants".into(), old: None, new: Some(new),
    })));
    let mut expected = original.clone();
    apply_expected(&mut expected, &changes)?;
    // Removing the last grant for an account also removes its derived domain
    // dependency row; no account or independent grant is itself mutated.
    if !viewer_plans.is_empty() {
        let recipients: BTreeSet<String> = table(&expected, "document_viewer_grants").iter().map(|g| text(g, "user_id").into()).collect();
        expected.get_mut("viewer_grant_recipients").ok_or_else(review_conflict)?.retain(|u| recipients.contains(text(u, "id")));
    }
    let escrow_rows = planned_escrow_rows(&op, &escrow);
    let escrow_binding = crate::storage::relocation::escrow_binding(&escrow_rows)?;
    let move_audit = insert_move_audit(&mut tx, auth, &request, &viewer_plans).await?;
    let expected_digest = digest(&serde_json::to_value(&expected).map_err(AppError::internal)?);
    let manifest_json = serde_json::to_string(&files).map_err(AppError::internal)?;
    let mut expected_receipt_state = json!({"escrow_count":escrow_binding["escrow_count"],"escrow_fingerprint":escrow_binding["escrow_fingerprint"],"domain_fingerprint":expected_digest,"audit_event":move_audit,"public_link_disposition":req.public_links,"root_id":request.root.id,"root_kind":request.root.kind,"source_space_id":source,"destination_space_id":request.destination.space_id,"pages":expected["documents"].iter().filter(|r|pages.contains(text(r,"id"))).map(|r|json!({"id":r["id"],"space_id":r["space_id"],"content_path":r["content_path"],"updated_at":r["updated_at"],"published_at":r["published_at"]})).collect::<Vec<_>>()});
    // Historical operation identities/paths/results remain original evidence.
    // Persist their captured raw values under the move seal; never relocate them.
    expected_receipt_state["retained_immutable_receipts"] = serde_json::to_value(
        RETAINED_RECEIPT_TABLES.iter().map(|name| ((*name).to_string(), table(&original, name).to_vec())).collect::<BTreeMap<_,_>>()
    ).map_err(AppError::internal)?;
    if !req.maintenance_decisions.is_empty() {
        expected_receipt_state["maintenance_decisions"] = json!(req.maintenance_decisions);
    }
    if !req.identifier_decisions.is_empty() {
        expected_receipt_state["identifier_decisions"] = json!(req.identifier_decisions);
    }
    if !req.viewer_grant_decisions.is_empty() {
        expected_receipt_state["viewer_grant_decisions"] = json!(req.viewer_grant_decisions);
        let mut grants_before: Vec<Value> = viewer_plans.iter().map(|p| p.original.clone()).collect();
        grants_before.sort_by_cached_key(Value::to_string);
        expected_receipt_state["viewer_grants_before"] = json!(grants_before);
        expected_receipt_state["viewer_grants_after"] = json!(viewer_plans.iter().filter_map(|p| p.expected.as_ref()).collect::<Vec<_>>());
    }
    let expected_json = expected_receipt_state.to_string();
    let receipt = json!({"op_id":op,"preview_id":id,"actor_id":auth.claims.sub,"source_space_id":source,"destination_space_id":request.destination.space_id,"request_fingerprint":request_fingerprint,"source_fingerprint":before.review.fingerprint,"manifest_json":manifest_json,"expected_state_json":expected_json,"created_at":now});
    crate::storage::relocation::seal_receipt(root, &receipt)?;
    sqlx::query("INSERT INTO move_commit_receipts(op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&op).bind(id).bind(&auth.claims.sub).bind(&source).bind(&request.destination.space_id).bind(&request_fingerprint).bind(&before.review.fingerprint).bind(&manifest_json).bind(&expected_json).bind(&now).execute(&mut *tx).await.map_err(AppError::internal)?;
    // All originals, including complete workflow history and private SQL drafts,
    // are sealed in THIS transaction BEFORE ANY relation/workflow/policy detach.
    write_escrow(&mut tx, &escrow_rows).await?;
    apply_changes(&mut tx, &changes).await?;
    validate_current_values(
        &mut tx,
        auth,
        &request.destination.space_id,
        &expected,
        &pages,
    )
    .await?;
    validate_incident_outcomes(&mut tx, auth, &incident).await?;
    for (name, kind) in [
        ("attachments", "attachment"),
        ("excalidraw_files", "drawing"),
    ] {
        for asset in before.manifest["metadata"][name]
            .as_array()
            .into_iter()
            .flatten()
        {
            let old = text(asset, "storage_path");
            let existing: Option<(String, String)> = sqlx::query_as(
                "SELECT asset_id,asset_kind FROM move_asset_path_aliases WHERE old_path=$1",
            )
            .bind(old)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
            if let Some((aid, k)) = existing {
                if aid != text(asset, "id") || k != kind {
                    return Err(review_conflict());
                }
            } else {
                sqlx::query("INSERT INTO move_asset_path_aliases(old_path,asset_id,asset_kind,op_id,created_at) VALUES($1,$2,$3,$4,$5)").bind(old).bind(text(asset,"id")).bind(kind).bind(&op).bind(&now).execute(&mut *tx).await.map_err(AppError::internal)?;
                expected_aliases.push(json!({"old_path":old,"asset_id":asset["id"],"asset_kind":kind,"op_id":op,"created_at":now}));
            }
        }
    }
    access::require_move_preview_space(&mut tx, auth, &source, MovePreviewCredential::Write)
        .await
        .map_err(conceal)?;
    access::require_move_preview_space(
        &mut tx,
        auth,
        &request.destination.space_id,
        MovePreviewCredential::Write,
    )
    .await
    .map_err(conceal)?;
    for page in &pages {
        access::resolve_page_access(
            &mut tx,
            access::principal_from_auth(auth)?,
            &request.destination.space_id,
            page,
            access::PageOperation::Move,
            access::Purpose::Routine,
        )
        .await
        .map_err(conceal)?;
    }
    if matches!(req.public_links, PublicLinkDisposition::Disable) {
        // Source participation/restrictions are preserved by exact captured domain proof;
        // the moved row is never queried under its former scope.
        require_create_metadata(&mut tx, auth, &source, false).await?;
        for page in &pages {
            access::resolve_page_access(
                &mut tx,
                access::principal_from_auth(auth)?,
                &request.destination.space_id,
                page,
                access::PageOperation::PublicLinkDisable,
                access::Purpose::Routine,
            )
            .await
            .map_err(conceal)?;
        }
    }

    require_folder_move_spaces(
        &mut tx,
        auth,
        &source,
        &request.destination.space_id,
        &before.manifest,
    )
    .await?;
    for plan in &metadata {
        if matches!(plan.choice, MetadataChoice::Create { .. }) {
            require_create_metadata(
                &mut tx,
                auth,
                &request.destination.space_id,
                plan.table == "workflow_templates",
            )
            .await?
        }
    }
    if matches!(req.public_links, PublicLinkDisposition::Preserve)
        && before.public_pages_enabled != public_pages_enabled()
    {
        return Err(review_conflict());
    }
    expected_aliases.sort_by_cached_key(Value::to_string);
    if rows_raw(&mut tx, "SELECT * FROM move_asset_path_aliases", &[]).await? != expected_aliases {
        return Err(review_conflict());
    }
    if domain_state(&mut tx).await? != expected
        || rows_raw(
            &mut tx,
            "SELECT * FROM move_metadata_escrow WHERE op_id=$1",
            &[&op],
        )
        .await?
            != escrow_rows
    {
        return Err(review_conflict());
    }
    for page in before.manifest["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| nonnull(p, "published_at"))
    {
        let hash = published_source_digest(&mut tx, page).await?;
        if files
            .files
            .iter()
            .find(|f| f.kind == "canonical" && f.subject_id == text(page, "id"))
            .map(|f| f.sha256.as_str())
            != Some(hash.as_str())
        {
            return Err(review_conflict());
        }
    }
    validate_incident_outcomes(&mut tx, auth, &incident).await?;
    if immutable_receipt(&mut tx, &op).await? != receipt {
        return Err(review_conflict());
    }
    verify_move_audit(&mut tx, &move_audit).await?;
    crate::storage::relocation::verify_escrow(&mut tx, &receipt).await?;
    for page in before.manifest["pages"].as_array().into_iter().flatten() {
        let pair = &identifiers[text(page, "id")];
        if pair.0 != page.get("human_identifier").cloned().unwrap_or(Value::Null)
            || pair.1
                != page
                    .get("human_identifier_key")
                    .cloned()
                    .unwrap_or(Value::Null)
        {
            // Original private/restriction/lock rows are in the exact domain/source proof.
            // Reprove the original account EditContent and the actual relocated target.
            require_create_metadata(&mut tx, auth, &source, false).await?;
            access::resolve_page_access(
                &mut tx,
                access::principal_from_auth(auth)?,
                &request.destination.space_id,
                text(page, "id"),
                access::PageOperation::Edit,
                access::Purpose::Routine,
            )
            .await
            .map_err(conceal)?;
        }
    }
    if prove_maintenance(
        &mut tx,
        auth,
        &request.destination.space_id,
        req,
        &before,
        &maintenance,
    )
    .await?
        != maintenance_witnesses
    {
        return Err(review_conflict());
    }
    for page in before.manifest["pages"].as_array().into_iter().flatten() {
        let old = maintenance_pair(page)?;
        if maintenance[text(page, "id")] != (json!(old.0), json!(old.1)) {
            require_create_metadata(&mut tx, auth, &source, false).await?;
            access::resolve_page_access(
                &mut tx,
                access::principal_from_auth(auth)?,
                &request.destination.space_id,
                text(page, "id"),
                access::PageOperation::Edit,
                access::Purpose::Routine,
            )
            .await
            .map_err(conceal)?;
        }
    }
    verify_final_viewer_grants(&mut tx, auth, &source, &request.destination.space_id, &viewer_plans, &viewer_dependencies).await?;
    #[cfg(test)]
    if !viewer_plans.is_empty() { after_viewer_grant_final().await; }
    // Retained full receipt/domain rows are checked after late capability SQL.
    // A trigger cannot alter historical evidence while preserving moved rows.
    if domain_state(&mut tx).await? != expected { return Err(review_conflict()); }
    require_preview_identity(&mut tx, auth, MovePreviewCredential::Write).await?;
    crate::storage::relocation::verify_sealed_receipt(root, &receipt)?;
    crate::storage::relocation::verify(root, &files, true)?;
    crate::storage::relocation::verify_sealed_receipt(root, &receipt)?;
    #[cfg(test)]
    move_process_fault("before_commit");
    match commit_move_transaction(tx).await {
        Ok(()) => {
            #[cfg(test)]
            move_process_fault("after_commit");
            crate::audit::append_committed_move_action(&move_audit).await;
            reindex_moved_published(state, root, &receipt).await;
            Ok(
                json!({"operation_id":op,"root_id":request.root.id,"destination_space_id":request.destination.space_id,"moved_at":now,"committed":true,"preserved_metadata_url":format!("/api/v1/moves/commits/{op}/preserved/{{document_id}}")}),
            )
        }
        Err(error) => {
            if confirm_receipt(root, &state.db, &receipt).await? {
                crate::audit::append_committed_move_action(&move_audit).await;
                reindex_moved_published(state, root, &receipt).await;
                Ok(
                    json!({"operation_id":op,"root_id":request.root.id,"committed":true,"commit_acknowledgement_recovered":true}),
                )
            } else {
                Err(AppError::Internal(format!(
                    "Move transaction did not commit; staged evidence retained: {error}"
                )))
            }
        }
    }
}
async fn insert_move_audit(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    request: &MovePreviewRequest,
    viewer_plans: &[ViewerGrantPlan],
) -> Result<Value, AppError> {
    require_preview_identity(conn, auth, MovePreviewCredential::Write).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let action = if request.root.kind == "folder" {
        "folder.move"
    } else {
        "document.move"
    };
    let created:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,ip_address,action,target_type,target_id,actor_kind,actor_id) VALUES($1,$2,NULL,$3,$4,$5,'user',$2) RETURNING created_at").bind(&id).bind(&auth.claims.sub).bind(action).bind(&request.root.kind).bind(&request.root.id).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    let mut grant_before: Vec<Value> = viewer_plans.iter().map(|p| p.original.clone()).collect();
    grant_before.sort_by_cached_key(Value::to_string);
    let before_value = if viewer_plans.is_empty() { Value::Null } else { json!(json!({"page_viewer_grants":grant_before}).to_string()) };
    let after_value = if viewer_plans.is_empty() { Value::Null } else { json!(json!({"page_viewer_grants":viewer_plans.iter().map(|p| &p.expected).collect::<Vec<_>>()} ).to_string()) };
    if !viewer_plans.is_empty() {
        sqlx::query("UPDATE audit_log SET before_value=$1,after_value=$2 WHERE id=$3").bind(before_value.as_str()).bind(after_value.as_str()).bind(&id).execute(&mut *conn).await.map_err(AppError::internal)?;
    }
    Ok(json!({"id":id,"user_id":auth.claims.sub,"ip_address":null,"action":action,"target_type":request.root.kind,"target_id":request.root.id,"created_at":created,"actor_kind":"user","actor_id":auth.claims.sub,"before_value":before_value,"after_value":after_value}))
}
async fn verify_move_audit(conn: &mut AnyConnection, event: &Value) -> Result<(), AppError> {
    let rows = rows_raw(
        conn,
        "SELECT * FROM audit_log WHERE id=$1",
        &[text(event, "id")],
    )
    .await?;
    if rows != vec![event.clone()] {
        return Err(review_conflict());
    }
    Ok(())
}
#[cfg(test)]
thread_local! {static BEFORE_ADVISORY_REINDEX:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);}
/// Background indexing only after known commit; it grants no access and cannot fail the ACK.
async fn reindex_moved_published(state: &AppState, root: &std::path::Path, receipt: &Value) {
    #[cfg(test)]
    if let Some(hook) = BEFORE_ADVISORY_REINDEX.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    let result=async{
  let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
  if immutable_receipt(&mut conn,text(receipt,"op_id")).await?!=*receipt{return Err(review_conflict())}
  let m=crate::storage::relocation::verify_sealed_receipt(root,receipt)?;
  let expected:Value=serde_json::from_str(text(receipt,"expected_state_json")).map_err(AppError::internal)?;
  for page in expected["pages"].as_array().into_iter().flatten().filter(|p|nonnull(p,"published_at")){
   let rows=rows_raw(&mut conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL",&[text(page,"id"),text(page,"space_id")]).await?;
   let Some(current)=rows.first() else {continue};if ["id","space_id","content_path","updated_at","published_at"].iter().any(|field|page[*field]!=current[*field]){continue}
   let bytes=match crate::storage::relocation::installed_canonical_bytes(root,&m,text(page,"id")){Ok(bytes)=>bytes,Err(error)=>{tracing::warn!(document_id=%text(page,"id"),?error,"Moved published search source unavailable; index retained");continue}};
   let body=match String::from_utf8(bytes.clone()){Ok(body)=>body,Err(_)=>continue};
   let tags:Vec<String>=sqlx::query_scalar("SELECT t.name FROM tags t JOIN doc_tags dt ON dt.tag_id=t.id WHERE dt.document_id=$1 AND t.space_id=$2 ORDER BY t.id").bind(text(page,"id")).bind(text(page,"space_id")).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
   let final_rows=rows_raw(&mut conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL",&[text(page,"id"),text(page,"space_id")]).await?;
   let final_tags:Vec<String>=sqlx::query_scalar("SELECT t.name FROM tags t JOIN doc_tags dt ON dt.tag_id=t.id WHERE dt.document_id=$1 AND t.space_id=$2 ORDER BY t.id").bind(text(page,"id")).bind(text(page,"space_id")).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
   if final_rows!=rows||final_tags!=tags{continue}
   if immutable_receipt(&mut conn,text(receipt,"op_id")).await?!=*receipt{return Err(review_conflict())}
   crate::storage::relocation::verify_escrow(&mut conn,receipt).await?;
   crate::storage::relocation::verify_sealed_receipt(root,receipt)?;
   if crate::storage::relocation::installed_canonical_bytes(root,&m,text(page,"id"))?!=bytes{return Err(review_conflict())}
   if let Err(error)=state.search_index.upsert(crate::search::index::DocEntry{doc_id:text(page,"id").into(),space_id:text(page,"space_id").into(),title:text(current,"title").into(),body,tags:tags.join(" ")}){tracing::warn!(document_id=%text(page,"id"),?error,"Moved published search index update failed; committed move retained");}
  }Ok::<(),AppError>(())
 }.await;
    if let Err(error) = result {
        tracing::warn!(operation_id=%text(receipt,"op_id"),?error,"Moved search indexing unavailable; committed move retained");
    }
}

async fn immutable_receipt(conn: &mut AnyConnection, op: &str) -> Result<Value, AppError> {
    let mut rows=rows_raw(conn,"SELECT op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at FROM move_commit_receipts WHERE op_id=$1",&[op]).await?;
    if rows.len() != 1 {
        return Err(review_conflict());
    }
    Ok(rows.remove(0))
}
async fn confirm_receipt(
    root: &std::path::Path,
    pool: &sqlx::AnyPool,
    expected: &Value,
) -> Result<bool, AppError> {
    let mut tx = pool.begin().await.map_err(AppError::internal)?;
    super::super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let source = text(expected, "source_space_id");
    let destination = text(expected, "destination_space_id");
    let spaces = rows_raw(
        &mut tx,
        "SELECT id FROM spaces WHERE id=$1 OR id=$2",
        &[source, destination],
    )
    .await?;
    let ids: Vec<&str> = spaces.iter().map(|r| text(r, "id")).collect();
    super::super::mutation_guards::lock_spaces_for_mutation(&mut tx, &ids).await?;
    let op = text(expected, "op_id");
    let changed =
        sqlx::query("UPDATE move_commit_receipts SET verified_at=verified_at WHERE op_id=$1")
            .bind(op)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected();
    let answer = if changed == 0 {
        false
    } else {
        if immutable_receipt(&mut tx, op).await? != *expected {
            return Err(review_conflict());
        }
        crate::storage::relocation::verify_escrow(&mut tx, expected).await?;
        crate::storage::relocation::verify_sealed_receipt(root, expected)?;
        true
    };
    tx.commit().await.map_err(AppError::internal)?;
    Ok(answer)
}
#[cfg(test)]
mod tests {
    use super::*;
    async fn setup() -> (AppState, String, String, CommitRequest, String) {
        setup_with_wal(false).await
    }
    async fn setup_with_wal(wal: bool) -> (AppState, String, String, CommitRequest, String) {
        let (state, source, target) = super::super::tests::fixture_with_wal(wal).await;
        sqlx::query("UPDATE space_members SET role='owner' WHERE user_id='mover'")
            .execute(&state.db)
            .await
            .unwrap();
        // The preview fixture inventories a historical unservable root-level asset.
        // Commit positives use the current document-owned upload path contract.
        let old_asset = crate::storage::space_dir(&source).join("attachments/file.bin");
        let active_asset =
            crate::storage::space_dir(&source).join("attachments/move-page/file.bin");
        std::fs::create_dir_all(active_asset.parent().unwrap()).unwrap();
        std::fs::copy(&old_asset, &active_asset).unwrap();
        sqlx::query("UPDATE attachments SET storage_path=$1 WHERE id='attachment'")
            .bind(
                active_asset
                    .strip_prefix(crate::storage::data_dir())
                    .unwrap()
                    .to_string_lossy()
                    .as_ref(),
            )
            .execute(&state.db)
            .await
            .unwrap();

        for id in ["move-page", "move-child"] {
            let mut tx = state.db.begin().await.unwrap();
            let row: (String, String, String) = sqlx::query_as(
                "SELECT content_path,updated_at,published_at FROM documents WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            let bytes = std::fs::read(crate::storage::data_dir().join(&row.0)).unwrap();
            let revision = format!("immutable-{id}");
            let auth = super::super::tests::test_auth("mover");
            let intent = crate::storage::canonical_rewrite::RewriteIntent {
                space_id: "move-source".into(),
                document_id: id.into(),
                source_path: row.0,
                source_revision: row.1,
                published_stamp: row.2,
                revision: revision.clone(),
                source_sha256: None,
                reason: crate::storage::canonical_rewrite::RewriteReason::ReplayContent,
                incoming: crate::storage::canonical_rewrite::IncomingIdentity {
                    provider: "move-fixture".into(),
                    operation_id: format!("initial-{id}"),
                    entity_type: "document".into(),
                    entity_id: id.into(),
                    action: "update_content".into(),
                    payload_fingerprint: "a".repeat(64),
                },
            };
            let prepared =
                crate::storage::canonical_rewrite::prepare_rewrite(&mut tx, &auth, intent, &bytes)
                    .await
                    .unwrap();
            sqlx::query("UPDATE documents SET content_path=$1,updated_at=$2 WHERE id=$3")
                .bind(prepared.path())
                .bind(&revision)
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
            crate::storage::canonical_rewrite::record_rewrite(&mut tx, &auth, &prepared)
                .await
                .unwrap();
            sqlx::query("UPDATE document_drafts SET base_updated_at=$1 WHERE document_id=$2")
                .bind(&revision)
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        let req:CommitRequest=serde_json::from_value(json!({"fingerprint":preview.fingerprint,"decisions":[{"family":"tag","source_id":"source-tag","choice":{"kind":"map","destination_id":"target-tag"}},{"family":"property","source_id":"source-property","choice":{"kind":"map","destination_id":"target-number"}},{"family":"workflow","source_id":"source-workflow","choice":{"kind":"map","destination_id":"target-workflow"}}],"public_links":"preserve","confirm_audience_change":true,"confirm_draft_space_change":true})).unwrap();
        (state, source, target, req, preview.preview_id)
    }
    async fn cleanup(source: &str, target: &str) {
        let _ = tokio::fs::remove_dir_all(crate::storage::space_dir(source)).await;
        let _ = tokio::fs::remove_dir_all(crate::storage::space_dir(target)).await;
    }
    #[tokio::test]
    async fn enabled_handler_moves_whole_subtree_and_seals_retained_receipts() {
        let (state, source, target, req, id) = setup().await;
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap(); drop(conn);
        let (status, Json(result)) = commit_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path(id),Json(req)).await.unwrap();
        assert_eq!(status,StatusCode::OK); assert_eq!(result["committed"],true);
        let mut conn = state.db.acquire().await.unwrap();
        let after = domain_state(&mut conn).await.unwrap();
        for name in RETAINED_RECEIPT_TABLES {assert_eq!(table(&after,name),table(&before,name));}
        for page in ["move-page","move-child"] {assert_eq!(after["documents"].iter().find(|r|r["id"]==page).unwrap()["space_id"],"move-target");}
        let receipt = immutable_receipt(&mut conn,result["operation_id"].as_str().unwrap()).await.unwrap();
        let expected:Value=serde_json::from_str(text(&receipt,"expected_state_json")).unwrap();
        for name in RETAINED_RECEIPT_TABLES {assert_eq!(expected["retained_immutable_receipts"][*name],json!(table(&before,name)));}
        drop(conn); cleanup(&source,&target).await;
    }

    #[tokio::test]
    async fn full_subtree_core_preserves_ids_bodies_history_and_maps_metadata() {
        let (state, source, target, req, id) = setup().await;
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let after = domain_state(&mut conn).await.unwrap();
        for name in [
            "doc_versions",
            "document_draft_access",
            "comments",
            "reactions",
            "starred_documents",
            "doc_subscriptions",
        ] {
            assert_eq!(after[name], before[name], "{name}")
        }
        for page in &after["documents"] {
            assert_eq!(page["space_id"], json!("move-target"));
            let old = before["documents"]
                .iter()
                .find(|r| r["id"] == page["id"])
                .unwrap();
            for key in [
                "id",
                "title",
                "icon",
                "cover_image",
                "published_at",
                "updated_at",
                "created_at",
                "created_by",
                "last_updated_by",
                "automerge_state",
                "public",
                "public_slug",
            ] {
                assert_eq!(page[key], old[key], "{key}")
            }
            assert_eq!(
                std::fs::read(crate::storage::data_dir().join(text(page, "content_path"))).unwrap(),
                std::fs::read(crate::storage::data_dir().join(text(old, "content_path"))).unwrap()
            );
        }
        let draft = &after["document_drafts"][0];
        let mut expected = before["document_drafts"][0].clone();
        expected["space_id"] = json!("move-target");
        assert_eq!(*draft, expected);
        assert_eq!(after["property_values"][0]["id"], json!("source-value"));
        assert_eq!(
            after["property_values"][0]["property_id"],
            json!("target-number")
        );
        assert_eq!(after["property_values"][0]["value"], json!("42"));
        assert_eq!(after["document_workflows"][0]["id"], json!("workflow"));
        assert_eq!(
            after["document_workflows"][0]["workflow_template_id"],
            json!("target-workflow")
        );
        assert_eq!(
            after["document_workflow_events"],
            before["document_workflow_events"]
        );
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM move_metadata_escrow WHERE op_id=$1")
                .bind(op)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(count, 4);
        drop(conn);
        let Json(evidence) = read_preserved(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path((op.into(), "move-page".into())),
        )
        .await
        .unwrap();
        assert_eq!(evidence["entries"].as_array().unwrap().len(), 3);
        assert!(!evidence.to_string().contains("Private saved body"));
        assert!(!evidence.to_string().contains("other-owner"));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserve_and_remove_are_inactive_immutable_history_not_empty_workflow() {
        let (state, source, target, mut req, id) = setup().await;
        req.decisions[0].choice = MetadataChoice::Remove { confirm: true };
        req.decisions[1].choice = MetadataChoice::Preserve;
        req.decisions[2].choice = MetadataChoice::Preserve;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        for name in [
            "property_values",
            "doc_tags",
            "document_workflows",
            "document_workflow_events",
        ] {
            assert!(rows_raw(&mut conn, &format!("SELECT * FROM {name}"), &[])
                .await
                .unwrap()
                .is_empty())
        }
        drop(conn);
        let Json(e) = read_preserved(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path((
                result["operation_id"].as_str().unwrap().into(),
                "move-page".into(),
            )),
        )
        .await
        .unwrap();
        let workflow = e["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["family"] == "workflow")
            .unwrap();
        assert_eq!(workflow["editable"], Value::Null);
        assert!(workflow["expected_active"]["assignments"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(workflow["original"]["events"][0]["id"], json!("event"));
        assert_eq!(e["restoration_available"], json!(false));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn explicit_create_and_stage_mapping_preserve_original_ids_times_and_evidence() {
        let (state, source, target, mut req, id) = setup().await;
        let tag = uuid::Uuid::new_v4().to_string();
        let property = uuid::Uuid::new_v4().to_string();
        let workflow = uuid::Uuid::new_v4().to_string();
        req.decisions[0].choice = MetadataChoice::Create {
            definition: json!({"id":tag,"name":"Explicit tag","color":"#112233"}),
            values: BTreeMap::new(),
            stages: BTreeMap::new(),
        };
        req.decisions[1].choice = MetadataChoice::Create {
            definition: json!({"id":property,"name":"Explicit score","property_type":"number","options":null,"sort_order":5}),
            values: BTreeMap::from([("source-value".into(), "43".into())]),
            stages: BTreeMap::new(),
        };
        req.decisions[2].choice = MetadataChoice::Create {
            definition: json!({"id":workflow,"name":"Explicit mapped review","description":null,"stages":[{"id":"new-review","name":"Review"},{"id":"new-done","name":"Done"}],"approvers":[]}),
            values: BTreeMap::new(),
            stages: BTreeMap::from([
                ("review".into(), "new-review".into()),
                ("done".into(), "new-done".into()),
            ]),
        };
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let value: (String, String, String) =
            sqlx::query_as("SELECT id,property_id,value FROM property_values")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(value, ("source-value".into(), property, "43".into()));
        let event: (String, String, String) = sqlx::query_as(
            "SELECT id,document_workflow_id,to_stage_id FROM document_workflow_events",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            event,
            ("event".into(), "workflow".into(), "new-review".into())
        );
        let original: String = sqlx::query_scalar(
            "SELECT original_json FROM move_metadata_escrow WHERE op_id=$1 AND family='workflow'",
        )
        .bind(result["operation_id"].as_str().unwrap())
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(original.contains("source-workflow") && original.contains("History note"));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn late_actor_or_history_source_changes_rollback_whole_subtree_and_receipt() {
        for trigger in [
            "UPDATE users SET is_active=0 WHERE id='mover'",
            "UPDATE document_workflow_events SET note='unexpected history'",
        ] {
            let (state, source, target, req, id) = setup().await;
            sqlx::query(&format!("CREATE TRIGGER move_late AFTER INSERT ON move_metadata_escrow WHEN NEW.family='workflow' BEGIN {trigger}; END")).execute(&state.db).await.unwrap();
            let mut conn = state.db.acquire().await.unwrap();
            let before = domain_state(&mut conn).await.unwrap();
            drop(conn);
            let error = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                AppError::Conflict(_)
                    | AppError::NotFound(_)
                    | AppError::Forbidden(_)
                    | AppError::Unauthorized(_)
            ));
            let mut conn = state.db.acquire().await.unwrap();
            assert_eq!(domain_state(&mut conn).await.unwrap(), before);
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM move_commit_receipts")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
            assert_eq!(count, 0);
            let active: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='mover'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
            assert_eq!(active, 1);
            drop(conn);
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn restriction_rows_are_same_ids_and_exact_timestamps_after_scope_transition() {
        let (state, source, target, _, old_id) = setup().await;
        let _ = old_id;
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('move-page','move-source','restricted','original-policy','mover')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('same-allowlist','move-page','move-source','mover','mover')").execute(&state.db).await.unwrap();
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let old = rows_raw(&mut conn, "SELECT * FROM document_access_allowlist", &[])
            .await
            .unwrap();
        drop(conn);
        let mut req:CommitRequest=serde_json::from_value(json!({"fingerprint":preview.fingerprint,"decisions":[{"family":"tag","source_id":"source-tag","choice":{"kind":"map","destination_id":"target-tag"}},{"family":"property","source_id":"source-property","choice":{"kind":"map","destination_id":"target-number"}},{"family":"workflow","source_id":"source-workflow","choice":{"kind":"map","destination_id":"target-workflow"}}],"public_links":"preserve","confirm_audience_change":true,"confirm_draft_space_change":true})).unwrap();
        req.fingerprint = preview.fingerprint;
        relocate(
            &state,
            &super::super::tests::test_auth("mover"),
            &preview.preview_id,
            &req,
        )
        .await
        .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let new = rows_raw(&mut conn, "SELECT * FROM document_access_allowlist", &[])
            .await
            .unwrap();
        let mut expected = old[0].clone();
        expected["space_id"] = json!("move-target");
        assert_eq!(new, vec![expected]);
        drop(conn);
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn folder_relocation_requires_exact_manage_folders_not_global_edit_or_move() {
        let (state, source, target, mut req, _old_id) = setup().await;
        sqlx::query("DELETE FROM space_members WHERE user_id='mover'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('move-only-group','Move only')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('move-only-member','move-only-group','mover')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('move-only-role','Move only','[\"view\",\"edit_content\",\"move_content\"]')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('move-only-group','move-only-role')").execute(&state.db).await.unwrap();
        let auth = super::super::tests::test_auth("mover");
        let mut conn = state.db.acquire().await.unwrap();
        access::require_move_preview_space(
            &mut conn,
            &auth,
            "move-source",
            MovePreviewCredential::Write,
        )
        .await
        .unwrap();
        access::resolve_page_access(
            &mut conn,
            access::principal_from_auth(&auth).unwrap(),
            "move-source",
            "move-page",
            access::PageOperation::Move,
            access::Purpose::Routine,
        )
        .await
        .unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        assert!(matches!(
            relocate(&state, &auth, &preview.preview_id, &req).await,
            Err(AppError::NotFound(_))
        ));
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_commit_receipts")
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
            0
        );
        drop(conn);
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn moved_attachment_old_url_uses_current_stable_asset_and_destination_access() {
        let (state, source, target, mut req, _old_id) = setup().await;
        let old_path = format!(
            "spaces/{source}/attachments/move-page/{}.bin",
            uuid::Uuid::new_v4()
        );
        let absolute = crate::storage::data_dir().join(&old_path);
        std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        std::fs::write(&absolute, b"UNCHANGED MOVE ASSET").unwrap();
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('stable-move-asset','move-page','move-source','original.bin','Original file.bin','application/octet-stream',20,$1,'mover')").bind(&old_path).execute(&state.db).await.unwrap();
        {
            let mut legacy=state.db.acquire().await.unwrap();
            crate::spaces::asset_audiences::admit_legacy(&mut legacy,"stable-move-asset","move-source","move-page").await.unwrap();
        }
        for (user, space) in [
            ("other-owner", "move-source"),
            ("SECRET-recipient", "move-target"),
        ] {
            sqlx::query(
                "INSERT INTO space_members(id,user_id,space_id,role) VALUES($1,$2,$3,'viewer')",
            )
            .bind(format!("asset-view-{user}"))
            .bind(user)
            .bind(space)
            .execute(&state.db)
            .await
            .unwrap();
        }
        let auth = super::super::tests::test_auth("mover");
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        relocate(&state, &auth, &preview.preview_id, &req)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&absolute).unwrap(), b"UNCHANGED MOVE ASSET");
        let response = super::super::super::attachments::moved_asset_test_response(
            state.clone(),
            super::super::tests::test_auth("SECRET-recipient"),
            old_path.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()[..],
            b"UNCHANGED MOVE ASSET"
        );
        assert!(matches!(
            super::super::super::attachments::moved_asset_test_response(
                state.clone(),
                super::super::tests::test_auth("other-owner"),
                old_path.clone()
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        sqlx::query("DELETE FROM space_members WHERE user_id='SECRET-recipient'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            super::super::super::attachments::moved_asset_test_response(
                state.clone(),
                super::super::tests::test_auth("SECRET-recipient"),
                old_path.clone()
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        let stable: (String, String, String) = sqlx::query_as(
            "SELECT id,space_id,original_name FROM attachments WHERE id='stable-move-asset'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            stable,
            (
                "stable-move-asset".into(),
                "move-target".into(),
                "Original file.bin".into()
            )
        );
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn committed_restart_verifies_history_without_recreating_deleted_destination() {
        let (state, source, target, req, id) = setup().await;
        let response = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = response["operation_id"].as_str().unwrap();
        let raw: String =
            sqlx::query_scalar("SELECT manifest_json FROM move_commit_receipts WHERE op_id=$1")
                .bind(op)
                .fetch_one(&state.db)
                .await
                .unwrap();
        let installed: crate::storage::relocation::PreparedRelocation =
            serde_json::from_str(&raw).unwrap();
        sqlx::query("DELETE FROM spaces WHERE id='move-target'")
            .execute(&state.db)
            .await
            .unwrap();
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM spaces WHERE id='move-target'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM documents WHERE space_id='move-target'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            0
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM move_commit_receipts WHERE op_id=$1 AND verified_at IS NOT NULL").bind(op).fetch_one(&state.db).await.unwrap(),1);
        for file in installed.files {
            assert!(crate::storage::data_dir().join(file.source).exists());
            assert!(crate::storage::data_dir().join(file.destination).exists());
        }
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn late_move_audit_corruption_or_identity_revocation_rolls_back_every_domain_row() {
        for mutation in [
            "UPDATE audit_log SET before_value='unexpected' WHERE id=NEW.id",
            "UPDATE audit_log SET after_value='unexpected' WHERE id=NEW.id",
            "UPDATE audit_log SET actor_id='other-owner' WHERE id=NEW.id",
            "UPDATE users SET is_active=0 WHERE id='mover'",
        ] {
            let (state, source, target, req, id) = setup().await;
            sqlx::query(&format!(
                "CREATE TRIGGER corrupt_move_audit AFTER INSERT ON audit_log BEGIN {mutation}; END"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            let mut conn = state.db.acquire().await.unwrap();
            let before = domain_state(&mut conn).await.unwrap();
            drop(conn);
            assert!(
                relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                    .await
                    .is_err()
            );
            let mut conn = state.db.acquire().await.unwrap();
            assert_eq!(domain_state(&mut conn).await.unwrap(), before);
            for table in [
                "audit_log",
                "move_commit_receipts",
                "move_metadata_escrow",
                "move_asset_path_aliases",
            ] {
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                        .fetch_one(&mut *conn)
                        .await
                        .unwrap(),
                    0,
                    "{table}"
                );
            }
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT is_active FROM users WHERE id='mover'")
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap(),
                1
            );
            drop(conn);
            cleanup(&source, &target).await;
        }
    }

    #[tokio::test]
    async fn deliberate_public_link_disable_rechecks_source_edit_even_when_move_remains() {
        let (state, source, target, mut req, _old_id) = setup().await;
        req.public_links = PublicLinkDisposition::Disable;
        sqlx::query("DELETE FROM space_members WHERE space_id='move-source' AND user_id='mover'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('disable-group','Disable group')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('disable-member','disable-group','mover')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('disable-role','Disable role','[\"view\",\"move_content\",\"manage_folders\",\"edit_content\"]')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('disable-group','disable-role')").execute(&state.db).await.unwrap();
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        sqlx::query("CREATE TRIGGER revoke_disable AFTER INSERT ON audit_log BEGIN UPDATE global_roles SET permissions='[\"view\",\"move_content\",\"manage_folders\"]' WHERE id='disable-role'; END").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        assert!(relocate(
            &state,
            &super::super::tests::test_auth("mover"),
            &preview.preview_id,
            &req
        )
        .await
        .is_err());
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_log")
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
            0
        );
        let permissions: String =
            sqlx::query_scalar("SELECT permissions FROM global_roles WHERE id='disable-role'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert!(permissions.contains("edit_content"));
        drop(conn);
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn real_commit_lost_ack_is_bound_historical_receipt_without_reapply() {
        let (state, source, target, req, id) = setup().await;
        MOVE_COMMIT_OUTCOME.with(|slot| *slot.borrow_mut() = Some("lost_ack"));
        let response = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        assert_eq!(response["commit_acknowledgement_recovered"], json!(true));
        let mut conn = state.db.acquire().await.unwrap();
        let committed = domain_state(&mut conn).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_log")
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
            1
        );
        drop(conn);
        let (_, Json(retry)) = commit_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path(id),
            Json(req),
        )
        .await
        .unwrap();
        assert_eq!(retry["operation_id"], response["operation_id"]);
        assert_eq!(retry["historical_receipt"], json!(true));
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), committed);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_log")
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
            1
        );
        drop(conn);
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn actual_rollback_never_false_acknowledges_and_retains_installed_evidence() {
        let (state, source, target, req, id) = setup().await;
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        MOVE_COMMIT_OUTCOME.with(|slot| *slot.borrow_mut() = Some("rollback"));
        assert!(
            matches!(relocate(&state,&super::super::tests::test_auth("mover"),&id,&req).await,Err(AppError::Internal(message))if message.contains("did not commit"))
        );
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        for table in [
            "audit_log",
            "move_commit_receipts",
            "move_metadata_escrow",
            "move_asset_path_aliases",
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap(),
                0
            )
        }
        drop(conn);
        assert!(
            std::fs::read_dir(crate::storage::space_dir(&target).join("docs/revisions"))
                .unwrap()
                .next()
                .is_some()
        );
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn real_wal_guard_wait_rechecks_external_session_revocation_before_install() {
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let mut held = state.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET id=id WHERE id='move-source'")
            .execute(&mut *held)
            .await
            .unwrap();
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        MOVE_BEFORE_GUARD.with(|slot| *slot.borrow_mut() = Some(ready.clone()));
        let worker_state = state.clone();
        let task = tokio::spawn(async move {
            relocate(
                &worker_state,
                &super::super::tests::test_auth("mover"),
                &id,
                &req,
            )
            .await
        });
        ready.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(
            !task.is_finished(),
            "actual SQLite writer guard must remain blocked"
        );
        sqlx::query("DELETE FROM sessions WHERE id='preview-mover'")
            .execute(&mut *held)
            .await
            .unwrap();
        held.commit().await.unwrap();
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap(),
            Err(AppError::Unauthorized(_))
        ));
        for table in [
            "audit_log",
            "move_commit_receipts",
            "move_metadata_escrow",
            "move_asset_path_aliases",
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&state.db)
                    .await
                    .unwrap(),
                0
            )
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM documents WHERE space_id='move-source'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            2
        );
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn move_process_child() {
        let Ok(_phase) = std::env::var("STARLANE_MOVE_COMMIT_CHILD_PHASE") else {
            return;
        };
        let marker = std::env::var("STARLANE_MOVE_COMMIT_CHILD_MARKER").unwrap();
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let databases: Vec<(i64, String, String)> = sqlx::query_as("PRAGMA database_list")
            .fetch_all(&state.db)
            .await
            .unwrap();
        let db = databases.iter().find(|r| r.1 == "main").unwrap().2.clone();
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        let raw =
            json!({"database":db,"source_slug":source,"target_slug":target,"domain_before":before})
                .to_string();
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(marker)
            .unwrap();
        file.write_all(raw.as_bytes()).unwrap();
        file.sync_all().unwrap();
        relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        panic!("child fault did not stop process");
    }
    #[tokio::test]
    async fn actual_process_crashes_before_and_after_commit_recover_without_reapply() {
        sqlx::any::install_default_drivers();
        for phase in ["before_commit", "after_commit"] {
            let marker = std::env::temp_dir()
                .join(format!("move-commit-marker-{}.json", uuid::Uuid::new_v4()));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg("spaces::moves::commit::tests::move_process_child")
                .arg("--test-threads=1")
                .env("STARLANE_MOVE_COMMIT_CHILD_PHASE", phase)
                .env("STARLANE_MOVE_COMMIT_CHILD_MARKER", &marker)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(77));
            let saved: Value = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
            let db = sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect(&format!("sqlite://{}?mode=rw", text(&saved, "database")))
                .await
                .unwrap();
            crate::storage::relocation::recover(&db).await.unwrap();
            let mut conn = db.acquire().await.unwrap();
            if phase == "before_commit" {
                assert_eq!(
                    serde_json::to_value(domain_state(&mut conn).await.unwrap()).unwrap(),
                    saved["domain_before"]
                );
                for table in [
                    "audit_log",
                    "move_commit_receipts",
                    "move_metadata_escrow",
                    "move_asset_path_aliases",
                ] {
                    assert_eq!(
                        sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                            .fetch_one(&mut *conn)
                            .await
                            .unwrap(),
                        0
                    )
                }
            } else {
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT count(*) FROM documents WHERE space_id='move-target'"
                    )
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap(),
                    2
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_log")
                        .fetch_one(&mut *conn)
                        .await
                        .unwrap(),
                    1
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT count(*) FROM move_commit_receipts WHERE verified_at IS NOT NULL"
                    )
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap(),
                    1
                );
            }
            drop(conn);
            crate::storage::relocation::recover(&db).await.unwrap();
            assert!(crate::storage::space_dir(text(&saved, "source_slug"))
                .join("docs")
                .exists());
            assert!(std::fs::read_dir(
                crate::storage::space_dir(text(&saved, "target_slug")).join("docs/revisions")
            )
            .unwrap()
            .next()
            .is_some());
            db.close().await;
            cleanup(text(&saved, "source_slug"), text(&saved, "target_slug")).await;
            std::fs::remove_file(marker).unwrap();
        }
    }

    async fn relation_setup(wal: bool) -> (AppState, String, String, CommitRequest, String) {
        let (state, source, target, mut req, _old_id) = setup_with_wal(wal).await;
        for id in ["outside-page", "outside-alternative"] {
            let path = crate::storage::write_document(
                &source,
                id,
                id,
                "title: Outside",
                "Outside source bytes",
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,folder_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,'move-source','other-folder',$1,$2,'other-owner','other-owner','outside-published')").bind(id).bind(path).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type) VALUES('source-link','move-source','Source link','relation'),('target-link','move-target','Destination link','relation')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('outgoing-edge','move-page','source-link','outside-page'),('incoming-edge','outside-page','source-link','move-page')").execute(&state.db).await.unwrap();
        req.decisions.push(MetadataDecision {
            family: MetadataFamily::Property,
            source_id: "source-link".into(),
            choice: MetadataChoice::Map {
                destination_id: "target-link".into(),
                values: BTreeMap::from([("outgoing-edge".into(), "move-child".into())]),
                stages: BTreeMap::new(),
            },
        });
        req.incident_relations = vec![
            IncidentRelationDecision {
                assignment_id: "outgoing-edge".into(),
                choice: IncidentRelationChoice::Map {
                    target_id: "move-child".into(),
                },
            },
            IncidentRelationDecision {
                assignment_id: "incoming-edge".into(),
                choice: IncidentRelationChoice::Map {
                    target_id: "outside-alternative".into(),
                },
            },
        ];
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        (state, source, target, req, preview.preview_id)
    }
    #[tokio::test]
    async fn explicit_incident_maps_preserve_assignment_ids_times_and_use_current_source_scope() {
        let (state, source, target, req, id) = relation_setup(false).await;
        let mut conn = state.db.acquire().await.unwrap();
        let before = rows_raw(
            &mut conn,
            "SELECT * FROM property_values WHERE id='incoming-edge' OR id='outgoing-edge'",
            &[],
        )
        .await
        .unwrap();
        drop(conn);
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let rows = rows_raw(
            &mut conn,
            "SELECT * FROM property_values WHERE id='incoming-edge' OR id='outgoing-edge'",
            &[],
        )
        .await
        .unwrap();
        for row in rows {
            let original = before.iter().find(|v| v["id"] == row["id"]).unwrap();
            for key in ["id", "document_id", "created_at", "updated_at"] {
                assert_eq!(row.get(key), original.get(key), "{key}")
            }
            if row["id"] == "outgoing-edge" {
                assert_eq!(row["property_id"], json!("target-link"));
                assert_eq!(row["value"], json!("move-child"))
            } else {
                assert_eq!(row["property_id"], json!("source-link"));
                assert_eq!(row["value"], json!("outside-alternative"))
            }
        }
        drop(conn);
        let Json(evidence) = read_preserved(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path((
                result["operation_id"].as_str().unwrap().into(),
                "outside-page".into(),
            )),
        )
        .await
        .unwrap();
        assert_eq!(evidence["entries"][0]["family"], json!("incident_relation"));
        assert_eq!(
            evidence["entries"][0]["original"]["assignment"]["value"],
            json!("move-page")
        );
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn incident_preserve_remove_are_sealed_before_detach_and_never_empty_active_values() {
        let (state, source, target, mut req, id) = relation_setup(false).await;
        if let MetadataChoice::Map { values, .. } = &mut req.decisions.last_mut().unwrap().choice {
            values.clear()
        }
        req.incident_relations[0].choice = IncidentRelationChoice::Preserve;
        req.incident_relations[1].choice = IncidentRelationChoice::Remove { confirm: true };
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM property_values WHERE id='incoming-edge' OR id='outgoing-edge'").fetch_one(&state.db).await.unwrap(),0);
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM move_metadata_escrow WHERE op_id=$1 AND family='incident_relation'").bind(result["operation_id"].as_str().unwrap()).fetch_one(&state.db).await.unwrap();
        assert_eq!(count, 2);
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn missing_conflicting_or_inaccessible_incident_choices_rollback_without_endpoint_labels()
    {
        for scenario in ["missing", "conflict", "locked"] {
            let (state, source, target, mut req, mut id) = relation_setup(false).await;
            match scenario {
                "missing" => req.incident_relations.clear(),
                "conflict" => {
                    req.incident_relations[0].choice = IncidentRelationChoice::Map {
                        target_id: "outside-alternative".into(),
                    }
                }
                _ => {
                    sqlx::query("UPDATE documents SET locked=1,locked_by='other-owner' WHERE id='outside-page'").execute(&state.db).await.unwrap();
                    let Json(preview) = create_preview(
                        State(state.clone()),
                        super::super::tests::test_auth("mover"),
                        Path("move-source".into()),
                        Json(super::super::tests::request("folder")),
                    )
                    .await
                    .unwrap();
                    req.fingerprint = preview.fingerprint;
                    id = preview.preview_id;
                }
            }
            let mut conn = state.db.acquire().await.unwrap();
            let before = domain_state(&mut conn).await.unwrap();
            drop(conn);
            let error = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap_err();
            assert!(matches!(error, AppError::Conflict(_)));
            assert!(!format!("{error:?}").contains("outside-page"));
            let mut conn = state.db.acquire().await.unwrap();
            assert_eq!(domain_state(&mut conn).await.unwrap(), before);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_commit_receipts")
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap(),
                0
            );
            drop(conn);
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn late_new_incoming_edge_cannot_escape_exact_expected_source_or_partially_move() {
        let (state, source, target, req, id) = relation_setup(false).await;
        sqlx::query("CREATE TRIGGER add_late_incident AFTER INSERT ON audit_log BEGIN INSERT INTO property_values(id,document_id,property_id,value) VALUES('late-edge','outside-alternative','source-link','move-child'); END").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req).await;
        assert!(matches!(result, Err(AppError::Conflict(_))), "{result:?}");
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_log")
                .fetch_one(&mut *conn)
                .await
                .unwrap(),
            0
        );
        drop(conn);
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn real_wal_incident_source_editor_share_loss_keeps_view_but_rolls_back_move() {
        let (state, source, target, mut req, _) = relation_setup(true).await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='outside-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('outside-private','outside-page','move-source','other-owner','Private outside','Private body','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('outside-private','mover','editor','other-owner')").execute(&state.db).await.unwrap();
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        let id = preview.preview_id;
        let mut held = state.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET id=id WHERE id='move-source'")
            .execute(&mut *held)
            .await
            .unwrap();
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        MOVE_BEFORE_GUARD.with(|slot| *slot.borrow_mut() = Some(ready.clone()));
        let worker = state.clone();
        let task = tokio::spawn(async move {
            relocate(&worker, &super::super::tests::test_auth("mover"), &id, &req).await
        });
        ready.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(!task.is_finished());
        sqlx::query("UPDATE document_draft_access SET role='viewer' WHERE draft_id='outside-private' AND user_id='mover'").execute(&mut *held).await.unwrap();
        let expected = domain_state(&mut held).await.unwrap();
        held.commit().await.unwrap();
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(outcome, Err(AppError::Conflict(_))), "{outcome:?}");
        let auth = super::super::tests::test_auth("mover");
        let mut conn = state.db.acquire().await.unwrap();
        access::resolve_page_access(
            &mut conn,
            access::principal_from_auth(&auth).unwrap(),
            "move-source",
            "outside-page",
            access::PageOperation::View,
            access::Purpose::Routine,
        )
        .await
        .unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), expected);
        for table in [
            "audit_log",
            "move_commit_receipts",
            "move_metadata_escrow",
            "move_asset_path_aliases",
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap(),
                0
            )
        }
        drop(conn);
        cleanup(&source, &target).await;
    }
    async fn seed_index_receipt(
        state: &AppState,
        document: Option<&str>,
        _number: u128,
        stamp: &str,
    ) -> String {
        let op = uuid::Uuid::new_v4().to_string();
        let root = crate::storage::data_dir();
        let manifest =
            crate::storage::relocation::prepare(&root, &op, &"a".repeat(64), &"b".repeat(64), &[])
                .unwrap();
        let binding = crate::storage::relocation::escrow_binding(&[]).unwrap();
        let expected = json!({"pages":document.map(|id|vec![json!({"id":id})]).unwrap_or_default(),"escrow_count":binding["escrow_count"],"escrow_fingerprint":binding["escrow_fingerprint"]});
        let receipt = json!({"op_id":op,"preview_id":format!("index-preview-{op}"),"actor_id":"mover","source_space_id":"move-source","destination_space_id":"move-target","request_fingerprint":"b".repeat(64),"source_fingerprint":"a".repeat(64),"manifest_json":serde_json::to_string(&manifest).unwrap(),"expected_state_json":expected.to_string(),"created_at":stamp});
        crate::storage::relocation::seal_receipt(&root, &receipt).unwrap();
        sqlx::query("INSERT INTO move_commit_receipts(op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&op).bind(text(&receipt,"preview_id")).bind("mover").bind("move-source").bind("move-target").bind(text(&receipt,"request_fingerprint")).bind(text(&receipt,"source_fingerprint")).bind(text(&receipt,"manifest_json")).bind(text(&receipt,"expected_state_json")).bind(stamp).execute(&state.db).await.unwrap();
        op
    }
    async fn index_for(
        state: &AppState,
        page: &str,
        cursor: Option<String>,
    ) -> Result<Json<Value>, AppError> {
        inspect_preserved_index(
            State(state.clone()),
            EvidenceReadAuth(super::super::tests::test_auth("mover")),
            Path(page.into()),
            axum::extract::Query(PreservedIndexQuery { cursor }),
        )
        .await
    }
    #[tokio::test]
    async fn preserved_index_scans_hidden_chunks_and_paginates_sealed_ties_without_counts() {
        let (state, source, target) = super::super::tests::fixture_with_wal(true).await;
        let mut expected = Vec::new();
        for number in 1..=25 {
            expected.push(
                seed_index_receipt(&state, Some("move-child"), number, "2026-10-09T00:00:00Z")
                    .await,
            );
        }
        let mut hidden = None;
        for number in 1000..1130 {
            hidden = Some(seed_index_receipt(&state, None, number, "2026-10-10T00:00:00Z").await);
        }
        // An unrelated corrupt sealed SQL row is never a catalog error oracle.
        sqlx::query("UPDATE move_commit_receipts SET manifest_json='broken' WHERE op_id=$1")
            .bind(hidden.as_ref().unwrap())
            .execute(&state.db)
            .await
            .unwrap();
        let malformed = seed_index_receipt(&state, None, 2000, "2026-10-11T00:00:00Z").await;
        sqlx::query("UPDATE move_commit_receipts SET expected_state_json='broken' WHERE op_id=$1")
            .bind(&malformed)
            .execute(&state.db)
            .await
            .unwrap();
        let foreign_escrow = seed_index_receipt(&state, None, 2001, "2026-10-12T00:00:00Z").await;
        sqlx::query("INSERT INTO move_metadata_escrow(op_id,document_id,family,source_id,disposition,original_json,expected_json) VALUES($1,'unrelated-hidden','tag','foreign','preserve','broken','broken')").bind(&foreign_escrow).execute(&state.db).await.unwrap();
        let forged_page = seed_index_receipt(&state, None, 2002, "2026-10-13T00:00:00Z").await;
        sqlx::query("UPDATE move_commit_receipts SET expected_state_json=$1 WHERE op_id=$2")
            .bind(json!({"pages":[{"id":"move-child"}]}).to_string())
            .bind(&forged_page)
            .execute(&state.db)
            .await
            .unwrap();
        let forged_incident = seed_index_receipt(&state, None, 2003, "2026-10-14T00:00:00Z").await;
        sqlx::query("INSERT INTO move_metadata_escrow(op_id,document_id,family,source_id,disposition,original_json,expected_json) VALUES($1,'move-child','incident_relation','forged','preserve','broken','broken')").bind(&forged_incident).execute(&state.db).await.unwrap();
        let before = super::super::super::documents::tests::move_database_snapshot(&state).await;
        let Json(first) = index_for(&state, "move-child", None).await.unwrap();
        assert_eq!(first["schema_version"], 1);
        assert_eq!(first["space_id"], "move-source");
        assert_eq!(first["document_id"], "move-child");
        assert_eq!(first["operations"].as_array().unwrap().len(), 20);
        expected.sort();
        expected.reverse();
        let first_ids: Vec<_> = first["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["operation_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(first_ids, expected[..20]);
        assert_eq!(first["next_cursor"], expected[19]);
        assert!(first.get("total").is_none() && first.get("hidden_count").is_none());
        let Json(second) = index_for(&state, "move-child", Some(expected[19].clone()))
            .await
            .unwrap();
        let second_ids: Vec<_> = second["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["operation_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(second_ids, expected[20..]);
        assert!(second["next_cursor"].is_null());
        let Json(end) = index_for(&state, "move-child", Some(expected.last().unwrap().clone()))
            .await
            .unwrap();
        assert!(end["operations"].as_array().unwrap().is_empty());
        for cursor in [
            hidden.unwrap(),
            malformed,
            foreign_escrow,
            forged_page,
            forged_incident,
            "missing-op".into(),
            String::new(),
        ] {
            assert!(matches!(
                index_for(&state, "move-child", Some(cursor)).await,
                Err(AppError::NotFound(_))
            ));
        }
        assert_eq!(
            super::super::super::documents::tests::move_database_snapshot(&state).await,
            before
        );
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserved_index_current_http_read_method_scope_empty_and_no_domain_writes() {
        use tower::ServiceExt;
        let (state, source, target) = super::super::tests::fixture_with_wal(true).await;
        for (key, scope) in [
            ("index-read", "read"),
            ("index-write", "write"),
            ("index-admin", "admin"),
        ] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'mover',$1,$2,$1,$3)").bind(key).bind(crate::auth::api_keys::hash_api_key(key)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES('index-service','mover','index-service',$1,'index-service','[\"admin\"]',1)").bind(crate::auth::api_keys::hash_api_key("index-service")).execute(&state.db).await.unwrap();
        let bookkeeping: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT id,last_used_at FROM api_keys ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        let before = super::super::super::documents::tests::move_database_snapshot(&state).await;
        for (key, status) in [
            ("index-read", StatusCode::OK),
            ("index-admin", StatusCode::OK),
            ("index-write", StatusCode::FORBIDDEN),
            ("index-service", StatusCode::UNAUTHORIZED),
        ] {
            let response = super::super::router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/moves/documents/move-page/preserved/inspect")
                        .header("X-API-Key", key)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            let body = axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap();
            if status == StatusCode::OK {
                let v: Value = serde_json::from_slice(&body).unwrap();
                assert!(v["operations"].as_array().unwrap().is_empty());
                assert!(v["next_cursor"].is_null());
            }
        }
        for (key, last_used) in bookkeeping {
            sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id=$2")
                .bind(last_used)
                .bind(key)
                .execute(&state.db)
                .await
                .unwrap();
        }
        assert_eq!(
            super::super::super::documents::tests::move_database_snapshot(&state).await,
            before
        );
        sqlx::query("UPDATE documents SET archived_at='hidden' WHERE id='move-page'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            index_for(&state, "move-page", None).await,
            Err(AppError::NotFound(_))
        ));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserved_index_late_wal_policy_receipt_escrow_and_identity_drift_reject() {
        for scenario in ["block", "role", "parent", "receipt", "escrow", "session"] {
            let (state, source, target) = super::super::tests::fixture_with_wal(true).await;
            let op =
                seed_index_receipt(&state, Some("move-child"), 500, "2026-10-09T00:00:00Z").await;
            let db = state.db.clone();
            let changed_op = op.clone();
            BEFORE_INDEX_FINAL.with(|slot| *slot.borrow_mut()=Some(Box::pin(async move {
                match scenario {
                    "block"=>{sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('move-source','mover','other-owner')").execute(&db).await.unwrap();},
                    "role"=>{sqlx::query("UPDATE space_members SET role='viewer' WHERE user_id='mover' AND space_id='move-source'").execute(&db).await.unwrap();},
                    "parent"=>{sqlx::query("UPDATE documents SET parent_id=NULL WHERE id='move-child'").execute(&db).await.unwrap();},
                    "receipt"=>{sqlx::query("UPDATE move_commit_receipts SET created_at='changed' WHERE op_id=$1").bind(&changed_op).execute(&db).await.unwrap();},
                    "escrow"=>{sqlx::query("INSERT INTO move_metadata_escrow(op_id,document_id,family,source_id,disposition,original_json,expected_json) VALUES($1,'move-child','tag','phantom','preserve','{}','{}')").bind(&changed_op).execute(&db).await.unwrap();},
                    _=>{sqlx::query("DELETE FROM sessions WHERE id='preview-mover'").execute(&db).await.unwrap();},
                }
            })));
            assert!(
                index_for(&state, "move-child", None).await.is_err(),
                "{scenario}"
            );
            if scenario == "role" {
                let mut conn = state.db.acquire().await.unwrap();
                assert!(access::resolve_page_access(
                    &mut conn,
                    access::principal_from_auth(&super::super::tests::test_auth("mover")).unwrap(),
                    "move-source",
                    "move-child",
                    access::PageOperation::View,
                    access::Purpose::Routine
                )
                .await
                .is_ok());
            }
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn preserved_index_receipt_wait_late_page_block_denies_with_current_http_credential() {
        use tower::ServiceExt;
        let (state, source, target) = super::super::tests::fixture_with_wal(true).await;
        seed_index_receipt(&state, Some("move-child"), 888, "2026-10-09T00:00:00Z").await;
        let tokens = crate::auth::jwt::create_token_pair_for_session(
            "mover",
            "mover@test.local",
            "member",
            &state.jwt_secret,
            Some("preview-mover"),
            30,
        )
        .unwrap();
        let db = state.db.clone();
        BEFORE_INDEX_WITNESSES.with(|slot| *slot.borrow_mut()=Some(Box::pin(async move {sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('move-source','mover','other-owner')").execute(&db).await.unwrap();})));
        let response = super::super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/moves/documents/move-child/preserved/inspect")
                    .header("Authorization", format!("Bearer {}", tokens.access_token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("operation_id"));
        let mut conn = state.db.acquire().await.unwrap();
        assert!(access::require_current_human_read(
            &mut conn,
            &super::super::tests::test_auth("mover")
        )
        .await
        .is_ok());
        drop(conn);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_commit_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserved_index_actual_signed_jwt_expiry_after_all_proofs_denies() {
        use tower::ServiceExt;
        let (state, source, target) = super::super::tests::fixture_with_wal(true).await;
        let mut claims = super::super::tests::test_auth("mover").claims;
        claims.iat = chrono::Utc::now().timestamp();
        claims.exp = claims.iat + 3;
        let expires = claims.exp;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        AFTER_INDEX_PROOFS.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                while chrono::Utc::now().timestamp() < expires {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }))
        });
        let response = super::super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/moves/documents/move-child/preserved/inspect")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let session: Option<String> =
            sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id='preview-mover'")
                .fetch_optional(&state.db)
                .await
                .unwrap();
        assert!(session.is_some());
        assert!(
            chrono::DateTime::parse_from_rfc3339(&session.unwrap())
                .unwrap()
                .timestamp()
                > expires
        );
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserved_index_incident_owner_binding_and_corrupt_bound_seal_fail_closed() {
        let (state, source, target, req, id) = relation_setup(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        let Json(index) = index_for(&state, "outside-page", None).await.unwrap();
        assert_eq!(index["operations"][0]["operation_id"], op);
        sqlx::query("UPDATE move_commit_receipts SET manifest_json='broken' WHERE op_id=$1")
            .bind(op)
            .execute(&state.db)
            .await
            .unwrap();
        let Json(hidden) = index_for(&state, "outside-page", None).await.unwrap();
        assert!(hidden["operations"].as_array().unwrap().is_empty());
        assert!(matches!(
            index_for(&state, "outside-page", Some(op.into())).await,
            Err(AppError::NotFound(_))
        ));
        let moved =
            seed_index_receipt(&state, Some("move-child"), 777, "2026-10-11T00:00:00Z").await;
        sqlx::query("INSERT INTO move_metadata_escrow(op_id,document_id,family,source_id,disposition,original_json,expected_json) VALUES($1,'move-child','tag','tamper','preserve','{}','{}')").bind(&moved).execute(&state.db).await.unwrap();
        assert!(matches!(
            index_for(&state, "move-child", None).await,
            Err(AppError::Conflict(_))
        ));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserved_evidence_never_projects_tamper_restored_before_seal_check() {
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap().to_owned();
        let original: String = sqlx::query_scalar(
            "SELECT original_json FROM move_metadata_escrow WHERE op_id=$1 AND family='tag'",
        )
        .bind(&op)
        .fetch_one(&state.db)
        .await
        .unwrap();
        let before = super::super::super::documents::tests::move_database_snapshot(&state).await;
        tamper_escrow_original(&state, &op).await;
        let db = state.db.clone();
        let restore_op = op.clone();
        BEFORE_EVIDENCE_REFERENCES.with(|slot| *slot.borrow_mut()=Some(Box::pin(async move {sqlx::query("UPDATE move_metadata_escrow SET original_json=$1 WHERE op_id=$2 AND family='tag'").bind(original).bind(restore_op).execute(&db).await.unwrap();})));
        assert!(matches!(
            read_preserved(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path((op.clone(), "move-page".into()))
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(
            super::super::super::documents::tests::move_database_snapshot(&state).await,
            before
        );
        let Json(clean) = read_preserved(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path((op, "move-page".into())),
        )
        .await
        .unwrap();
        assert!(!clean.to_string().contains("FORGED EVIDENCE"));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserved_evidence_rejects_late_source_and_related_policy_loss() {
        for scenario in [
            "source_block",
            "source_restriction",
            "incident_original",
            "incident_expected",
            "property_original",
            "property_expected",
        ] {
            let (state, source, target, req, id) = relation_setup(true).await;
            let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap();
            let page = if scenario.starts_with("incident") {
                "outside-page"
            } else {
                "move-page"
            };
            let op = result["operation_id"].as_str().unwrap().to_owned();
            let Json(before) = read_preserved(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path((op.clone(), page.into())),
            )
            .await
            .unwrap();
            assert!(!before["entries"].as_array().unwrap().is_empty());
            let sql = match scenario {
                "source_block" => "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('move-target','mover','other-owner')",
                "source_restriction" => "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('move-page','move-target','restricted','late','other-owner')",
                "incident_original" => "UPDATE documents SET archived_at='late' WHERE id='move-page'",
                "incident_expected" => "UPDATE documents SET published_at=NULL WHERE id='outside-alternative'",
                "property_original" => "UPDATE documents SET published_at=NULL WHERE id='outside-page'",
                _ => "UPDATE documents SET archived_at='late' WHERE id='move-child'",
            };
            let db = state.db.clone();
            BEFORE_EVIDENCE_IDENTITY.with(|slot| {
                *slot.borrow_mut() = Some(Box::pin(async move {
                    sqlx::query(sql).execute(&db).await.unwrap();
                }))
            });
            assert!(
                matches!(
                    read_preserved(
                        State(state.clone()),
                        super::super::tests::test_auth("mover"),
                        Path((op, page.into()))
                    )
                    .await,
                    Err(AppError::NotFound(_))
                ),
                "{scenario}"
            );
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn preserved_fresh_post_is_read_only_and_binds_page_to_sealed_operation() {
        use tower::ServiceExt;
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('unrelated-readable','move-target','Unrelated','','mover','mover','old')").execute(&state.db).await.unwrap();
        for (key, scope) in [
            ("evidence-read", "read"),
            ("evidence-write", "write"),
            ("evidence-admin", "admin"),
        ] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'mover',$1,$2,$1,$3)")
                .bind(key).bind(crate::auth::api_keys::hash_api_key(key)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
        }
        let bookkeeping: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT id,last_used_at FROM api_keys ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        let before = super::super::super::documents::tests::move_database_snapshot(&state).await;
        let source_files = super::super::super::documents::tests::move_file_snapshot(&source);
        let target_files = super::super::super::documents::tests::move_file_snapshot(&target);
        for (method, key, page, operation, expected) in [
            ("GET", "evidence-read", "move-child", op, StatusCode::OK),
            ("POST", "evidence-read", "move-child", op, StatusCode::OK),
            ("POST", "evidence-admin", "move-page", op, StatusCode::OK),
            (
                "POST",
                "evidence-write",
                "move-page",
                op,
                StatusCode::FORBIDDEN,
            ),
            (
                "POST",
                "evidence-read",
                "unrelated-readable",
                op,
                StatusCode::NOT_FOUND,
            ),
            (
                "POST",
                "evidence-read",
                "unrelated-readable",
                "absent-op",
                StatusCode::NOT_FOUND,
            ),
        ] {
            let suffix = if method == "POST" { "/inspect" } else { "" };
            let response = super::super::router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri(format!(
                            "/moves/commits/{operation}/preserved/{page}{suffix}"
                        ))
                        .header("X-API-Key", key)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let body = axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap();
            assert_eq!(
                status,
                expected,
                "{method}/{key}/{page}: {}",
                String::from_utf8_lossy(&body)
            );
            if status == StatusCode::OK {
                let v: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(v["space_id"], "move-target");
                assert_eq!(v["editable"], false);
                assert_eq!(v["restoration_available"], false);
                if page == "move-child" {
                    assert!(v["entries"].as_array().unwrap().is_empty());
                }
            }
        }
        for (key, last_used) in bookkeeping {
            sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id=$2")
                .bind(last_used)
                .bind(key)
                .execute(&state.db)
                .await
                .unwrap();
        }
        assert_eq!(
            super::super::super::documents::tests::move_database_snapshot(&state).await,
            before
        );
        assert_eq!(
            super::super::super::documents::tests::move_file_snapshot(&source),
            source_files
        );
        assert_eq!(
            super::super::super::documents::tests::move_file_snapshot(&target),
            target_files
        );
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn preserved_foreign_corrupt_operations_are_uniformly_unavailable() {
        use tower::ServiceExt;
        for scenario in [
            "escrow",
            "malformed_expected",
            "forged_page",
            "forged_incident",
            "manifest",
        ] {
            let (state, source, target, req, id) = setup_with_wal(true).await;
            let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap();
            let op = result["operation_id"].as_str().unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('unrelated-readable','move-target','Unrelated','','mover','mover','old')").execute(&state.db).await.unwrap();
            sqlx::query(r#"INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('uniform-read','mover','uniform-read',$1,'uniform-read','["read"]')"#)
                .bind(crate::auth::api_keys::hash_api_key("uniform-read")).execute(&state.db).await.unwrap();
            match scenario {
                "escrow" => tamper_escrow_original(&state, op).await,
                "malformed_expected" => {
                    sqlx::query(
                        "UPDATE move_commit_receipts SET expected_state_json='{' WHERE op_id=$1",
                    )
                    .bind(op)
                    .execute(&state.db)
                    .await
                    .unwrap();
                }
                "forged_page" => {
                    let raw: String = sqlx::query_scalar(
                        "SELECT expected_state_json FROM move_commit_receipts WHERE op_id=$1",
                    )
                    .bind(op)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
                    let mut expected: Value = serde_json::from_str(&raw).unwrap();
                    expected["pages"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":"unrelated-readable"}));
                    sqlx::query(
                        "UPDATE move_commit_receipts SET expected_state_json=$1 WHERE op_id=$2",
                    )
                    .bind(expected.to_string())
                    .bind(op)
                    .execute(&state.db)
                    .await
                    .unwrap();
                }
                "forged_incident" => {
                    sqlx::query("INSERT INTO move_metadata_escrow(op_id,document_id,family,source_id,disposition,original_json,expected_json) VALUES($1,'unrelated-readable','incident_relation','forged-incident','preserve','{','{}')").bind(op).execute(&state.db).await.unwrap();
                }
                _ => {
                    sqlx::query(
                        "UPDATE move_commit_receipts SET manifest_json='{}' WHERE op_id=$1",
                    )
                    .bind(op)
                    .execute(&state.db)
                    .await
                    .unwrap();
                }
            }
            let before =
                super::super::super::documents::tests::move_database_snapshot(&state).await;
            let source_files = super::super::super::documents::tests::move_file_snapshot(&source);
            let target_files = super::super::super::documents::tests::move_file_snapshot(&target);
            for method in ["GET", "POST"] {
                let suffix = if method == "POST" { "/inspect" } else { "" };
                let mut results = Vec::new();
                for operation in ["absent-op", op] {
                    let response = super::super::router().with_state(state.clone()).oneshot(
                        axum::http::Request::builder().method(method)
                            .uri(format!("/moves/commits/{operation}/preserved/unrelated-readable{suffix}"))
                            .header("X-API-Key", "uniform-read").body(axum::body::Body::empty()).unwrap()
                    ).await.unwrap();
                    let status = response.status();
                    let body = axum::body::to_bytes(response.into_body(), 65536)
                        .await
                        .unwrap();
                    results.push((status, body));
                }
                assert_eq!(results[0].0, StatusCode::NOT_FOUND, "{scenario}/{method}");
                assert_eq!(
                    results[0], results[1],
                    "{scenario}/{method}: foreign corruption must not disclose receipt existence"
                );
            }
            // Only the existing extractor's ordinary key last-use bookkeeping is restored.
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id='uniform-read'")
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                super::super::super::documents::tests::move_database_snapshot(&state).await,
                before,
                "{scenario}"
            );
            assert_eq!(
                super::super::super::documents::tests::move_file_snapshot(&source),
                source_files
            );
            assert_eq!(
                super::super::super::documents::tests::move_file_snapshot(&target),
                target_files
            );
            cleanup(&source, &target).await;
        }
    }

    #[tokio::test]
    async fn preserved_evidence_last_read_proof_rejects_expiry_and_external_session_loss() {
        for scenario in ["jwt", "session"] {
            let (state, source, target, req, id) = setup_with_wal(true).await;
            let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap();
            let mut auth = super::super::tests::test_auth("mover");
            let db = state.db.clone();
            if scenario == "jwt" {
                auth.claims.exp = chrono::Utc::now().timestamp() + 3;
                let expires = auth.claims.exp;
                BEFORE_EVIDENCE_IDENTITY.with(|slot| {
                    *slot.borrow_mut() = Some(Box::pin(async move {
                        while chrono::Utc::now().timestamp() < expires {
                            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        }
                    }))
                });
            } else {
                BEFORE_EVIDENCE_IDENTITY.with(|slot| {
                    *slot.borrow_mut() = Some(Box::pin(async move {
                        sqlx::query("DELETE FROM sessions WHERE id='preview-mover'")
                            .execute(&db)
                            .await
                            .unwrap();
                    }))
                });
            }
            assert!(
                matches!(
                    read_preserved(
                        State(state.clone()),
                        auth,
                        Path((
                            result["operation_id"].as_str().unwrap().into(),
                            "move-page".into()
                        ))
                    )
                    .await,
                    Err(AppError::Unauthorized(_))
                ),
                "{scenario}"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_commit_receipts")
                    .fetch_one(&state.db)
                    .await
                    .unwrap(),
                1
            );
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn preserved_relation_entries_require_current_original_and_expected_target_view() {
        for scenario in [
            "incident_original",
            "incident_expected",
            "property_original",
            "property_expected",
        ] {
            let (state, source, target, req, id) = relation_setup(true).await;
            let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap();
            let page = if scenario.starts_with("incident") {
                "outside-page"
            } else {
                "move-page"
            };
            let op = result["operation_id"].as_str().unwrap().to_string();
            let Json(before) = read_preserved(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path((op.clone(), page.into())),
            )
            .await
            .unwrap();
            assert!(before["entries"].as_array().unwrap().iter().any(|e| {
                if scenario.starts_with("incident") {
                    e["family"] == "incident_relation"
                } else {
                    e["family"] == "property" && e["source_id"] == "source-link"
                }
            }));
            let sql = match scenario {
                "incident_original" => {
                    "UPDATE documents SET archived_at='hidden' WHERE id='move-page'"
                }
                "incident_expected" => {
                    "UPDATE documents SET published_at=NULL WHERE id='outside-alternative'"
                }
                "property_original" => {
                    "UPDATE documents SET published_at=NULL WHERE id='outside-page'"
                }
                _ => "UPDATE documents SET archived_at='hidden' WHERE id='move-child'",
            };
            let db = state.db.clone();
            BEFORE_EVIDENCE_REFERENCES.with(|slot| {
                *slot.borrow_mut() = Some(Box::pin(async move {
                    sqlx::query(sql).execute(&db).await.unwrap();
                }))
            });
            let Json(after) = read_preserved(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path((op, page.into())),
            )
            .await
            .unwrap();
            assert!(
                !after["entries"].as_array().unwrap().iter().any(|e| {
                    if scenario.starts_with("incident") {
                        e["family"] == "incident_relation"
                    } else {
                        e["family"] == "property" && e["source_id"] == "source-link"
                    }
                }),
                "{scenario}"
            );
            assert!(after.get("hidden_count").is_none());
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn recovery_and_historical_ack_cannot_certify_any_changed_logical_receipt_field() {
        for field in [
            "op_id",
            "preview_id",
            "actor_id",
            "source_space_id",
            "destination_space_id",
            "request_fingerprint",
            "source_fingerprint",
            "manifest_json",
            "expected_state_json",
            "created_at",
        ] {
            let (state, source, target, req, id) = setup_with_wal(true).await;
            let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap();
            let op = result["operation_id"].as_str().unwrap();
            let altered = if field == "op_id" {
                uuid::Uuid::new_v4().to_string()
            } else {
                "altered".into()
            };
            let mut tamper = state.db.acquire().await.unwrap();
            if field == "op_id" {
                sqlx::query("PRAGMA foreign_keys=OFF")
                    .execute(&mut *tamper)
                    .await
                    .unwrap();
            }
            sqlx::query(&format!(
                "UPDATE move_commit_receipts SET {field}=$1,verified_at=NULL WHERE op_id=$2"
            ))
            .bind(&altered)
            .bind(op)
            .execute(&mut *tamper)
            .await
            .unwrap();
            if field == "op_id" {
                sqlx::query("PRAGMA foreign_keys=ON")
                    .execute(&mut *tamper)
                    .await
                    .unwrap();
            }
            drop(tamper);
            assert!(
                commit_preview(
                    State(state.clone()),
                    super::super::tests::test_auth("mover"),
                    Path(id),
                    Json(req)
                )
                .await
                .is_err(),
                "historical {field}"
            );
            crate::storage::relocation::recover(&state.db)
                .await
                .unwrap();
            let verified: Option<String> =
                sqlx::query_scalar("SELECT verified_at FROM move_commit_receipts")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert!(verified.is_none(), "changed {field} was certified");
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn recovery_rechecks_complete_logical_receipt_after_actual_wal_guard_wait() {
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap().to_string();
        let mut held = state.db.begin().await.unwrap();
        sqlx::query("UPDATE move_commit_receipts SET verified_at=verified_at WHERE op_id=$1")
            .bind(&op)
            .execute(&mut *held)
            .await
            .unwrap();
        let ready = std::sync::Arc::new(tokio::sync::Notify::new());
        crate::storage::relocation::BEFORE_RECOVERY_GUARD
            .with(|slot| *slot.borrow_mut() = Some(ready.clone()));
        let db = state.db.clone();
        let task = tokio::spawn(async move { crate::storage::relocation::recover(&db).await });
        ready.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(!task.is_finished());
        sqlx::query("UPDATE move_commit_receipts SET actor_id='altered' WHERE op_id=$1")
            .bind(&op)
            .execute(&mut *held)
            .await
            .unwrap();
        held.commit().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(sqlx::query_scalar::<_, Option<String>>(
            "SELECT verified_at FROM move_commit_receipts WHERE op_id=$1"
        )
        .bind(&op)
        .fetch_one(&state.db)
        .await
        .unwrap()
        .is_none());
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn recovery_last_sql_boundary_rechecks_independent_full_receipt_envelope() {
        let (state, source, target, req, id) = setup().await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap().to_string();
        let path = crate::storage::data_dir()
            .join(".move-relocation-journal")
            .join(&op)
            .join("receipt.json");
        crate::storage::relocation::AFTER_RECOVERY_SQL.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                let mut receipt: Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                receipt["receipt"]["actor_id"] = json!("altered");
                std::fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
            }))
        });
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        assert!(sqlx::query_scalar::<_, Option<String>>(
            "SELECT verified_at FROM move_commit_receipts WHERE op_id=$1"
        )
        .bind(&op)
        .fetch_one(&state.db)
        .await
        .unwrap()
        .is_none());
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn known_commit_and_lost_ack_refresh_actual_destination_search_without_historical_reapply(
    ) {
        for lost_ack in [false, true] {
            let (state, source, target, req, id) = setup().await;
            if lost_ack {
                MOVE_COMMIT_OUTCOME.with(|slot| *slot.borrow_mut() = Some("lost_ack"));
            }
            let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap();
            assert_eq!(result["committed"], json!(true));
            let query = crate::search::SearchQuery {
                q: Some("Preserved".into()),
                space_id: Some("move-target".into()),
                is_draft: Some(false),
                ..Default::default()
            };
            let auth = super::super::tests::test_auth("mover");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                let results = crate::search::search_visible_pages(&state, &auth, &query)
                    .await
                    .unwrap();
                if results
                    .iter()
                    .filter(|r| ["move-page", "move-child"].contains(&r.document_id.as_str()))
                    .count()
                    == 2
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "destination published search was not refreshed"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            let Json(_) = read_preserved(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path((
                    result["operation_id"].as_str().unwrap().into(),
                    "move-page".into(),
                )),
            )
            .await
            .unwrap();
            sqlx::query(
                "UPDATE documents SET title='Later title',updated_at='later' WHERE id='move-page'",
            )
            .execute(&state.db)
            .await
            .unwrap();
            let before = state
                .search_index
                .snapshot("Preserved")
                .unwrap()
                .page(0, 20)
                .unwrap()
                .into_iter()
                .map(|h| (h.doc_id, h.space_id, h.title, h.body, h.tags))
                .collect::<Vec<_>>();
            let _ = commit_preview(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path(id),
                Json(req),
            )
            .await
            .unwrap();
            let after = state
                .search_index
                .snapshot("Preserved")
                .unwrap()
                .page(0, 20)
                .unwrap()
                .into_iter()
                .map(|h| (h.doc_id, h.space_id, h.title, h.body, h.tags))
                .collect::<Vec<_>>();
            assert_eq!(after, before);
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn advisory_reindex_missing_installed_source_never_changes_known_commit_ack() {
        let (state, source, target, req, id) = setup().await;
        let db = state.db.clone();
        BEFORE_ADVISORY_REINDEX.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                let path: String =
                    sqlx::query_scalar("SELECT content_path FROM documents WHERE id='move-page'")
                        .fetch_one(&db)
                        .await
                        .unwrap();
                let full = crate::storage::data_dir().join(path);
                std::fs::rename(&full, full.with_extension("retained-fixture-evidence")).unwrap();
            }))
        });
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        assert_eq!(result["committed"], json!(true));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_commit_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn verification_status_trigger_cannot_alter_sealed_logical_receipt() {
        let (state, source, target, req, id) = setup().await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let original = immutable_receipt(&mut conn, op).await.unwrap();
        drop(conn);
        sqlx::query("CREATE TRIGGER corrupt_verification AFTER UPDATE OF verified_at ON move_commit_receipts WHEN NEW.verified_at IS NOT NULL BEGIN UPDATE move_commit_receipts SET created_at='altered' WHERE op_id=NEW.op_id; END").execute(&state.db).await.unwrap();
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(immutable_receipt(&mut conn, op).await.unwrap(), original);
        assert!(sqlx::query_scalar::<_, Option<String>>(
            "SELECT verified_at FROM move_commit_receipts WHERE op_id=$1"
        )
        .bind(op)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
        .is_none());
        drop(conn);
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn late_receipt_insert_label_changes_rollback_whole_move_before_ack() {
        for field in ["actor_id", "created_at", "expected_state_json"] {
            let (state, source, target, req, id) = setup().await;
            sqlx::query(&format!("CREATE TRIGGER corrupt_insert AFTER INSERT ON move_commit_receipts BEGIN UPDATE move_commit_receipts SET {field}='altered' WHERE op_id=NEW.op_id; END")).execute(&state.db).await.unwrap();
            let mut conn = state.db.acquire().await.unwrap();
            let before = domain_state(&mut conn).await.unwrap();
            drop(conn);
            assert!(
                relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                    .await
                    .is_err()
            );
            let mut conn = state.db.acquire().await.unwrap();
            assert_eq!(domain_state(&mut conn).await.unwrap(), before);
            for table in [
                "audit_log",
                "move_commit_receipts",
                "move_metadata_escrow",
                "move_asset_path_aliases",
            ] {
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                        .fetch_one(&mut *conn)
                        .await
                        .unwrap(),
                    0
                )
            }
            drop(conn);
            cleanup(&source, &target).await;
        }
    }
    async fn tamper_escrow_original(state: &AppState, op: &str) {
        let raw: String = sqlx::query_scalar(
            "SELECT original_json FROM move_metadata_escrow WHERE op_id=$1 AND family='tag'",
        )
        .bind(op)
        .fetch_one(&state.db)
        .await
        .unwrap();
        let mut evidence: Value = serde_json::from_str(&raw).unwrap();
        evidence["definition"]["name"] = json!("FORGED EVIDENCE");
        sqlx::query(
            "UPDATE move_metadata_escrow SET original_json=$1 WHERE op_id=$2 AND family='tag'",
        )
        .bind(evidence.to_string())
        .bind(op)
        .execute(&state.db)
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn postcommit_escrow_original_tamper_is_rejected_by_actual_preserved_get() {
        use tower::ServiceExt;
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        tamper_escrow_original(&state, op).await;
        let tokens = crate::auth::jwt::create_token_pair_for_session(
            "mover",
            "mover@test.local",
            "member",
            &state.jwt_secret,
            Some("preview-mover"),
            30,
        )
        .unwrap();
        let response = super::super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/moves/commits/{op}/preserved/move-page"))
                    .header("Authorization", format!("Bearer {}", tokens.access_token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "{}",
            String::from_utf8_lossy(&body)
        );
        assert!(!String::from_utf8_lossy(&body).contains("FORGED EVIDENCE"));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn postcommit_escrow_original_tamper_is_not_certified_by_restart() {
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        tamper_escrow_original(&state, op).await;
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        assert!(sqlx::query_scalar::<_, Option<String>>(
            "SELECT verified_at FROM move_commit_receipts WHERE op_id=$1"
        )
        .bind(op)
        .fetch_one(&state.db)
        .await
        .unwrap()
        .is_none());
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn restart_rechecks_previously_verified_escrow_without_erasing_last_success() {
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        let verified: Option<String> =
            sqlx::query_scalar("SELECT verified_at FROM move_commit_receipts WHERE op_id=$1")
                .bind(op)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(verified.is_some());
        // A receipt verified on an earlier boot is still rechecked, not skipped.
        sqlx::query(
            "UPDATE move_commit_receipts SET verified_at='historical-success' WHERE op_id=$1",
        )
        .bind(op)
        .execute(&state.db)
        .await
        .unwrap();
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        let refreshed: Option<String> =
            sqlx::query_scalar("SELECT verified_at FROM move_commit_receipts WHERE op_id=$1")
                .bind(op)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(refreshed.is_some());
        assert_ne!(refreshed.as_deref(), Some("historical-success"));
        tamper_escrow_original(&state, op).await;
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        let retained: Option<String> =
            sqlx::query_scalar("SELECT verified_at FROM move_commit_receipts WHERE op_id=$1")
                .bind(op)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(retained, refreshed);
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        drop(conn);
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn whole_operation_escrow_digest_rejects_every_raw_field_add_remove_and_equivalent_json_reorder(
    ) {
        for scenario in [
            "original_json",
            "expected_json",
            "family",
            "source_id",
            "disposition",
            "document_id",
            "add",
            "remove",
            "raw_reorder",
        ] {
            let (state, source, target, req, id) = setup_with_wal(true).await;
            let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
                .await
                .unwrap();
            let op = result["operation_id"].as_str().unwrap();
            let mut conn = state.db.acquire().await.unwrap();
            let receipt = immutable_receipt(&mut conn, op).await.unwrap();
            drop(conn);
            match scenario {
                "add" => {
                    sqlx::query("INSERT INTO move_metadata_escrow SELECT op_id,document_id,family,'added-entry',disposition,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1 AND family='tag'").bind(op).execute(&state.db).await.unwrap();
                }
                "remove" => {
                    sqlx::query("DELETE FROM move_metadata_escrow WHERE op_id=$1 AND family='tag'")
                        .bind(op)
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                "raw_reorder" => {
                    let original:String=sqlx::query_scalar("SELECT original_json FROM move_metadata_escrow WHERE op_id=$1 AND family='tag'").bind(op).fetch_one(&state.db).await.unwrap();
                    let parsed: Value = serde_json::from_str(&original).unwrap();
                    let reordered = format!(
                        "{{\"events\":{},\"definition\":{},\"assignments\":{}}}",
                        parsed["events"], parsed["definition"], parsed["assignments"]
                    );
                    assert_ne!(reordered, original);
                    assert_eq!(serde_json::from_str::<Value>(&reordered).unwrap(), parsed);
                    sqlx::query("UPDATE move_metadata_escrow SET original_json=$1 WHERE op_id=$2 AND family='tag'").bind(reordered).bind(op).execute(&state.db).await.unwrap();
                }
                field => {
                    let value = match field {
                        "disposition" => "remove",
                        "original_json" | "expected_json" => "{}",
                        _ => "altered",
                    };
                    sqlx::query(&format!("UPDATE move_metadata_escrow SET {field}=$1 WHERE op_id=$2 AND family='tag'")).bind(value).bind(op).execute(&state.db).await.unwrap();
                }
            }
            assert!(
                matches!(
                    read_preserved(
                        State(state.clone()),
                        super::super::tests::test_auth("mover"),
                        Path((op.into(), "move-page".into()))
                    )
                    .await,
                    Err(AppError::Conflict(_))
                ),
                "read {scenario}"
            );
            assert!(
                commit_preview(
                    State(state.clone()),
                    super::super::tests::test_auth("mover"),
                    Path(id),
                    Json(req)
                )
                .await
                .is_err(),
                "historical {scenario}"
            );
            assert!(
                confirm_receipt(&crate::storage::data_dir(), &state.db, &receipt)
                    .await
                    .is_err(),
                "uncertain {scenario}"
            );
            crate::storage::relocation::recover(&state.db)
                .await
                .unwrap();
            assert!(
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT verified_at FROM move_commit_receipts WHERE op_id=$1"
                )
                .bind(op)
                .fetch_one(&state.db)
                .await
                .unwrap()
                .is_none(),
                "restart {scenario}"
            );
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn escrow_read_final_whole_set_proof_sees_external_wal_tamper_after_source_proof() {
        let (state, source, target, req, id) = setup_with_wal(true).await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap().to_string();
        let other = state.clone();
        let captured = op.clone();
        BEFORE_EVIDENCE_IDENTITY.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                tamper_escrow_original(&other, &captured).await;
            }))
        });
        assert!(matches!(
            read_preserved(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path((op, "move-page".into()))
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn recovery_final_status_trigger_escrow_tamper_rolls_back_certification_and_evidence_change(
    ) {
        let (state, source, target, req, id) = setup().await;
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        let before:Vec<(String,String,String)>=sqlx::query_as("SELECT source_id,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1 ORDER BY source_id").bind(op).fetch_all(&state.db).await.unwrap();
        sqlx::query("CREATE TRIGGER corrupt_escrow_at_verification AFTER UPDATE OF verified_at ON move_commit_receipts WHEN NEW.verified_at IS NOT NULL BEGIN UPDATE move_metadata_escrow SET original_json='{}' WHERE op_id=NEW.op_id AND family='tag'; END").execute(&state.db).await.unwrap();
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        let after:Vec<(String,String,String)>=sqlx::query_as("SELECT source_id,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1 ORDER BY source_id").bind(op).fetch_all(&state.db).await.unwrap();
        assert_eq!(after, before);
        assert!(sqlx::query_scalar::<_, Option<String>>(
            "SELECT verified_at FROM move_commit_receipts WHERE op_id=$1"
        )
        .bind(op)
        .fetch_one(&state.db)
        .await
        .unwrap()
        .is_none());
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn move_identifier_collision_requires_explicit_resolution_before_files_and_preserves_escrow(
    ) {
        let (state, source, target, mut req, old_id) = setup().await;
        sqlx::query("UPDATE documents SET human_identifier='REF-1',human_identifier_key='REF-1' WHERE id='move-page'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET human_identifier='Child.Ref',human_identifier_key='CHILD.REF' WHERE id='move-child'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,human_identifier,human_identifier_key,archived_at) VALUES('hidden-identifier-holder','move-target','SECRET hidden destination','','other-owner','other-owner','ref-1','REF-1','old')").execute(&state.db).await.unwrap();
        let auth = super::super::tests::test_auth("mover");
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        assert!(preview.inventory["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "move-page")
            .unwrap()["identifier_resolution_required"]
            .as_bool()
            .unwrap());
        assert!(!preview
            .inventory
            .to_string()
            .contains("hidden-identifier-holder"));
        assert!(!preview
            .inventory
            .to_string()
            .contains("SECRET hidden destination"));
        req.fingerprint = preview.fingerprint;
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        drop(conn);
        assert!(relocate(&state, &auth, &preview.preview_id, &req)
            .await
            .is_err());
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        drop(conn);
        req.identifier_decisions.push(IdentifierDecision {
            document_id: "move-page".into(),
            choice: IdentifierChoice::Rename {
                value: " Ref-2 ".into(),
            },
        });
        let result = relocate(&state, &auth, &preview.preview_id, &req)
            .await
            .unwrap();
        let op = text(&result, "operation_id");
        let row=rows_raw(&mut *state.db.acquire().await.unwrap(),"SELECT id,space_id,human_identifier,human_identifier_key,updated_at,published_at FROM documents WHERE id=$1",&["move-page"]).await.unwrap().remove(0);
        assert_eq!(row["human_identifier"], "Ref-2");
        assert_eq!(row["human_identifier_key"], "REF-2");
        assert_eq!(row["id"], "move-page");
        let child = rows_raw(
            &mut *state.db.acquire().await.unwrap(),
            "SELECT * FROM documents WHERE id=$1",
            &["move-child"],
        )
        .await
        .unwrap()
        .remove(0);
        let original_child = before["documents"]
            .iter()
            .find(|p| p["id"] == "move-child")
            .unwrap();
        for field in [
            "human_identifier",
            "human_identifier_key",
            "updated_at",
            "published_at",
            "created_at",
            "created_by",
            "last_updated_by",
        ] {
            assert_eq!(child[field], original_child[field], "{field}");
        }

        let evidence:String=sqlx::query_scalar("SELECT original_json FROM move_metadata_escrow WHERE op_id=$1 AND document_id='move-page' AND family='identifier'").bind(op).fetch_one(&state.db).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&evidence).unwrap()["human_identifier"],
            "REF-1"
        );
        let Json(read) = read_preserved(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path((op.into(), "move-page".into())),
        )
        .await
        .unwrap();
        assert!(read["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["family"] == "identifier"));
        let historical = commit_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path(preview.preview_id),
            Json(req),
        )
        .await
        .unwrap()
        .1
         .0;
        assert_eq!(historical["historical_receipt"], true);
        let _ = old_id;
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn move_identifier_legacy_empty_decisions_keep_receipt_identity_and_reject_unrelated_choices(
    ) {
        let (state, source, target, mut req, id) = setup().await;
        let old = serde_json::to_value(&req).unwrap();
        assert!(old.get("identifier_decisions").is_none());
        assert!(old.get("maintenance_decisions").is_none());
        let result = relocate(&state, &super::super::tests::test_auth("mover"), &id, &req)
            .await
            .unwrap();
        let op = text(&result, "operation_id");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM move_metadata_escrow WHERE op_id=$1 AND family='identifier'",
        )
        .bind(op)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(count, 0);
        let parsed: CommitRequest = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), old);
        let mut conn = state.db.acquire().await.unwrap();
        let old_receipt = immutable_receipt(&mut conn, op).await.unwrap();
        assert!(
            serde_json::from_str::<Value>(text(&old_receipt, "expected_state_json"))
                .unwrap()
                .get("identifier_decisions")
                .is_none()
        );
        let maintenance_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM move_metadata_escrow WHERE op_id=$1 AND family='maintenance'",
        )
        .bind(op)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(maintenance_count, 0);
        let old_escrow = rows_raw(
            &mut conn,
            "SELECT * FROM move_metadata_escrow WHERE op_id=$1",
            &[op],
        )
        .await
        .unwrap();
        let old_aliases = rows_raw(&mut conn, "SELECT * FROM move_asset_path_aliases", &[])
            .await
            .unwrap();
        drop(conn);
        sqlx::query("UPDATE documents SET human_identifier='Later-1',human_identifier_key='LATER-1',maintenance_user_id='mover' WHERE id='move-page'").execute(&state.db).await.unwrap();
        crate::storage::relocation::recover(&state.db)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(immutable_receipt(&mut conn, op).await.unwrap(), old_receipt);
        assert_eq!(
            rows_raw(
                &mut conn,
                "SELECT * FROM move_metadata_escrow WHERE op_id=$1",
                &[op]
            )
            .await
            .unwrap(),
            old_escrow
        );
        assert_eq!(
            rows_raw(&mut conn, "SELECT * FROM move_asset_path_aliases", &[])
                .await
                .unwrap(),
            old_aliases
        );
        drop(conn);

        let Json(_) = read_preserved(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path((op.into(), "move-page".into())),
        )
        .await
        .unwrap();
        let historical = commit_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path(id.clone()),
            Json(req.clone()),
        )
        .await
        .unwrap()
        .1
         .0;
        assert_eq!(historical["historical_receipt"], true);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT human_identifier FROM documents WHERE id='move-page'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "Later-1"
        );
        req.identifier_decisions.push(IdentifierDecision {
            document_id: "missing".into(),
            choice: IdentifierChoice::Clear,
        });
        assert!(commit_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path(id),
            Json(req)
        )
        .await
        .is_err());
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn move_only_actor_preserves_identifier_but_cannot_assign_or_clear_it() {
        let (state, source, target, mut req, _id) = setup().await;
        sqlx::query("UPDATE documents SET human_identifier='Keep-1',human_identifier_key='KEEP-1',maintenance_user_id='other-owner' WHERE id='move-page'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='viewer' WHERE user_id='mover'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('identifier-movers','Identifier movers')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO global_roles(id,name,permissions) VALUES('identifier-move-only','Identifier move only','["view","move_content","manage_folders"]')"#).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('identifier-mover-member','identifier-movers','mover')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('identifier-movers','identifier-move-only')").execute(&state.db).await.unwrap();
        let auth = super::super::tests::test_auth("mover");
        let mut conn = state.db.acquire().await.unwrap();
        let prepared = review_move_current(
            &mut conn,
            &auth,
            "move-source",
            &super::super::tests::request("folder"),
            MovePreviewCredential::Write,
        )
        .await
        .unwrap();
        assert_eq!(
            prepared.review.inventory["pages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == "move-page")
                .unwrap()["identifier_edit_allowed"],
            false
        );
        let preserved = plan_identifiers(&prepared, &req).unwrap();
        require_identifier_edits(&mut conn, &auth, "move-source", &prepared, &preserved)
            .await
            .unwrap();
        for choice in [
            IdentifierChoice::Clear,
            IdentifierChoice::Rename {
                value: "New-1".into(),
            },
        ] {
            req.identifier_decisions = vec![IdentifierDecision {
                document_id: "move-page".into(),
                choice,
            }];
            let planned = plan_identifiers(&prepared, &req).unwrap();
            assert!(
                require_identifier_edits(&mut conn, &auth, "move-source", &prepared, &planned)
                    .await
                    .is_err()
            );
        }
        req.identifier_decisions = vec![IdentifierDecision {
            document_id: "missing".into(),
            choice: IdentifierChoice::Clear,
        }];
        assert!(plan_identifiers(&prepared, &req).is_err());
        req.identifier_decisions = vec![
            IdentifierDecision {
                document_id: "move-page".into(),
                choice: IdentifierChoice::Clear,
            },
            IdentifierDecision {
                document_id: "move-page".into(),
                choice: IdentifierChoice::Clear,
            },
        ];
        assert!(plan_identifiers(&prepared, &req).is_err());
        req.identifier_decisions.clear();
        let planned = plan_maintenance(&prepared, &req).unwrap();
        require_maintenance_edits(&mut conn, &auth, "move-source", &prepared, &planned)
            .await
            .unwrap();
        for choice in [
            MaintenanceChoice::Clear,
            MaintenanceChoice::User {
                user_id: "mover".into(),
            },
        ] {
            req.maintenance_decisions = vec![MaintenanceDecision {
                document_id: "move-page".into(),
                choice,
            }];
            let planned = plan_maintenance(&prepared, &req).unwrap();
            assert!(require_maintenance_edits(
                &mut conn,
                &auth,
                "move-source",
                &prepared,
                &planned
            )
            .await
            .is_err());
        }
        req.maintenance_decisions = vec![MaintenanceDecision {
            document_id: "missing".into(),
            choice: MaintenanceChoice::Clear,
        }];
        assert!(plan_maintenance(&prepared, &req).is_err());
        req.maintenance_decisions = vec![
            MaintenanceDecision {
                document_id: "move-page".into(),
                choice: MaintenanceChoice::Clear
            };
            2
        ];
        assert!(plan_maintenance(&prepared, &req).is_err());
        drop(conn);
        cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn renamed_identifier_late_source_edit_loss_with_move_retained_rolls_back_whole_operation(
    ) {
        let (state, source, target, mut req, _) = setup().await;
        sqlx::query("UPDATE documents SET human_identifier='Before-1',human_identifier_key='BEFORE-1' WHERE id='move-page'").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO groups(id,name) VALUES('late-identifier-movers','Late identifier movers')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO global_roles(id,name,permissions) VALUES('late-identifier-move','Late identifier move','["view","move_content","manage_folders"]')"#).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('late-identifier-member','late-identifier-movers','mover')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('late-identifier-movers','late-identifier-move')").execute(&state.db).await.unwrap();
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        req.identifier_decisions = vec![IdentifierDecision {
            document_id: "move-page".into(),
            choice: IdentifierChoice::Rename {
                value: "After-1".into(),
            },
        }];
        sqlx::query("CREATE TRIGGER identifier_source_edit_loss AFTER INSERT ON move_commit_receipts BEGIN UPDATE space_members SET role='viewer' WHERE user_id='mover' AND space_id='move-source'; END").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        let members = rows_raw(&mut conn, "SELECT * FROM space_members", &[])
            .await
            .unwrap();
        drop(conn);
        assert!(relocate(
            &state,
            &super::super::tests::test_auth("mover"),
            &preview.preview_id,
            &req
        )
        .await
        .is_err());
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        assert_eq!(
            rows_raw(&mut conn, "SELECT * FROM space_members", &[])
                .await
                .unwrap(),
            members
        );
        assert!(
            rows_raw(&mut conn, "SELECT * FROM move_commit_receipts", &[])
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            rows_raw(&mut conn, "SELECT * FROM move_metadata_escrow", &[])
                .await
                .unwrap()
                .is_empty()
        );
        drop(conn);
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn changed_maintenance_late_source_edit_loss_with_move_retained_rolls_back_whole_operation(
    ) {
        let (state, source, target, mut req, _) = setup().await;
        sqlx::query("UPDATE documents SET maintenance_user_id='other-owner' WHERE id='move-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO groups(id,name) VALUES('late-identifier-movers','Late identifier movers')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO global_roles(id,name,permissions) VALUES('late-identifier-move','Late identifier move','["view","move_content","manage_folders"]')"#).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('late-identifier-member','late-identifier-movers','mover')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('late-identifier-movers','late-identifier-move')").execute(&state.db).await.unwrap();
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        req.maintenance_decisions = vec![MaintenanceDecision {
            document_id: "move-page".into(),
            choice: MaintenanceChoice::Clear,
        }];
        sqlx::query("CREATE TRIGGER identifier_source_edit_loss AFTER INSERT ON move_commit_receipts BEGIN UPDATE space_members SET role='viewer' WHERE user_id='mover' AND space_id='move-source'; END").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let before = domain_state(&mut conn).await.unwrap();
        let members = rows_raw(&mut conn, "SELECT * FROM space_members", &[])
            .await
            .unwrap();
        drop(conn);
        assert!(relocate(
            &state,
            &super::super::tests::test_auth("mover"),
            &preview.preview_id,
            &req
        )
        .await
        .is_err());
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(domain_state(&mut conn).await.unwrap(), before);
        assert_eq!(
            rows_raw(&mut conn, "SELECT * FROM space_members", &[])
                .await
                .unwrap(),
            members
        );
        assert!(
            rows_raw(&mut conn, "SELECT * FROM move_commit_receipts", &[])
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            rows_raw(&mut conn, "SELECT * FROM move_metadata_escrow", &[])
                .await
                .unwrap()
                .is_empty()
        );
        drop(conn);
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn maintenance_move_maps_or_clears_foreign_team_and_seals_exact_pair_without_timestamp_changes(
    ) {
        for kind in ["clear", "user", "group"] {
            let (state, source, target, mut req, _) = setup().await;
            sqlx::query("INSERT INTO groups(id,name,managed_space_id) VALUES('source-maintainers','Source team','move-source'),('destination-maintainers','Destination team','move-target')").execute(&state.db).await.unwrap();
            sqlx::query("UPDATE documents SET maintenance_group_id='source-maintainers' WHERE id='move-page'").execute(&state.db).await.unwrap();
            let prior:(String,String,String,String)=sqlx::query_as("SELECT updated_at,created_at,created_by,last_updated_by FROM documents WHERE id='move-page'").fetch_one(&state.db).await.unwrap();
            let Json(preview) = create_preview(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path("move-source".into()),
                Json(super::super::tests::request("folder")),
            )
            .await
            .unwrap();
            req.fingerprint = preview.fingerprint.clone();
            let mut conn = state.db.acquire().await.unwrap();
            let before = domain_state(&mut conn).await.unwrap();
            drop(conn);
            assert!(relocate(
                &state,
                &super::super::tests::test_auth("mover"),
                &preview.preview_id,
                &req
            )
            .await
            .is_err());
            let mut conn = state.db.acquire().await.unwrap();
            assert_eq!(domain_state(&mut conn).await.unwrap(), before);
            drop(conn);
            req.maintenance_decisions = vec![MaintenanceDecision {
                document_id: "move-page".into(),
                choice: match kind {
                    "clear" => MaintenanceChoice::Clear,
                    "user" => MaintenanceChoice::User {
                        user_id: "other-owner".into(),
                    },
                    _ => MaintenanceChoice::Group {
                        group_id: "destination-maintainers".into(),
                    },
                },
            }];
            let result = relocate(
                &state,
                &super::super::tests::test_auth("mover"),
                &preview.preview_id,
                &req,
            )
            .await
            .unwrap();
            let op = result["operation_id"].as_str().unwrap();
            let actual:(Option<String>,Option<String>,String,String,String,String)=sqlx::query_as("SELECT maintenance_user_id,maintenance_group_id,updated_at,created_at,created_by,last_updated_by FROM documents WHERE id='move-page'").fetch_one(&state.db).await.unwrap();
            assert_eq!(
                (actual.0, actual.1),
                match kind {
                    "clear" => (None, None),
                    "user" => (Some("other-owner".into()), None),
                    _ => (None, Some("destination-maintainers".into())),
                }
            );
            assert_eq!((actual.2, actual.3, actual.4, actual.5), prior);
            let raw:(String,String)=sqlx::query_as("SELECT original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1 AND family='maintenance' AND document_id='move-page'").bind(op).fetch_one(&state.db).await.unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&raw.0).unwrap()["maintenance_group_id"],
                "source-maintainers"
            );
            crate::storage::relocation::recover(&state.db)
                .await
                .unwrap();
            let (_, Json(ack)) = commit_preview(
                State(state.clone()),
                super::super::tests::test_auth("mover"),
                Path(preview.preview_id),
                Json(req),
            )
            .await
            .unwrap();
            assert_eq!(ack["historical_receipt"], true);
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn maintenance_inactive_existing_person_is_preserved_but_cannot_be_newly_selected() {
        let (state, source, target, mut req, _) = setup().await;
        sqlx::query("UPDATE users SET is_active=0 WHERE id='other-owner'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET maintenance_user_id='other-owner' WHERE id='move-page'")
            .execute(&state.db)
            .await
            .unwrap();
        let Json(preview) = create_preview(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path("move-source".into()),
            Json(super::super::tests::request("folder")),
        )
        .await
        .unwrap();
        req.fingerprint = preview.fingerprint;
        assert_eq!(
            preview.inventory["pages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == "move-page")
                .unwrap()["maintenance_resolution_required"],
            false
        );
        req.maintenance_decisions = vec![MaintenanceDecision {
            document_id: "move-page".into(),
            choice: MaintenanceChoice::User {
                user_id: "other-owner".into(),
            },
        }];
        assert!(relocate(
            &state,
            &super::super::tests::test_auth("mover"),
            &preview.preview_id,
            &req
        )
        .await
        .is_err());
        req.maintenance_decisions.clear();
        let result = relocate(
            &state,
            &super::super::tests::test_auth("mover"),
            &preview.preview_id,
            &req,
        )
        .await
        .unwrap();
        let op = result["operation_id"].as_str().unwrap();
        let actual: Option<String> =
            sqlx::query_scalar("SELECT maintenance_user_id FROM documents WHERE id='move-page'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(actual.as_deref(), Some("other-owner"));
        let Json(evidence) = read_preserved(
            State(state.clone()),
            super::super::tests::test_auth("mover"),
            Path((op.into(), "move-page".into())),
        )
        .await
        .unwrap();
        assert!(!evidence["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["family"] == "maintenance"));
        cleanup(&source, &target).await;
    }
    async fn receipt_populated_setup() -> (AppState,String,String,CommitRequest,String) {
        let (state,source,target,mut req,_) = setup().await;
        for (suffix,page,space) in [("source","move-page","move-source"),("foreign","unrelated-original-page","foreign-original-space")] {
            sqlx::query("INSERT INTO draft_birth_receipts(actor_id,operation_id,space_id,document_id,request_fingerprint,draft_id,result_digest,result_json) VALUES('mover',$1,$2,$3,'original-fingerprint','move-draft','original-digest','{\"original\":true}')")
                .bind(format!("birth-{suffix}")).bind(space).bind(page).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO draft_save_replay_receipts(provider,incoming_id,actor_id,entity_type,entity_id,action,payload_fingerprint,space_id,document_id,draft_id,source_digest,result_digest,created_at,receipt_json) VALUES('fixture',$1,'mover','document',$2,'update_content','original-fingerprint',$3,$2,'move-draft','original-source','original-result','original-stamp','{\"original\":true}')")
                .bind(format!("save-{suffix}")).bind(page).bind(space).execute(&state.db).await.unwrap();
            // Historical, superseded paths deliberately do not match the current published tuple.
            sqlx::query("INSERT INTO publication_receipts(op_id,space_id,document_id,draft_id,actor_id,source_path,source_revision,content_path,revision,sha256,byte_length,payload_fingerprint,manifest_json) VALUES($1,$2,$3,'move-draft','mover','historical-source','historical-source-stamp','historical-published','historical-published-stamp','original-hash',1,'original-fingerprint','{}')")
                .bind(format!("publication-{suffix}")).bind(space).bind(page).execute(&state.db).await.unwrap();
        }
        // Existing setup already populates actual current canonical receipts; add a foreign historical one.
        sqlx::query("INSERT INTO canonical_revision_receipts(op_id,space_id,document_id,actor_id,reason,provider,incoming_id,entity_type,entity_id,action,payload_fingerprint,source_path,source_revision,published_stamp,content_path,revision,sha256,byte_length,manifest_json) VALUES('canonical-foreign','foreign-original-space','unrelated-original-page','mover','replay','fixture','foreign-original','document','unrelated-original-page','update_content','original-fingerprint','historical-source','old','old','historical-published','historical-stamp','original-hash',1,'{}')")
            .execute(&state.db).await.unwrap();
        let Json(preview)=create_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path("move-source".into()),Json(super::super::tests::request("folder"))).await.unwrap();
        assert!(preview.commit_available); req.fingerprint=preview.fingerprint;
        (state,source,target,req,preview.preview_id)
    }
    async fn retained_receipts(state:&AppState)->BTreeMap<String,Vec<Value>> {
        let mut conn=state.db.acquire().await.unwrap(); let all=domain_state(&mut conn).await.unwrap();
        RETAINED_RECEIPT_TABLES.iter().map(|n|((*n).to_string(),table(&all,n).to_vec())).collect()
    }
    #[tokio::test]
    async fn enabled_actual_http_folder_commit_retains_four_raw_receipt_families_and_foreign_evidence() {
        use tower::ServiceExt;
        let (state,source,target,req,id)=receipt_populated_setup().await;
        let before=retained_receipts(&state).await;
        let mut original_conn=state.db.acquire().await.unwrap();
        let original_draft=rows_raw(&mut original_conn,"SELECT * FROM document_drafts WHERE id='move-draft'",&[]).await.unwrap().remove(0);
        let original_digest=super::super::super::draft_save::digest(&original_draft).unwrap(); drop(original_conn);
        let auth=super::super::tests::test_auth("mover");
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&auth.claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let response=crate::api_routes().with_state(state.clone()).oneshot(axum::http::Request::builder().method("POST")
            .uri(format!("/moves/previews/{id}/commit")).header("Authorization",format!("Bearer {token}"))
            .header("Content-Type","application/json").body(axum::body::Body::from(serde_json::to_vec(&req).unwrap())).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        let result:Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),usize::MAX).await.unwrap()).unwrap();
        assert_eq!(result["committed"],true); assert_eq!(retained_receipts(&state).await,before);
        let mut conn=state.db.acquire().await.unwrap();
        let receipt=immutable_receipt(&mut conn,text(&result,"operation_id")).await.unwrap();
        let sealed:Value=serde_json::from_str(text(&receipt,"expected_state_json")).unwrap();
        assert_eq!(sealed["retained_immutable_receipts"],serde_json::to_value(&before).unwrap());
        for id in ["move-page","move-child"] {let space:String=sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1").bind(id).fetch_one(&mut *conn).await.unwrap();assert_eq!(space,"move-target");}
        let moved_draft=rows_raw(&mut conn,"SELECT * FROM document_drafts WHERE id='move-draft'",&[]).await.unwrap().remove(0);
        let moved_digest=super::super::super::draft_save::digest(&moved_draft).unwrap(); assert_ne!(moved_digest,original_digest); drop(conn);
        let get=crate::api_routes().with_state(state.clone()).oneshot(axum::http::Request::builder().uri("/spaces/move-target/documents/move-page/drafts/move-draft").header("Authorization",format!("Bearer {token}")).body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(get.status(),StatusCode::OK);
        let current:Value=serde_json::from_slice(&axum::body::to_bytes(get.into_body(),usize::MAX).await.unwrap()).unwrap();assert_eq!(current["source_digest"],moved_digest);
        let old=crate::api_routes().with_state(state.clone()).oneshot(axum::http::Request::builder().method("PUT").uri("/spaces/move-source/documents/move-page/drafts/move-draft").header("Authorization",format!("Bearer {token}")).header("Content-Type","application/json").body(axum::body::Body::from(json!({"source_digest":original_digest,"content":"OLD PENDING MUST NOT OVERWRITE"}).to_string())).unwrap()).await.unwrap();
        assert!(!old.status().is_success());
        let mut conn=state.db.acquire().await.unwrap();assert_eq!(rows_raw(&mut conn,"SELECT * FROM document_drafts WHERE id='move-draft'",&[]).await.unwrap(),vec![moved_draft]);drop(conn);
        assert_eq!(retained_receipts(&state).await,before);
        cleanup(&source,&target).await;
    }
    #[tokio::test]
    async fn retained_receipt_tamper_after_move_insert_rolls_back_source_foreign_and_phantom_rows() {
        for sql in [
            "UPDATE draft_birth_receipts SET result_json='{}' WHERE operation_id='birth-source'",
            "DELETE FROM draft_save_replay_receipts WHERE incoming_id='save-foreign'",
            "UPDATE publication_receipts SET content_path='wrong' WHERE op_id='publication-source'",
            "UPDATE canonical_revision_receipts SET payload_fingerprint='wrong' WHERE op_id='canonical-foreign'",
            "INSERT INTO draft_birth_receipts SELECT actor_id,'phantom',space_id,document_id,request_fingerprint,draft_id,result_digest,result_json FROM draft_birth_receipts WHERE operation_id='birth-source'",
        ] {
            let (state,source,target,req,id)=receipt_populated_setup().await;
            sqlx::query(&format!("CREATE TRIGGER corrupt_retained_receipt AFTER INSERT ON move_commit_receipts BEGIN {sql}; END")).execute(&state.db).await.unwrap();
            let before=super::super::super::documents::tests::move_database_snapshot(&state).await;
            assert!(commit_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path(id),Json(req)).await.is_err());
            assert_eq!(super::super::super::documents::tests::move_database_snapshot(&state).await,before);
            cleanup(&source,&target).await;
        }
    }
    #[tokio::test]
    async fn cancelled_or_incompatible_saved_preview_never_rebases_or_moves_current_receipts() {
        for cancelled in [false,true] {
            let (state,source,target,mut req,id)=receipt_populated_setup().await;
            let mut conn=state.db.acquire().await.unwrap();let before=domain_state(&mut conn).await.unwrap();drop(conn);
            let files=super::super::super::documents::tests::move_file_snapshot(&source);
            if cancelled {assert_eq!(super::super::cancel_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path(id.clone())).await.unwrap(),StatusCode::NO_CONTENT);}
            else {req.fingerprint="prior-incompatible-preview".into();}
            assert!(commit_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path(id),Json(req)).await.is_err());
            let mut conn=state.db.acquire().await.unwrap();assert_eq!(domain_state(&mut conn).await.unwrap(),before);drop(conn);
            assert_eq!(super::super::super::documents::tests::move_file_snapshot(&source),files);
            cleanup(&source,&target).await;
        }
    }
    #[tokio::test]
    async fn lost_ack_historical_move_after_newer_and_recreated_ids_never_reapplies_receipts_or_body() {
        let (state,source,target,req,id)=receipt_populated_setup().await;
        let before=retained_receipts(&state).await;
        MOVE_COMMIT_OUTCOME.with(|slot|*slot.borrow_mut()=Some("lost_ack"));
        let (_,Json(result))=commit_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path(id.clone()),Json(req.clone())).await.unwrap();
        assert_eq!(result["committed"],true);
        sqlx::query("UPDATE documents SET title='NEWER B',updated_at='newer-b' WHERE id='move-page'").execute(&state.db).await.unwrap();
        sqlx::query("DELETE FROM documents WHERE id='move-child'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('move-child','move-source','RECREATED CHILD','','mover','mover',NULL)").execute(&state.db).await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();let current=domain_state(&mut conn).await.unwrap();drop(conn);
        let (_,Json(ack))=commit_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path(id),Json(req)).await.unwrap();
        assert_eq!(ack["historical_receipt"],true);
        let mut conn=state.db.acquire().await.unwrap();assert_eq!(domain_state(&mut conn).await.unwrap(),current);drop(conn);
        assert_eq!(retained_receipts(&state).await,before);
        cleanup(&source,&target).await;
    }

    #[tokio::test]
    async fn viewer_grant_added_after_review_holds_move_before_receipt_or_file_install() {
        let (state,source,target,req,id)=setup().await;
        sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('move-grant','move-page','move-source','mover','viewer','page','original','mover','created','updated')").execute(&state.db).await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();let before=domain_state(&mut conn).await.unwrap();drop(conn);
        let error=commit_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path(id),Json(req)).await.unwrap_err();
        assert!(matches!(error,AppError::Conflict(_)));
        let mut conn=state.db.acquire().await.unwrap();assert_eq!(domain_state(&mut conn).await.unwrap(),before);
        assert!(rows_raw(&mut conn,"SELECT * FROM move_commit_receipts",&[]).await.unwrap().is_empty());drop(conn);cleanup(&source,&target).await;
    }
    #[tokio::test]
    async fn viewer_grant_preview_hold_creates_no_preview_journal() {
        let (state,source,target,_req,_id)=setup().await;
        sqlx::query("UPDATE space_members SET role='editor' WHERE user_id='mover'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('move-grant','move-page','move-source','mover','viewer','page','original','mover','created','updated')").execute(&state.db).await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();let before=rows_raw(&mut conn,"SELECT * FROM move_previews",&[]).await.unwrap();drop(conn);
        assert!(create_preview(State(state.clone()),super::super::tests::test_auth("mover"),Path("move-source".into()),Json(super::super::tests::request("folder"))).await.is_err());
        let mut conn=state.db.acquire().await.unwrap();assert_eq!(rows_raw(&mut conn,"SELECT * FROM move_previews",&[]).await.unwrap(),before);drop(conn);cleanup(&source,&target).await;
    }

    async fn grant_move_setup() -> (AppState, String, String, CommitRequest, String) {
        let (state, source, target, mut req, _) = setup_with_wal(true).await;
        {
            let mut first = state.db.acquire().await.unwrap(); let mut second = state.db.acquire().await.unwrap();
            for conn in [&mut first, &mut second] {
                sqlx::query("PRAGMA foreign_keys=ON").execute(&mut **conn).await.unwrap();
                let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&mut **conn).await.unwrap(); assert_eq!(enabled, 1);
            }
        }
        for (user, page, grant) in [("move-viewer-one", "move-page", "viewer-preserve"), ("move-viewer-two", "move-child", "viewer-remove")] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(user).bind(format!("{user}@example.test")).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES($1,$2,'move-source',$3,'viewer','page','original-viewer-revision','mover','original-created','original-updated')")
                .bind(grant).bind(page).bind(user).execute(&state.db).await.unwrap();
        }
        let Json(preview) = create_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path("move-source".into()), Json(super::super::tests::request("folder"))).await.unwrap();
        assert_eq!(preview.inventory["viewer_grants"].as_array().unwrap().len(), 2);
        assert!(preview.inventory["viewer_grants"].as_array().unwrap().iter().all(|g| g["preserve_allowed"] == true && g["remove_allowed"] == true));
        req.fingerprint = preview.fingerprint;
        req.viewer_grant_decisions = vec![
            ViewerGrantDecision { grant_id: "viewer-preserve".into(), revision: "original-viewer-revision".into(), choice: ViewerGrantChoice::Preserve },
            ViewerGrantDecision { grant_id: "viewer-remove".into(), revision: "original-viewer-revision".into(), choice: ViewerGrantChoice::Remove { confirm: true } },
        ];
        (state, source, target, req, preview.preview_id)
    }
    async fn grant_complete_snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        super::super::super::documents::tests::move_database_snapshot(state).await
    }
    fn grant_signed_token(state: &AppState, lifetime: i64) -> String {
        let mut claims = super::super::tests::test_auth("mover").claims;
        let now = chrono::Utc::now().timestamp(); claims.iat = now; claims.exp = now + lifetime;
        jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap()
    }
    #[tokio::test]
    async fn grant_move_actual_http_preserves_removes_and_seals_exact_audience_evidence() {
        use tower::ServiceExt;
        let (state, source, target, req, id) = grant_move_setup().await;
        let token = grant_signed_token(&state, 3600);
        let mut conn = state.db.acquire().await.unwrap(); let before = domain_state(&mut conn).await.unwrap(); drop(conn);
        let bodies: Vec<(String, Vec<u8>)> = table(&before, "documents").iter().filter(|p| ["move-page", "move-child"].contains(&text(p, "id")))
            .map(|p| (text(p, "id").into(), std::fs::read(crate::storage::data_dir().join(text(p, "content_path"))).unwrap())).collect();
        let response = router().with_state(state.clone()).oneshot(axum::http::Request::builder().method("POST").uri(format!("/moves/previews/{id}/commit"))
            .header("Authorization", format!("Bearer {token}")).header("Content-Type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&req).unwrap())).unwrap()).await.unwrap();
        let status = response.status(); let result: Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(status, StatusCode::OK, "{result}"); assert_eq!(result["committed"], true);
        let mut conn = state.db.acquire().await.unwrap(); let after = domain_state(&mut conn).await.unwrap();
        let grants = table(&after, "document_viewer_grants"); assert_eq!(grants.len(), 1);
        assert_eq!(grants[0]["id"], "viewer-preserve"); assert_eq!(grants[0]["space_id"], "move-target");
        assert_eq!(grants[0]["user_id"], "move-viewer-one"); assert_eq!(grants[0]["created_by"], "mover"); assert_eq!(grants[0]["created_at"], "original-created");
        assert_ne!(grants[0]["revision"], "original-viewer-revision");
        for family in RETAINED_RECEIPT_TABLES { assert_eq!(table(&before, family), table(&after, family)); }
        assert_eq!(table(&before, "document_workflow_events"), table(&after, "document_workflow_events"));
        for (page, bytes) in bodies {
            let moved = table(&after, "documents").iter().find(|p| p["id"] == page).unwrap();
            assert_eq!(moved["space_id"], "move-target"); assert_eq!(std::fs::read(crate::storage::data_dir().join(text(moved, "content_path"))).unwrap(), bytes);
        }
        let receipt = immutable_receipt(&mut conn, result["operation_id"].as_str().unwrap()).await.unwrap();
        let expected: Value = serde_json::from_str(text(&receipt, "expected_state_json")).unwrap();
        assert_eq!(expected["viewer_grant_decisions"], json!(req.viewer_grant_decisions));
        assert_eq!(expected["viewer_grants_before"], json!(table(&before, "document_viewer_grants"))); assert_eq!(expected["viewer_grants_after"], json!(grants));
        assert_eq!(expected["audit_event"]["actor_id"], "mover");
        let audit_before: Value = serde_json::from_str(expected["audit_event"]["before_value"].as_str().unwrap()).unwrap();
        assert_eq!(audit_before["page_viewer_grants"], json!(table(&before, "document_viewer_grants")));
        crate::storage::relocation::verify_escrow(&mut conn, &receipt).await.unwrap(); drop(conn);
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn grant_move_requires_exact_complete_revision_choices_before_install() {
        for change in 0..5 {
            let (state, source, target, mut req, id) = grant_move_setup().await;
            match change {
                0 => req.viewer_grant_decisions.clear(),
                1 => { req.viewer_grant_decisions.push(req.viewer_grant_decisions[0].clone()); },
                2 => req.viewer_grant_decisions[0].revision = "foreign-revision".into(),
                3 => req.viewer_grant_decisions[0].grant_id = "foreign-grant".into(),
                _ => req.viewer_grant_decisions[1].choice = ViewerGrantChoice::Remove { confirm: false },
            }
            let before = grant_complete_snapshot(&state).await;
            let files = super::super::super::documents::tests::move_file_snapshot(&source);
            assert!(commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(id), Json(req)).await.is_err());
            assert_eq!(grant_complete_snapshot(&state).await, before); assert_eq!(super::super::super::documents::tests::move_file_snapshot(&source), files);
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn grant_move_management_is_not_implied_by_move_and_remove_needs_no_recipient_authority() {
        let (state, source, target, _, _) = grant_move_setup().await;
        sqlx::query("UPDATE space_members SET role='editor' WHERE user_id='mover'").execute(&state.db).await.unwrap();
        let before = grant_complete_snapshot(&state).await;
        assert!(create_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path("move-source".into()), Json(super::super::tests::request("folder"))).await.is_err());
        assert_eq!(grant_complete_snapshot(&state).await, before); cleanup(&source, &target).await;
        let (state, source, target, mut req, _) = grant_move_setup().await;
        sqlx::query("UPDATE space_members SET role='editor' WHERE user_id='mover' AND space_id='move-target'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='move-viewer-one'").execute(&state.db).await.unwrap();
        let Json(preview) = create_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path("move-source".into()), Json(super::super::tests::request("folder"))).await.unwrap();
        assert!(preview.inventory["viewer_grants"].as_array().unwrap().iter().all(|g| g["preserve_allowed"] == false && g["remove_allowed"] == true));
        assert!(preview.inventory["viewer_grants"].as_array().unwrap().iter().any(|g| g["recipient"]["available"] == false && g["recipient"]["display_name"].is_null()));
        req.fingerprint = preview.fingerprint;
        for decision in &mut req.viewer_grant_decisions { decision.choice = ViewerGrantChoice::Remove { confirm: true }; }
        let (_, Json(result)) = commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(preview.preview_id), Json(req)).await.unwrap();
        assert_eq!(result["committed"], true);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_viewer_grants").fetch_one(&state.db).await.unwrap(); assert_eq!(count, 0); cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn grant_move_changed_grant_recipient_or_destination_policy_retains_original_state() {
        for sql in ["UPDATE document_viewer_grants SET revision='later' WHERE id='viewer-preserve'", "DELETE FROM document_viewer_grants WHERE id='viewer-remove'", "INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('later-grant','move-child','move-source','other-owner','viewer','page','later','mover','now','now')", "UPDATE users SET is_active=0 WHERE id='move-viewer-one'", "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('move-target','move-viewer-one','mover')", "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('move-page','move-source','restricted','late-source-policy','mover')"] {
            let (state, source, target, req, id) = grant_move_setup().await;
            sqlx::query(sql).execute(&state.db).await.unwrap(); let before = grant_complete_snapshot(&state).await;
            let files = super::super::super::documents::tests::move_file_snapshot(&source);
            assert!(commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(id), Json(req)).await.is_err(), "{sql}");
            assert_eq!(grant_complete_snapshot(&state).await, before); assert_eq!(super::super::super::documents::tests::move_file_snapshot(&source), files); cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn grant_move_rechecks_revision_after_actual_two_connection_wal_guard_wait() {
        let (state, source, target, req, id) = grant_move_setup().await;
        let mut conn = state.db.acquire().await.unwrap();
        let mut expected = domain_state(&mut conn).await.unwrap(); drop(conn);
        expected.get_mut("document_viewer_grants").unwrap().iter_mut().find(|g| g["id"] == "viewer-preserve").unwrap()["revision"] = json!("after-real-wait");
        let source_files = super::super::super::documents::tests::move_file_snapshot(&source);
        let mut held = state.db.begin().await.unwrap();
        sqlx::query("UPDATE document_viewer_grants SET revision=revision WHERE id='viewer-preserve'").execute(&mut *held).await.unwrap();
        let reached = std::sync::Arc::new(tokio::sync::Notify::new()); let signal = reached.clone(); let clone = state.clone();
        let task = tokio::spawn(async move {
            MOVE_BEFORE_GUARD.with(|slot| *slot.borrow_mut() = Some(signal));
            relocate(&clone, &super::super::tests::test_auth("mover"), &id, &req).await
        });
        reached.notified().await; assert!(!task.is_finished());
        sqlx::query("UPDATE document_viewer_grants SET revision='after-real-wait' WHERE id='viewer-preserve'").execute(&mut *held).await.unwrap(); held.commit().await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), task).await.unwrap().unwrap(); assert!(result.is_err());
        let revision: String = sqlx::query_scalar("SELECT revision FROM document_viewer_grants WHERE id='viewer-preserve'").fetch_one(&state.db).await.unwrap(); assert_eq!(revision, "after-real-wait");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM move_commit_receipts").fetch_one(&state.db).await.unwrap(); assert_eq!(count, 0);
        let mut conn = state.db.acquire().await.unwrap(); assert_eq!(domain_state(&mut conn).await.unwrap(), expected); drop(conn);
        assert_eq!(super::super::super::documents::tests::move_file_snapshot(&source), source_files);
        cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn grant_move_late_receipt_audit_and_recipient_faults_roll_back_full_database() {
        for trigger in ["CREATE TRIGGER grant_move_fault AFTER INSERT ON move_commit_receipts BEGIN INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('move-target','move-viewer-one','mover'); END", "CREATE TRIGGER grant_move_fault AFTER INSERT ON audit_log WHEN NEW.action='folder.move' BEGIN UPDATE users SET is_active=0 WHERE id='move-viewer-one'; END", "CREATE TRIGGER grant_move_fault BEFORE INSERT ON audit_log WHEN NEW.action='folder.move' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END", "CREATE TRIGGER grant_move_fault AFTER INSERT ON document_viewer_grants WHEN NEW.space_id='move-target' BEGIN UPDATE documents SET title='injected' WHERE id='move-page'; END"] {
            let (state, source, target, req, id) = grant_move_setup().await;
            sqlx::query(trigger).execute(&state.db).await.unwrap(); let before = grant_complete_snapshot(&state).await;
            let files = super::super::super::documents::tests::move_file_snapshot(&source);
            assert!(commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(id), Json(req)).await.is_err());
            assert_eq!(grant_complete_snapshot(&state).await, before); assert_eq!(super::super::super::documents::tests::move_file_snapshot(&source), files); cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn grant_move_final_original_signed_write_expiry_rolls_back_after_receipt_waits() {
        use axum::extract::FromRequestParts;
        let (state, source, target, req, id) = grant_move_setup().await;
        let token = grant_signed_token(&state, 2);
        let (mut parts, _) = axum::http::Request::builder().method("POST").uri("/fixture").header("Authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty()).unwrap().into_parts();
        let auth = AuthUser::from_request_parts(&mut parts, &state).await.unwrap();
        let before = grant_complete_snapshot(&state).await;
        let reached = std::sync::Arc::new(tokio::sync::Notify::new()); let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let gates = (reached.clone(), release.clone()); let clone = state.clone();
        let task = tokio::spawn(AFTER_VIEWER_GRANT_FINAL.scope(gates, async move { relocate(&clone, &auth, &id, &req).await }));
        reached.notified().await; tokio::time::sleep(std::time::Duration::from_secs(3)).await; release.notify_one(); assert!(task.await.unwrap().is_err());
        assert_eq!(grant_complete_snapshot(&state).await, before); cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn grant_move_lost_ack_history_never_restores_later_revoked_grants_or_newer_body() {
        let (state, source, target, req, id) = grant_move_setup().await;
        MOVE_COMMIT_OUTCOME.with(|slot| *slot.borrow_mut() = Some("lost_ack"));
        let (_, Json(result)) = commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(id.clone()), Json(req.clone())).await.unwrap();
        assert_eq!(result["committed"], true);
        sqlx::query("DELETE FROM document_viewer_grants WHERE id='viewer-preserve'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET title='NEWER B',updated_at='newer-b' WHERE id='move-page'").execute(&state.db).await.unwrap();
        let before = grant_complete_snapshot(&state).await;
        let (_, Json(ack)) = commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(id), Json(req)).await.unwrap();
        assert_eq!(ack["historical_receipt"], true); assert_eq!(grant_complete_snapshot(&state).await, before); cleanup(&source, &target).await;
    }
    #[tokio::test]
    async fn grant_move_properties_projects_only_current_manager_entitled_recipient_fields() {
        let (state, source, target, req, id) = grant_move_setup().await;
        let (_, Json(result)) = commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(id), Json(req)).await.unwrap();
        let op = result["operation_id"].as_str().unwrap().to_owned();
        let Json(evidence) = read_preserved(State(state.clone()), super::super::tests::test_auth("mover"), Path((op.clone(), "move-page".into()))).await.unwrap();
        let entry = evidence["entries"].as_array().unwrap().iter().find(|e| e["family"] == "page_viewer_grant").unwrap();
        assert_eq!(entry["original"]["grant_id"], "viewer-preserve"); assert_eq!(entry["expected_active"]["space_id"], "move-target");
        assert_eq!(entry["original"].as_object().unwrap().len(), 7); assert!(entry["original"].get("user_id").is_none()); assert!(entry["original"].get("created_by").is_none());
        sqlx::query("UPDATE space_members SET role='editor' WHERE user_id='mover' AND space_id='move-target'").execute(&state.db).await.unwrap();
        let Json(ordinary) = read_preserved(State(state.clone()), super::super::tests::test_auth("mover"), Path((op, "move-page".into()))).await.unwrap();
        assert!(ordinary["entries"].as_array().unwrap().iter().all(|e| e["family"] != "page_viewer_grant"));
        assert!(ordinary["entries"].as_array().unwrap().iter().any(|e| e["family"] == "tag")); cleanup(&source, &target).await;
    }

    #[tokio::test]
    async fn grant_move_destination_ancestor_policy_is_bound_before_install() {
        for changed in [false, true] {
            let (state, source, target, mut req, _) = grant_move_setup().await;
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('grant-target-parent','move-target','Parent','','mover','mover','published-parent')")
                .execute(&state.db).await.unwrap();
            let mut intent = super::super::tests::request("document");
            intent.destination.parent_id = Some("grant-target-parent".into());
            let Json(preview) = create_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path("move-source".into()), Json(intent)).await.unwrap();
            assert!(preview.inventory["viewer_grants"].as_array().unwrap().iter().all(|g| g["preserve_allowed"] == true));
            req.fingerprint = preview.fingerprint;
            if changed {
                sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('grant-target-parent','move-target','restricted','late-parent-policy','mover')")
                    .execute(&state.db).await.unwrap();
            }
            let before = grant_complete_snapshot(&state).await;
            let files = super::super::super::documents::tests::move_file_snapshot(&source);
            let result = commit_preview(State(state.clone()), super::super::tests::test_auth("mover"), Path(preview.preview_id), Json(req)).await;
            if changed {
                assert!(result.is_err()); assert_eq!(grant_complete_snapshot(&state).await, before);
                assert_eq!(super::super::super::documents::tests::move_file_snapshot(&source), files);
            } else {
                assert_eq!(result.unwrap().1.0["committed"], true);
                let parent: Option<String> = sqlx::query_scalar("SELECT parent_id FROM documents WHERE id='move-page'").fetch_one(&state.db).await.unwrap();
                assert_eq!(parent.as_deref(), Some("grant-target-parent"));
            }
            cleanup(&source, &target).await;
        }
    }
    #[tokio::test]
    async fn grant_move_actual_write_only_key_and_late_revocation_use_original_method_proof() {
        use axum::extract::FromRequestParts;
        for revoked in [false, true] {
            let (state, source, target, req, id) = grant_move_setup().await;
            let key = "grant-move-write-only";
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'mover',$1,$2,$1,'[\"write\"]')")
                .bind(key).bind(crate::auth::api_keys::hash_api_key(key)).execute(&state.db).await.unwrap();
            let (mut parts, _) = axum::http::Request::builder().method("POST").uri("/fixture")
                .header("X-API-Key", key).body(axum::body::Body::empty()).unwrap().into_parts();
            let auth = AuthUser::from_request_parts(&mut parts, &state).await.unwrap();
            assert_eq!(auth.api_key_scopes.as_ref().unwrap(), &vec!["write".to_owned()]);
            if revoked {
                sqlx::query("CREATE TRIGGER grant_move_key_fault AFTER INSERT ON move_commit_receipts BEGIN DELETE FROM api_keys WHERE id='grant-move-write-only'; END")
                    .execute(&state.db).await.unwrap();
            }
            let before = grant_complete_snapshot(&state).await;
            let files = super::super::super::documents::tests::move_file_snapshot(&source);
            let result = commit_preview(State(state.clone()), auth, Path(id), Json(req)).await;
            if revoked {
                assert!(result.is_err()); assert_eq!(grant_complete_snapshot(&state).await, before);
                assert_eq!(super::super::super::documents::tests::move_file_snapshot(&source), files);
            } else { assert_eq!(result.unwrap().1.0["committed"], true); }
            cleanup(&source, &target).await;
        }
    }

}
