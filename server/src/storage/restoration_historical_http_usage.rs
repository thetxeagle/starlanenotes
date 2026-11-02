// One physical observation, borrowed by finite HTTP requests. No reacquisition
// of native family/capsule locks and no accounting, write or cleanup authority.
struct HistoricalRequestGate { closing: bool, active: usize }
struct HistoricalHttpUsageOwner {
    physical: tokio::sync::Mutex<HistoricalFinalizedObservation<'static>>,
    pool: sqlx::AnyPool,
    source: Arc<crate::storage::recovery_records::PoolSource>,
    state: Arc<HistoricalReadonlyReaderState>,
    custody: Arc<HistoricalReadonlyChildCustody>,
    gate: std::sync::Mutex<HistoricalRequestGate>,
    drained: tokio::sync::Notify,
    #[cfg(test)] finish_validated: std::sync::atomic::AtomicBool,
}
struct HistoricalRequestLease { owner: Arc<HistoricalHttpUsageOwner> }
impl Drop for HistoricalRequestLease {
    fn drop(&mut self) {
        if let Ok(mut gate) = self.owner.gate.lock() {
            // Admission increments under this same lock; a zero count here is
            // an invariant failure, never successful drain evidence.
            if let Some(count) = gate.active.checked_sub(1) {
                gate.active = count;
                if count == 0 { self.owner.drained.notify_waiters(); }
            } else { gate.closing = true; }
        }
    }
}
struct HistoricalReadonlyAuthBinding {
    key: std::sync::Weak<sqlx::any::AnyConnectOptions>,
    owner: std::sync::Weak<HistoricalHttpUsageOwner>,
    identity: Arc<()>,
}
fn historical_auth_bindings() -> &'static std::sync::Mutex<Vec<HistoricalReadonlyAuthBinding>> {
    static B: std::sync::OnceLock<std::sync::Mutex<Vec<HistoricalReadonlyAuthBinding>>> = std::sync::OnceLock::new();
    B.get_or_init(Default::default)
}
struct HistoricalAuthRegistration { key: Arc<sqlx::any::AnyConnectOptions>, identity: Arc<()>, owner: std::sync::Weak<HistoricalHttpUsageOwner>, nominal: bool }
struct HistoricalClosingRegistration { identity: Arc<()> }
impl HistoricalAuthRegistration {
    fn stop_and_unregister(mut self) -> io::Result<HistoricalClosingRegistration> {
        let owner = self.owner.upgrade().ok_or_else(|| fail("Historical registered owner unavailable"))?;
        owner.stop()?;
        let mut bindings = historical_auth_bindings().lock().map_err(|_| fail("Historical auth registry poisoned"))?;
        let binding = bindings.iter_mut().find(|b| Arc::ptr_eq(&b.identity, &self.identity)
            && b.key.upgrade().is_some_and(|key| Arc::ptr_eq(&key, &self.key)))
            .ok_or_else(|| fail("Historical exact registration absent"))?;
        if !std::sync::Weak::ptr_eq(&binding.owner, &self.owner) { return Err(fail("Historical registration owner changed")); }
        // Tombstone the exact live pool until its key dies. A late lookup must
        // fail closed, never fall through to normal session telemetry.
        binding.owner = std::sync::Weak::new();
        self.nominal = true;
        Ok(HistoricalClosingRegistration { identity: self.identity.clone() })
    }
}
impl Drop for HistoricalAuthRegistration {
    fn drop(&mut self) {
        if !self.nominal {
            if let Some(owner) = self.owner.upgrade() { let _ = owner.stop(); }
            // On an error the existing weak binding remains recognizable.
            // Drop cannot mint a successful closing token or native close ACK.
        }
    }
}
pub(crate) struct HistoricalReadonlyAuthObservation { _lease: HistoricalRequestLease }
pub(crate) fn historical_readonly_auth_observation(pool: &sqlx::AnyPool) -> io::Result<Option<HistoricalReadonlyAuthObservation>> {
    let key = pool.connect_options();
    let matched = {
        let bindings = historical_auth_bindings().lock().map_err(|_| fail("Historical auth registry poisoned"))?;
        bindings.iter().find(|b| b.key.upgrade().is_some_and(|prior| Arc::ptr_eq(&prior, &key)))
            .map(|binding| binding.owner.clone())
    };
    let Some(owner) = matched else { return Ok(None); };
    let owner = owner.upgrade().ok_or_else(|| fail("Historical known pool is closed"))?;
    let lease = owner.enter_request()?;
    owner.check_pool(pool)?;
    Ok(Some(HistoricalReadonlyAuthObservation { _lease: lease }))
}
impl HistoricalHttpUsageOwner {
    fn from_held(physical: HistoricalFinalizedObservation<'static>, reader: &HistoricalReadonlySealedReader,
        custody: Arc<HistoricalReadonlyChildCustody>) -> io::Result<Arc<Self>> {
        let state = reader.vfs.as_ref().ok_or_else(|| fail("Historical actual VFS absent"))?.state().clone();
        if !matches!(&physical.pager, HistoricalPhysicalPager::ActualReadonly511(original) if Arc::ptr_eq(original, &state)) {
            return Err(fail("Historical physical object belongs to another actual reader"));
        }
        if !matches!(&state.authority, HistoricalReadonlyReaderAuthority::Child(original) if Arc::ptr_eq(original, &custody)) {
            return Err(fail("Historical custody belongs to another actual reader"));
        }
        let source = reader.source.as_ref().ok_or_else(|| fail("Historical initialized pool source absent"))?.clone();
        physical.fence_physical()?;
        let owner = Arc::new(Self { physical: tokio::sync::Mutex::new(physical), pool: reader.pool.clone(), source, state, custody,
            gate: std::sync::Mutex::new(HistoricalRequestGate { closing: false, active: 0 }), drained: Default::default(), #[cfg(test)] finish_validated: std::sync::atomic::AtomicBool::new(false) });
        owner.check_pool(&reader.pool)?;
        Ok(owner)
    }
    fn check_pool(&self, pool: &sqlx::AnyPool) -> io::Result<()> {
        if !Arc::ptr_eq(&pool.connect_options(), &self.pool.connect_options()) { return Err(fail("Historical request pool differs")); }
        self.state.check()?;
        let source = crate::storage::recovery_records::pool_source(pool, self.state.authority.path())
            .ok_or_else(|| fail("Historical initialized pool source changed"))?;
        if !Arc::ptr_eq(&source, &self.source) { return Err(fail("Historical initialized source differs")); }
        self.custody.fence()
    }
    fn register(self: &Arc<Self>) -> io::Result<HistoricalAuthRegistration> {
        self.check_pool(&self.pool)?;
        let key = self.pool.connect_options();
        let identity = Arc::new(());
        let mut bindings = historical_auth_bindings().lock().map_err(|_| fail("Historical auth registry poisoned"))?;
        bindings.retain(|binding| binding.key.strong_count() > 0);
        if bindings.iter().any(|b| b.key.upgrade().is_some_and(|prior| Arc::ptr_eq(&prior, &key))) {
            return Err(fail("Historical actual pool already registered"));
        }
        bindings.push(HistoricalReadonlyAuthBinding { key: Arc::downgrade(&key), owner: Arc::downgrade(self), identity: identity.clone() });
        Ok(HistoricalAuthRegistration { key, identity, owner: Arc::downgrade(self), nominal: false })
    }
    fn enter_request(self: &Arc<Self>) -> io::Result<HistoricalRequestLease> {
        let mut gate = self.gate.lock().map_err(|_| fail("Historical request gate poisoned"))?;
        if gate.closing { return Err(fail("Historical request admission closed")); }
        gate.active = gate.active.checked_add(1).ok_or_else(|| fail("Historical request count overflow"))?;
        Ok(HistoricalRequestLease { owner: self.clone() })
    }
    fn stop(&self) -> io::Result<()> {
        self.gate.lock().map_err(|_| fail("Historical request gate poisoned"))?.closing = true;
        Ok(())
    }
    async fn drain_and_recover(self: Arc<Self>, closing: HistoricalClosingRegistration) -> io::Result<HistoricalFinalizedObservation<'static>> {
        {
            let bindings = historical_auth_bindings().lock().map_err(|_| fail("Historical auth registry poisoned"))?;
            if !bindings.iter().any(|b| Arc::ptr_eq(&b.identity, &closing.identity)
                && b.key.upgrade().is_some_and(|key| Arc::ptr_eq(&key, &self.pool.connect_options()))
                && b.owner.upgrade().is_none()) { return Err(fail("Historical exact closed registration absent")); }
        }
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let notified = self.drained.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                {
                    let gate = self.gate.lock().map_err(|_| fail("Historical request gate poisoned"))?;
                    if !gate.closing { return Err(fail("Historical drain before admission closed")); }
                    if gate.active == 0 { return Ok(()); }
                }
                notified.await;
            }
        }).await.map_err(|_| fail("Historical active requests did not drain"))??;
        // Axum 0.8.8 notifies close_rx before its connection future/service is
        // dropped (serve/mod.rs:414). Active request zero therefore precedes
        // the last framework-owned service drop. Wait for structural recovery,
        // retaining the SAME object throughout; counts never authorize a take.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut retained = self;
        loop {
            match Arc::try_unwrap(retained) {
                Ok(owner) => return Ok(owner.physical.into_inner()),
                Err(owner) => {
                    retained = owner;
                    if std::time::Instant::now() >= deadline {
                        return Err(fail("Historical live service owner did not retire after request drain"));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }
            }
        }
    }
}


// Only the admitted worker can install this extension. It carries an actual
// request lease, never a path, flag, caller-supplied total or serialized grant.
#[derive(Clone)]
pub(crate) struct HistoricalHealthRequest { lease: Arc<HistoricalRequestLease> }
struct HistoricalHealthFinalizer {
    lease: Arc<HistoricalRequestLease>,
    auth: crate::auth::middleware::AdminUser,
    used: std::sync::atomic::AtomicBool,
}
#[derive(Debug, serde::Serialize)]
pub(crate) struct ReadonlyKnownUsageObservation {
    status: &'static str,
    scope: &'static str,
    observed_at: String,
    logical_retained_floor_bytes: String,
    original_pending_maximum_bytes: String,
    conservative_known_sum_bytes: String,
    reference_semantics: &'static str,
    write_admission: &'static str,
}
pub(crate) struct BorrowedHistoricalHealthPurpose<'a> {
    owner: &'a HistoricalHttpUsageOwner,
    facts: &'a HistoricalUsageFacts,
}
impl BorrowedHistoricalHealthPurpose<'_> {
    pub(crate) fn check_pool(&self, pool: &sqlx::AnyPool) -> io::Result<()> { self.owner.check_pool(pool) }
    pub(crate) fn observation(&self) -> io::Result<ReadonlyKnownUsageObservation> {
        let expected = historical_checked_usage(self.facts.immutable_logical_charge_bytes, self.facts.pending_maximum_bytes)?;
        if &expected != self.facts { return Err(fail("Historical held known usage changed")); }
        Ok(ReadonlyKnownUsageObservation {
            status: "observed", scope: "restored_sqlite_historical_logical_and_reserved_v1",
            observed_at: chrono::Utc::now().to_rfc3339(),
            logical_retained_floor_bytes: expected.immutable_logical_charge_bytes.to_string(),
            original_pending_maximum_bytes: expected.pending_maximum_bytes.to_string(),
            conservative_known_sum_bytes: expected.conservative_total_bytes.to_string(),
            reference_semantics: "not_established", write_admission: "unavailable",
        })
    }
}
impl HistoricalHttpUsageOwner {
    async fn current_admin(&self, auth: &crate::auth::middleware::AdminUser) -> Result<(), crate::auth::routes::AppError> {
        use crate::auth::{middleware::{require_current_admin, CurrentAdminAuthority}, routes::AppError};
        self.check_pool(&self.pool).map_err(AppError::internal)?;
        if auth.api_key_scopes.is_some() || auth.claims.sub != self.custody.wire.human
            || auth.claims.sid != self.custody.wire.session {
            return Err(AppError::Unauthorized("Original historical administrator session unavailable".into()));
        }
        let mut conn = self.pool.acquire().await.map_err(AppError::internal)?;
        match require_current_admin(&mut conn, auth).await? {
            CurrentAdminAuthority::HumanInstanceAdmin { user_id } if user_id == self.custody.wire.human => Ok(()),
            _ => Err(AppError::Unauthorized("Original historical administrator unavailable".into())),
        }
        // require_current_admin checks original JWT expiry after its final SQL wait.
        // No later SQL/native await belongs to this final authority decision.
    }
}
impl HistoricalHealthRequest {
    pub(crate) async fn observe(&self, state: &crate::AppState, auth: &crate::auth::middleware::AdminUser)
        -> Result<axum::response::Response, crate::auth::routes::AppError> {
        use crate::auth::routes::AppError;
        let owner = &self.lease.owner;
        owner.check_pool(&state.db).map_err(AppError::internal)?;
        // Serialize use of the SAME physical observation. Never reacquire its
        // family/capsule exclusion, and hold no SQL connection over health probes.
        let physical = owner.physical.lock().await;
        owner.current_admin(auth).await?;
        let facts = {
            let mut conn = owner.pool.acquire().await.map_err(AppError::internal)?;
            let observed = HistoricalUsageObservation::hold_in(&physical, &mut conn).await.map_err(AppError::internal)?;
            observed.facts.clone()
        };
        let purpose = BorrowedHistoricalHealthPurpose { owner, facts: &facts };
        let mut report = crate::storage::health::observe_borrowed_historical(state, &purpose).await.map_err(AppError::internal)?;
        {
            let mut conn = owner.pool.acquire().await.map_err(AppError::internal)?;
            let observed = HistoricalUsageObservation::hold_in(&physical, &mut conn).await.map_err(AppError::internal)?;
            if observed.facts != facts { return Err(AppError::internal("Historical scoped usage changed across probes")); }
        }
        crate::storage::health::finalize_borrowed_historical(&mut report, state, &purpose).map_err(AppError::internal)?;
        owner.current_admin(auth).await?;
        let body = serde_json::to_vec(&report).map_err(AppError::internal)?;
        if body.len() > 1024 * 1024 { return Err(AppError::internal("Historical health report exceeds bounded response")); }
        let mut response = axum::http::Response::builder()
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .header(axum::http::header::CACHE_CONTROL, "no-store")
            .body(axum::body::Body::from(body)).map_err(AppError::internal)?;
        response.extensions_mut().insert(Arc::new(HistoricalHealthFinalizer {
            lease: self.lease.clone(),
            auth: crate::auth::middleware::AdminUser { claims: auth.claims.clone(), api_key_scopes: auth.api_key_scopes.clone() },
            used: std::sync::atomic::AtomicBool::new(false),
        }));
        Ok(response)
    }
}
impl HistoricalHealthFinalizer {
    async fn finish(&self, lease: &Arc<HistoricalRequestLease>) -> Result<(), crate::auth::routes::AppError> {
        use crate::auth::routes::AppError;
        if !Arc::ptr_eq(&self.lease, lease) || self.used.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return Err(AppError::Unauthorized("Historical health response finalizer unavailable".into()));
        }
        // Called AFTER the outer complete native/full81 fence. Only this final
        // current original human/SID/expiry decision follows that expensive wait.
        self.lease.owner.current_admin(&self.auth).await
    }
}


#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalKnownUsageExpectations { logical: i64, pending: i64, known_sum: i64 }
impl HistoricalKnownUsageExpectations {
    fn valid(&self) -> bool { self.logical >= 0 && self.pending >= 0 && self.logical.checked_add(self.pending) == Some(self.known_sum) }
}
fn historical_known_usage_nonnull<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<HistoricalKnownUsageExpectations>, D::Error> {
    HistoricalKnownUsageExpectations::deserialize(d).map(Some)
}
async fn historical_actual_known_usage(source: &mut StoppedHistoricalVerificationSource<'_>, handoff: &HistoricalReadonlyChildHandoff,
    client: &reqwest::Client, base: &str, token: &str, expected: &HistoricalKnownUsageExpectations) -> anyhow::Result<()> {
    source.fence().await?; handoff.fence()?;
    let clock = std::time::Instant::now();
    let response = client.get(format!("{base}/api/v1/admin/storage-health")).bearer_auth(token).send().await?;
    anyhow::ensure!(response.status() == reqwest::StatusCode::OK, "Historical known usage registered request refused status={}", response.status());
    let raw = response.bytes().await?;
    let request_ms = clock.elapsed().as_millis();
    anyhow::ensure!(raw.len() <= 1024 * 1024, "Historical health response exceeds finite bound");
    let report: serde_json::Value = serde_json::from_slice(&raw)?;
    source.fence().await?; handoff.fence()?;
    let capacity = &report["attachment_capacity"];
    anyhow::ensure!(capacity["accounting_status"] == "unavailable", "Historical scoped usage became generic accounting");
    for name in ["logical_retained_bytes", "charged_retained_bytes", "reserved_maximum_bytes", "charged_total_bytes"] {
        anyhow::ensure!(capacity.get(name).is_some_and(serde_json::Value::is_null), "Historical generic charge must remain explicit null");
    }
    let known = &capacity["readonly_known_usage"];
    anyhow::ensure!(known.as_object().is_some_and(|o| o.len() == 8)
        && known["status"] == "observed" && known["scope"] == "restored_sqlite_historical_logical_and_reserved_v1"
        && known["reference_semantics"] == "not_established" && known["write_admission"] == "unavailable"
        && known["logical_retained_floor_bytes"] == expected.logical.to_string()
        && known["original_pending_maximum_bytes"] == expected.pending.to_string()
        && known["conservative_known_sum_bytes"] == expected.known_sum.to_string()
        && known["observed_at"].as_str().is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok()),
        "Actual historical conservative known observation differs");
    for request in [client.get(format!("{base}/api/v1/admin/storage-health")),
        client.get(format!("{base}/api/v1/admin/storage-health")).header("X-API-Key", "untrusted-no-authority"),
        client.get(format!("{base}/api/v1/admin/storage-health?unexpected=1")).bearer_auth(token)] {
        let response = request.send().await?;
        anyhow::ensure!(matches!(response.status().as_u16(), 401|403|503), "Historical known usage denied request succeeded");
        source.fence().await?; handoff.fence()?;
    }
    println!("ACTUAL_NORMAL_HISTORICAL_REGISTERED_SCOPED_KNOWN_USAGE_CURRENT_ORIGINAL_SESSION_GENERIC_NULL_LIMITS_INDEPENDENT_ANONYMOUS_KEY_QUERY_DENIAL_PASS logical={} pending={} known_sum={} http_elapsed_ms={request_ms}", expected.logical, expected.pending, expected.known_sum);
    Ok(())
}
