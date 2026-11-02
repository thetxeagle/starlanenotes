// Application-owned append-only proof preparation in a marked destination.
// No ledger publication, SQL completion, serving registration or marker removal.
struct PendingMintEvidence {
    intent_relative: String,
    intent_bytes: Vec<u8>,
    proof_relative: String,
    proof_bytes: Vec<u8>,
    scope: Scope,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PendingMintIntent<'a> {
    version: u32,
    intent_id: &'a str,
    backup_id: &'a str,
    backup_manifest_sha256: &'a str,
    database_sha256: &'a str,
    root_device: u64,
    root_inode: u64,
    namespace: &'a str,
    op_id: &'a str,
    source_root_device: u64,
    source_root_inode: u64,
    source_class: &'a str,
    original_manifest: &'a [u8],
    original_manifest_sha256: String,
    receipt_identity: &'a crate::storage::journal::StagedDocument,
    receipt_state: &'a str,
    source_proof_absent: bool,
    source_anchor_absent: bool,
    source_canonical_absent: bool,
    prior_ledger_sha256: Option<String>,
    planned_anchor: &'a str,
    planned_proof: &'a str,
}
fn planned_parent(destination: &PlannedDestination, relative: &str) -> io::Result<(File, String)> {
    if !valid_relative(relative) || relative.contains('\\') {
        return Err(fail("Unsafe planned path"));
    }
    let (parent, leaf) = relative.rsplit_once('/').unwrap_or(("", relative));
    let mut fd = destination.copied.root.fd().try_clone()?;
    let mut prefix = String::new();
    for part in parent.split('/').filter(|p| !p.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        fd = open_at(&fd, part, libc::O_RDONLY | libc::O_DIRECTORY)?;
        let (held, expected) = destination
            .directories
            .get(&prefix)
            .ok_or_else(|| fail("Uncertified planned parent"))?;
        if physical_stamp(held)? != *expected || physical_stamp(&fd)? != *expected {
            return Err(fail("Held planned parent changed"));
        }
    }
    Ok((fd, leaf.into()))
}
fn pending_pair(
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    d: &PlannedDestination,
) -> io::Result<()> {
    bundle.fence()?;
    d.fence()?;
    if d.copied.capture_manifest_sha256 != digest(&bundle.raw_manifest)
        || d.plan.backup_id() != bundle.manifest.backup_id
        || d.plan.manifest_sha256() != digest(&bundle.raw_manifest)
        || d.plan.database_sha256() != bundle.manifest.files.0["starlane-notes.db"].sha256
        || d.plan.source_root() != &bundle.manifest.source_custody.root
    {
        return Err(fail("Pending mint plan/capture pairing differs"));
    }
    Ok(())
}
// Re-certify only the directory changed by this exact append. Every unrelated
// held leaf/directory and immutable marker remains pinned by Planned::fence.
fn pending_directory_delta(d: &mut PlannedDestination, parent: &str) -> io::Result<()> {
    if parent.is_empty() {
        let current = physical_stamp(d.copied.root.fd())?;
        if (current.device, current.inode, current.mode)
            != (d.root_stamp.device, d.root_stamp.inode, d.root_stamp.mode)
        {
            return Err(fail("Pending append root exchanged"));
        }
        d.copied.root.fd().sync_all()?;
        d.root_stamp = current;
    } else {
        let current = destination_directory(d.copied.root.fd(), parent)?;
        let (held, old) = d
            .directories
            .get_mut(parent)
            .ok_or_else(|| fail("Pending append parent not certified"))?;
        let stamp = physical_stamp(&current)?;
        if identity(held)? != identity(&current)?
            || (stamp.device, stamp.inode, stamp.mode) != (old.device, old.inode, old.mode)
        {
            return Err(fail("Pending append parent exchanged"));
        }
        current.sync_all()?;
        *old = stamp;
    }
    Ok(())
}
fn pending_new_directory(d: &mut PlannedDestination, relative: &str) -> io::Result<File> {
    if !valid_relative(relative) || relative.contains('\\') {
        return Err(fail("Unsafe pending directory"));
    }
    let mut prefix = String::new();
    for part in relative.split('/') {
        let parent = prefix.clone();
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        if d.directories.contains_key(&prefix) {
            let probe = format!("{prefix}/certified-parent-probe");
            planned_parent(d, &probe)?;
            continue;
        }
        d.fence()?;
        let (fd, name) = planned_parent(d, &prefix)?;
        let name = CString::new(name).map_err(|_| fail("Unsafe pending directory name"))?;
        if unsafe { libc::mkdirat(fd.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
            return Err(io::Error::last_os_error());
        }
        fd.sync_all()?;
        let held = destination_directory(d.copied.root.fd(), &prefix)?;
        if held.metadata()?.mode() & 0o777 != 0o700 {
            return Err(fail("New pending directory not private"));
        }
        let stamp = physical_stamp(&held)?;
        if d.directories
            .insert(prefix.clone(), (held, stamp))
            .is_some()
        {
            return Err(fail("Pending directory reservation repeated"));
        }
        pending_directory_delta(d, &parent)?;
        d.fence()?;
    }
    destination_directory(d.copied.root.fd(), relative)
}
fn pending_new_file(d: &mut PlannedDestination, relative: &str, bytes: &[u8]) -> io::Result<File> {
    pending_write_file(
        d,
        relative,
        |f| {
            use std::io::Write;
            f.write_all(bytes)
        },
        crate::storage::backup_inventory::Bytes {
            size_bytes: bytes.len() as u64,
            sha256: digest(bytes),
        },
    )
}
fn pending_write_file(
    d: &mut PlannedDestination,
    relative: &str,
    write: impl FnOnce(&mut File) -> io::Result<()>,
    expected: crate::storage::backup_inventory::Bytes,
) -> io::Result<File> {
    d.fence()?;
    if d.leaves.contains_key(relative) {
        return Err(fail("Pending append leaf already certified"));
    }
    let (parent, name) = planned_parent(d, relative)?;
    let mut file = open_at(&parent, &name, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
    write(&mut file)?;
    file.sync_all()?;
    parent.sync_all()?;
    if planned_hash(&file)? != expected || file.metadata()?.nlink() != 1 {
        return Err(fail("New pending bytes differ"));
    }
    d.leaves.insert(
        relative.into(),
        PlannedLeaf {
            held: file.try_clone()?,
            stamp: physical_stamp(&file)?,
            bytes: expected,
        },
    );
    pending_directory_delta(d, relative.rsplit_once('/').map_or("", |p| p.0))?;
    d.fence()?;
    Ok(file)
}
fn pending_new_link(d: &mut PlannedDestination, source: &str, target: &str) -> io::Result<()> {
    d.fence()?;
    if d.leaves.contains_key(target) {
        return Err(fail("Pending link would replace existing leaf"));
    }
    let source_leaf = d
        .leaves
        .get(source)
        .ok_or_else(|| fail("Uncertified pending link source"))?;
    let held = source_leaf.held.try_clone()?;
    let before = source_leaf.stamp.clone();
    let bytes = source_leaf.bytes.clone();
    let (parent, name) = planned_parent(d, target)?;
    crate::storage::journal_fs::link_descriptor(&held, &parent, &name)?;
    parent.sync_all()?;
    held.sync_all()?;
    let after = physical_stamp(&held)?;
    if (
        after.device,
        after.inode,
        after.length,
        after.mode,
        after.mtime,
        after.mtime_ns,
    ) != (
        before.device,
        before.inode,
        before.length,
        before.mode,
        before.mtime,
        before.mtime_ns,
    ) || after.links != before.links + 1
        || planned_hash(&held)? != bytes
    {
        return Err(fail("Pending link source changed"));
    }
    // Only this newly minted inode may acquire links. Existing certified groups
    // can never be passed here from an operation request.
    for leaf in d
        .leaves
        .values_mut()
        .filter(|l| l.stamp.device == before.device && l.stamp.inode == before.inode)
    {
        if leaf.stamp != before {
            return Err(fail("Pending source group stamps differ"));
        }
        leaf.stamp = after.clone();
    }
    d.leaves.insert(
        target.into(),
        PlannedLeaf {
            held: held.try_clone()?,
            stamp: after,
            bytes,
        },
    );
    pending_directory_delta(d, target.rsplit_once('/').map_or("", |p| p.0))?;
    d.fence()
}
impl PlannedDestination {
    #[cfg(test)]
    pub(crate) fn minted_count(&self) -> usize {
        self.minted.len()
    }
    fn pending_evidence_fence(&self) -> io::Result<()> {
        for (op, evidence) in &self.minted {
            if evidence.scope.namespace != ".storage-journal"
                || evidence.scope.op_id != *op
                || evidence.scope.evidence_sha256 != digest(&evidence.proof_bytes)
            {
                return Err(fail("Pending proof evidence scope changed"));
            }
            for (path, raw) in [
                (&evidence.intent_relative, &evidence.intent_bytes),
                (&evidence.proof_relative, &evidence.proof_bytes),
            ] {
                let leaf = self
                    .leaves
                    .get(path)
                    .ok_or_else(|| fail("Pending evidence leaf missing"))?;
                if leaf.bytes.sha256 != digest(raw) || leaf.bytes.size_bytes != raw.len() as u64 {
                    return Err(fail("Pending immutable evidence differs"));
                }
            }
            for binding in &evidence.scope.bindings {
                let leaf = self
                    .leaves
                    .get(&binding.relative)
                    .ok_or_else(|| fail("Pending binding absent"))?;
                if (leaf.stamp.device, leaf.stamp.inode, leaf.stamp.links)
                    != (binding.adopted_device, binding.adopted_inode, 3)
                    || leaf.bytes.size_bytes != binding.length
                    || leaf.bytes.sha256 != binding.sha256
                {
                    return Err(fail("Pending binding changed"));
                }
            }
        }
        Ok(())
    }
}
pub(crate) async fn mint_pending_preproof(
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    destination: PlannedDestination,
    op: &str,
) -> io::Result<PlannedDestination> {
    mint_pending_preproof_inner(bundle, destination, op, |_| Ok(())).await
}
#[cfg(test)]
pub(crate) async fn mint_pending_preproof_test(
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    destination: PlannedDestination,
    op: &str,
    checkpoint: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<PlannedDestination> {
    mint_pending_preproof_inner(bundle, destination, op, checkpoint).await
}
async fn mint_pending_preproof_inner(
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    mut d: PlannedDestination,
    op: &str,
    mut checkpoint: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<PlannedDestination> {
    pending_pair(bundle, &d)?;
    if !valid_id(op) || d.minted.contains_key(op) {
        return Err(fail("Pending operation duplicate or invalid"));
    }
    let plan_scope = d
        .plan
        .scopes()
        .iter()
        .find(|s| s.namespace == ".storage-journal" && s.operation_id == op)
        .ok_or_else(|| fail("Pending operation not owner-classified by plan"))?;
    if plan_scope.class != crate::storage::restoration_plan::OperationClass::CommittedPending
        || plan_scope.raw_manifest.is_none()
        || plan_scope.evidence.as_deref() != plan_scope.raw_manifest.as_deref()
        || plan_scope.evidence_sha256.as_deref()
            != plan_scope
                .raw_manifest
                .as_ref()
                .map(|raw| digest(raw))
                .as_deref()
        || !plan_scope.bindings.is_empty()
    {
        return Err(fail("Plan does not certify pending preproof"));
    }
    if d.plan.prior_lineage().is_some_and(|p| {
        p.scopes
            .iter()
            .any(|s| s.namespace == ".storage-journal" && s.op_id == op)
    }) {
        return Err(fail(
            "Prior operation cannot be reinterpreted as pending preproof",
        ));
    }
    let mut conn = bundle.db.acquire().await.map_err(io::Error::other)?;
    let prior = inspect_prior_lineage(bundle, &mut conn).await?;
    if prior.as_ref().is_some_and(|p|p.current_denied().iter().any(|d|d.namespace==".storage-journal"&&d.op_id==op)) {
        return Err(fail("Prior denied operation cannot mint pending preproof"));
    }
    let inspection =
        crate::storage::journal::inspect_restoration(&bundle.data_path(), &mut conn, op)
            .await
            .map_err(|_| fail("Pending application classification refused"))?;
    if inspection.class != crate::storage::journal::StorageOperationClass::CommittedPending
        || inspection.install.proof.is_some()
        || inspection.install.installed_present
        || !inspection.install.required_group.is_empty()
        || plan_scope.raw_manifest.as_deref() != Some(&inspection.raw_manifest)
    {
        return Err(fail("Pending application proof/manifest differs"));
    }
    let m = inspection.manifest;
    drop(conn);
    pending_pair(bundle, &d)?;
    let stage_relative = format!(".storage-journal/{op}");
    // The exact held inventory already proves absence, including absent ancestors.
    // Directory-shaped impostors are not treated as missing files.
    for path in [
        format!("{stage_relative}/install-proof.json"),
        format!("{stage_relative}/install-anchor"),
        m.path.clone(),
    ] {
        if d.leaves.contains_key(&path) || d.directories.contains_key(&path) {
            return Err(fail("Pending canonical/proof already present"));
        }
    }
    let payload = d
        .leaves
        .get(&format!("{stage_relative}/payload"))
        .ok_or_else(|| fail("Pending payload absent"))?
        .held
        .try_clone()?;
    let descriptor = crate::storage::journal_fs::FileDescriptor::from(&m);
    crate::storage::journal_fs::validate_descriptor(&mut payload.try_clone()?, &descriptor)?;
    let intent_id = uuid::Uuid::new_v4().to_string();
    let anchor_name = format!("anchor-{}", uuid::Uuid::new_v4());
    let proof_name = format!("proof-{}", uuid::Uuid::new_v4());
    let root_id = identity(d.copied.root.fd())?;
    let intent_bytes = serde_json::to_vec(&PendingMintIntent {
        version: 1,
        intent_id: &intent_id,
        backup_id: d.plan.backup_id(),
        backup_manifest_sha256: d.plan.manifest_sha256(),
        database_sha256: d.plan.database_sha256(),
        root_device: root_id.0,
        root_inode: root_id.1,
        namespace: ".storage-journal",
        op_id: op,
        source_root_device: d.plan.source_root().device,
        source_root_inode: d.plan.source_root().inode,
        source_class: "committed-pending-preproof",
        original_manifest: &inspection.raw_manifest,
        original_manifest_sha256: digest(&inspection.raw_manifest),
        receipt_identity: &m,
        receipt_state: "pending",
        source_proof_absent: true,
        source_anchor_absent: true,
        source_canonical_absent: true,
        prior_ledger_sha256: prior.as_ref().map(|p| digest(p.raw_ledger())),
        planned_anchor: &anchor_name,
        planned_proof: &proof_name,
    })
    .map_err(io::Error::other)?;
    pending_new_directory(&mut d, NAMESPACE)?;
    let intent_relative = format!("{NAMESPACE}/preproof-{intent_id}.json");
    pending_new_file(&mut d, &intent_relative, &intent_bytes)?;
    checkpoint("intent")?;
    pending_pair(bundle, &d)?;
    let anchor_relative = format!("{stage_relative}/{anchor_name}");
    // Read validated payload through a held descriptor without advancing offsets.
    // Bounded descriptor length is enforced by the owner inspection/manifest.
    let mut anchor = pending_write_file(
        &mut d,
        &anchor_relative,
        |file| {
            use std::io::Write;
            let mut offset = 0u64;
            let mut buffer = [0u8; 65536];
            while offset < m.length as u64 {
                let wanted = (m.length as u64 - offset).min(buffer.len() as u64) as usize;
                let n = payload.read_at(&mut buffer[..wanted], offset)?;
                if n == 0 {
                    return Err(fail("Pending payload truncated"));
                }
                file.write_all(&buffer[..n])?;
                offset += n as u64;
            }
            Ok(())
        },
        crate::storage::backup_inventory::Bytes {
            size_bytes: m.length as u64,
            sha256: m.sha256.clone(),
        },
    )?;
    crate::storage::journal_fs::validate_descriptor(&mut anchor, &descriptor)?;
    checkpoint("anchor")?;
    pending_pair(bundle, &d)?;
    let proof_bytes =
        crate::storage::journal_fs::encode_install_proof(&descriptor, &anchor_name, &mut anchor)?;
    let proof_relative = format!("{stage_relative}/{proof_name}");
    pending_new_file(&mut d, &proof_relative, &proof_bytes)?;
    pending_new_link(
        &mut d,
        &proof_relative,
        &format!("{stage_relative}/install-proof.json"),
    )?;
    checkpoint("proof")?;
    pending_pair(bundle, &d)?;
    pending_new_link(
        &mut d,
        &anchor_relative,
        &format!("{stage_relative}/install-anchor"),
    )?;
    let canonical_parent = m
        .path
        .rsplit_once('/')
        .ok_or_else(|| fail("Pending canonical parent missing"))?
        .0;
    pending_new_directory(&mut d, canonical_parent)?;
    pending_new_link(&mut d, &anchor_relative, &m.path)?;
    checkpoint("canonical")?;
    pending_pair(bundle, &d)?;
    let adopted = identity(&anchor)?;
    let scope = Scope {
        namespace: ".storage-journal".into(),
        op_id: op.into(),
        evidence_sha256: digest(&proof_bytes),
        bindings: [
            anchor_relative,
            format!("{stage_relative}/install-anchor"),
            m.path.clone(),
        ]
        .into_iter()
        .map(|relative| Binding {
            relative,
            original_device: adopted.0,
            original_inode: adopted.1,
            adopted_device: adopted.0,
            adopted_inode: adopted.1,
            length: m.length as u64,
            sha256: m.sha256.clone(),
        })
        .collect(),
    };
    d.minted.insert(
        op.into(),
        PendingMintEvidence {
            intent_relative,
            intent_bytes,
            proof_relative,
            proof_bytes,
            scope,
        },
    );
    d.fence()?;
    bundle.fence()?;
    d.fence()?;
    Ok(d)
}
