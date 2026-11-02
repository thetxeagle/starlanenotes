// Opaque live original source retained across the dedicated verification journey.
// Captured bundle rows or copied receipts cannot construct this source authority.
pub(crate) struct StoppedVerificationSource<'a> {
    root: &'a Root,
    database: &'a File,
    database_stamp: PhysicalStamp,
    inventory: &'a SqliteCaptureInventory,
    custody: &'a SqliteOriginalSourceCustody,
    rows: &'a [admission::CapturedReservation],
    prior: Option<&'a PriorRestoredCaptureLeases>,
    preservation: Option<&'a FinalizedAdmissionPreservationLeases>,
    native: Option<&'a admission::capture::NativeCaptureLeases>,
    certificate: Option<&'a admission::ClassifiedAdmissionSource>,
    completed: Option<&'a Arc<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture>>,
    prior_completed: Option<&'a FinalizedCompletedCapture>,
    mixed_completed: Option<&'a MixedCompletedCapture>,
    composite: Option<&'a crate::storage::attachment_upload::admission::committed_erasure::accounting::composite::CompositeOriginalSource>,
    historical:Option<&'a HistoricalCompositeSource>,
    conn: &'a mut sqlx::AnyConnection,
    admin: Option<&'a crate::auth::middleware::AdminUser>,
}
async fn verification_source_human(conn:&mut sqlx::AnyConnection,admin:&crate::auth::middleware::AdminUser)->anyhow::Result<String> {
    anyhow::ensure!(admin.api_key_scopes.is_none(),"Verification producer requires the original human bearer session");
    match crate::auth::middleware::require_current_admin(conn,admin).await
        .map_err(|_|fail(if admin.claims.exp<=chrono::Utc::now().timestamp(){"Original verification source administrator unavailable (bearer expired at refusal)"}else{"Original verification source administrator unavailable"}))? {
        crate::auth::middleware::CurrentAdminAuthority::HumanInstanceAdmin{user_id}=>Ok(user_id),
        _=>Err(fail("Verification producer requires original current human authority").into()),
    }
}
impl StoppedVerificationSource<'_> {
    pub(crate) fn path(&self)->&Path {&self.root.absolute}
    pub(crate) fn root_identity(&self)->io::Result<Identity> {identity(self.root.fd())}
    pub(crate) async fn current_human(&mut self)->anyhow::Result<String> {
        let admin=self.admin.ok_or_else(||fail("Dedicated verification authority absent"))?;
        self.custody.fence(self.root)?;
        let human=verification_source_human(self.conn,admin).await?;
        self.custody.fence(self.root)?;Ok(human)
    }
    pub(crate) async fn fence(&mut self)->anyhow::Result<()> {
        self.custody.fence(self.root)?;
        self.root.check()?;
        anyhow::ensure!(physical_stamp(self.database)?==self.database_stamp
            &&physical_stamp(&self.root.leaf("starlane-notes.db")?)?==self.database_stamp,
            "Original verification source database exchanged");
        if let Some(a)=self.admin {verification_source_human(self.conn,a).await?;}
        if let Some(p)=self.prior {p.recheck_in(self.conn).await?;}
        if let Some(p)=self.preservation {p.recheck_in(self.conn).await?;}
        if let Some(n)=self.native {n.recheck(self.conn).await?;}
        if let Some(c)=self.certificate {c.fence()?;}
        if let Some(c)=self.completed {c.fence_in(self.conn).await?;}
        if let Some(c)=self.prior_completed {c.recheck_in(self.conn).await?;}
        if let Some(c)=self.mixed_completed {c.recheck_in(self.conn).await?;}
        if let Some(c)=self.composite {c.recheck_in(self.conn).await?;}
        if let Some(c)=self.historical {c.recheck_in(self.conn).await?;}
        anyhow::ensure!(prior_capture_rows(self.conn).await?==self.rows,"Original verification source reservation rows changed");
        self.inventory.fence(self.root)?;
        // Final authority follows every SQL wait. No native fence mints a role.
        if let Some(a)=self.admin {verification_source_human(self.conn,a).await?;}
        self.root.check()?;
        anyhow::ensure!(physical_stamp(self.database)?==self.database_stamp
            &&physical_stamp(&self.root.leaf("starlane-notes.db")?)?==self.database_stamp,
            "Original verification source changed after current-authority wait");
        self.custody.fence(self.root)?;
        Ok(())
    }
}

/// Cryptographically authenticates only a supplied access bearer. Current user,
/// role, session, lifecycle and expiry are revalidated on the ORIGINAL stopped
/// source connection before capture output; copied bundle sessions are irrelevant.
pub(crate) struct VerificationHumanCredential {
    admin:crate::auth::middleware::AdminUser,
}
impl VerificationHumanCredential {
    pub(crate) fn supplied(token:&str,configured_secret:&str)->anyhow::Result<Self> {
        anyhow::ensure!(configured_secret.len()>=32,"Configured verification authentication secret must be strong");
        anyhow::ensure!(!token.is_empty()&&token.len()<=8192&&!token.bytes().any(|b|b.is_ascii_whitespace()),"Invalid bounded supplied verification bearer");
        let claims=crate::auth::jwt::validate_token(token,configured_secret,crate::auth::jwt::TokenType::Access)
            .map_err(|_|fail("Supplied verification access credential invalid"))?;
        anyhow::ensure!(claims.role=="admin","Original human instance administrator required");
        Ok(Self{admin:crate::auth::middleware::AdminUser{claims,api_key_scopes:None}})
    }
    pub(crate) fn admin(&self)->&crate::auth::middleware::AdminUser {&self.admin}
}
