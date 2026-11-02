/// Certified hardlink topology in an incomplete private destination. This token
/// grants neither application installation nor activation.
pub(crate) struct PlannedDestination {
    copied: PreparedDestination,
    plan: crate::storage::restoration_plan::CustodyPlan,
    leaves: std::collections::BTreeMap<String, PlannedLeaf>,
    directories: std::collections::BTreeMap<String, (File, PhysicalStamp)>,
    root_stamp: PhysicalStamp,
    marker_stamp: PhysicalStamp,
    minted: std::collections::BTreeMap<String, PendingMintEvidence>,
    // Only existing reconstructed alias groups are sealed. Database, marker,
    // ledgers and newly minted groups retain their separate mutation protocols.
    group_custody: Option<crate::storage::file_read_custody::CustodySet>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct PhysicalStamp {
    device: u64,
    inode: u64,
    length: u64,
    links: u64,
    mode: u32,
    mtime: i64,
    mtime_ns: i64,
    ctime: i64,
    ctime_ns: i64,
}
fn physical_stamp(file: &File) -> io::Result<PhysicalStamp> {
    let m = file.metadata()?;
    Ok(PhysicalStamp {
        device: m.dev(),
        inode: m.ino(),
        length: m.len(),
        links: m.nlink(),
        mode: m.mode(),
        mtime: m.mtime(),
        mtime_ns: m.mtime_nsec(),
        ctime: m.ctime(),
        ctime_ns: m.ctime_nsec(),
    })
}
struct PlannedLeaf {
    held: File,
    stamp: PhysicalStamp,
    bytes: crate::storage::backup_inventory::Bytes,
}
// Filesystem selection is one admission prerequisite, never a certification by
// name. Every selected inode must also acquire a real read lease and watch and
// retain them through each fence; all existing hash/identity/topology checks stay.
// Unknown, remote, userspace and stacked profiles cannot use this new operation.
fn materialized_group_local_vfs(file: &File) -> io::Result<()> {
    crate::storage::file_read_custody::admit_local_filesystem(file)
}
fn planned_hash(file: &File) -> io::Result<crate::storage::backup_inventory::Bytes> {
    let before = physical_stamp(file)?;
    if !file.metadata()?.is_file() || before.mode & 0o777 != 0o600 || before.links == 0 {
        return Err(fail("Planned leaf is not private regular data"));
    }
    let mut hash = Sha256::new();
    let mut offset = 0;
    let mut buffer = [0u8; 65536];
    while offset < before.length {
        let wanted = (before.length - offset).min(buffer.len() as u64) as usize;
        let n = file.read_at(&mut buffer[..wanted], offset)?;
        if n == 0 {
            return Err(fail("Planned bytes truncated"));
        }
        hash.update(&buffer[..n]);
        offset += n as u64;
    }
    if physical_stamp(file)? != before {
        return Err(fail("Planned bytes changed while hashing"));
    }
    Ok(crate::storage::backup_inventory::Bytes {
        size_bytes: offset,
        sha256: hex::encode(hash.finalize()),
    })
}
fn certified_parent(copy: &PreparedDestination, path: &str) -> io::Result<(File, String)> {
    if !valid_relative(path) || path.contains('\\') {
        return Err(fail("Invalid certified path"));
    }
    let (parent, leaf) = path.rsplit_once('/').unwrap_or(("", path));
    let mut fd = copy.root.fd().try_clone()?;
    let mut prefix = String::new();
    for part in parent.split('/').filter(|p| !p.is_empty()) {
        fd = open_at(&fd, part, libc::O_RDONLY | libc::O_DIRECTORY)?;
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        if copy.directories.get(&prefix) != Some(&identity(&fd)?)
            || fd.metadata()?.mode() & 0o777 != 0o700
        {
            return Err(fail("Certified parent changed"));
        }
    }
    Ok((fd, leaf.into()))
}
fn protected_evidence(path: &str) -> bool {
    path == "starlane-notes.db"
        || path.starts_with(".restoration-custody/")
        || path == DESTINATION_MARKER
        || path.rsplit('/').next().is_some_and(|p| {
            matches!(
                p,
                "manifest.json" | "install-proof.json" | "receipt.json" | "receipt-seal.json"
            )
        })
}
impl PlannedDestination {
    fn group_custody_fence(&self) -> io::Result<()> {
        if let Some(custody) = &self.group_custody { custody.fence()?; }
        Ok(())
    }
    pub(crate) fn path(&self) -> &Path {
        self.copied.path()
    }
    pub(crate) fn plan(&self) -> &crate::storage::restoration_plan::CustodyPlan {
        &self.plan
    }
    /// Read-only descriptor identity for a certified consumer. No global alias.
    pub(crate) fn observed(&self, path: &str) -> io::Result<(u64, u64, u64)> {
        self.fence()?;
        let leaf = self
            .leaves
            .get(path)
            .ok_or_else(|| fail("Unknown planned leaf"))?;
        Ok((leaf.stamp.device, leaf.stamp.inode, leaf.stamp.links))
    }
    pub(crate) fn fence(&self) -> io::Result<()> {
        self.group_custody_fence()?;
        self.copied.guard()?;
        if physical_stamp(self.copied.root.fd())? != self.root_stamp
            || physical_stamp(&self.copied.marker)? != self.marker_stamp
        {
            return Err(fail("Planned root or marker changed"));
        }
        fn walk(
            fd: &File,
            prefix: &str,
            files: &mut std::collections::BTreeSet<String>,
            dirs: &mut std::collections::BTreeSet<String>,
        ) -> io::Result<()> {
            for entry in std::fs::read_dir(format!("/proc/self/fd/{}", fd.as_raw_fd()))? {
                let name = entry?
                    .file_name()
                    .into_string()
                    .map_err(|_| fail("Invalid planned entry"))?;
                if prefix.is_empty() && name == DESTINATION_MARKER {
                    continue;
                }
                if !valid_relative(&name) || name.contains(['/', '\\']) {
                    return Err(fail("Unsafe planned entry"));
                }
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                let leaf = open_at(fd, &name, libc::O_RDONLY | libc::O_NONBLOCK)?;
                if leaf.metadata()?.is_dir() {
                    dirs.insert(path.clone());
                    walk(&leaf, &path, files, dirs)?;
                } else if leaf.metadata()?.is_file() {
                    files.insert(path);
                } else {
                    return Err(fail("Nonregular planned entry"));
                }
            }
            Ok(())
        }
        let mut files = std::collections::BTreeSet::new();
        let mut dirs = std::collections::BTreeSet::new();
        walk(self.copied.root.fd(), "", &mut files, &mut dirs)?;
        if files != self.leaves.keys().cloned().collect()
            || dirs != self.directories.keys().cloned().collect()
        {
            return Err(fail("Planned inventory changed"));
        }
        for (path, (held, stamp)) in &self.directories {
            let fresh = destination_directory(self.copied.root.fd(), path)?;
            if physical_stamp(held)? != *stamp || physical_stamp(&fresh)? != *stamp {
                return Err(fail("Planned directory changed"));
            }
        }
        for (path, leaf) in &self.leaves {
            let (parent, name) = planned_parent(self, path)?;
            let fresh = open_at(&parent, &name, libc::O_RDONLY | libc::O_NONBLOCK)?;
            if physical_stamp(&leaf.held)? != leaf.stamp
                || physical_stamp(&fresh)? != leaf.stamp
                || planned_hash(&fresh)? != leaf.bytes
                || physical_stamp(&fresh)? != leaf.stamp
            {
                return Err(fail("Planned leaf changed"));
            }
        }
        self.copied.guard()?;
        if physical_stamp(self.copied.root.fd())? != self.root_stamp
            || physical_stamp(&self.copied.marker)? != self.marker_stamp
        {
            return Err(fail("Planned root or marker changed"));
        }
        self.pending_evidence_fence()?;
        self.group_custody_fence()?;
        Ok(())
    }
}
pub(crate) fn materialize_groups(
    copy: PreparedDestination,
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    plan: crate::storage::restoration_plan::CustodyPlan,
) -> io::Result<PlannedDestination> {
    materialize_groups_inner(copy, bundle, plan, |_, _| Ok(()))
}
#[cfg(test)]
pub(crate) fn materialize_groups_test(
    copy: PreparedDestination,
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    plan: crate::storage::restoration_plan::CustodyPlan,
    hook: impl FnMut(&str, &str) -> io::Result<()>,
) -> io::Result<PlannedDestination> {
    materialize_groups_inner(copy, bundle, plan, hook)
}
fn materialize_groups_inner(
    copy: PreparedDestination,
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    plan: crate::storage::restoration_plan::CustodyPlan,
    mut hook: impl FnMut(&str, &str) -> io::Result<()>,
) -> io::Result<PlannedDestination> {
    bundle.fence()?;
    copy.fence()?;
    if copy.capture_manifest_sha256 != digest(&bundle.raw_manifest)
        || plan.backup_id() != bundle.manifest.backup_id
        || plan.manifest_sha256() != digest(&bundle.raw_manifest)
        || plan.database_sha256() != bundle.manifest.files.0["starlane-notes.db"].sha256
        || plan.source_root() != &bundle.manifest.source_custody.root
    {
        return Err(fail("Plan and copied source differ"));
    }
    let marker_before = physical_stamp(&copy.marker)?;
    let mut initial = std::collections::BTreeMap::new();
    for (path, (expected, bytes)) in &copy.files {
        let (parent, name) = certified_parent(&copy, path)?;
        let held = open_at(&parent, &name, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let stamp = physical_stamp(&held)?;
        if identity(&held)? != *expected || stamp.links != 1 || planned_hash(&held)? != *bytes {
            return Err(fail("Initial certified copy changed"));
        }
        initial.insert(path.clone(), (held, stamp));
    }
    let mut approved = std::collections::BTreeSet::new();
    let mut work = Vec::new();
    for group in plan.groups() {
        let physical = (group.captured_device, group.captured_inode);
        let mut members = group.binding_members.clone();
        members.sort();
        members.dedup();
        for path in &members {
            if protected_evidence(path)
                || !approved.insert(path.clone())
                || !group.members.contains(path)
            {
                return Err(fail("Unapproved hardlink member"));
            }
            let captured = bundle
                .manifest
                .source_custody
                .files
                .0
                .get(path)
                .ok_or_else(|| fail("Certified capture missing"))?;
            let expected = copy
                .files
                .get(path)
                .ok_or_else(|| fail("Certified copy missing"))?;
            if (captured.device, captured.inode) != physical
                || expected.1.size_bytes != group.length
                || expected.1.sha256 != group.sha256
                || !plan.scopes().iter().flat_map(|s| &s.bindings).any(|b| {
                    b.relative == *path
                        && (b.captured_device, b.captured_inode) == physical
                        && b.length == group.length
                        && b.sha256 == group.sha256
                })
            {
                return Err(fail("Hardlink member lacks scoped certification"));
            }
        }
        if members.len() > 1 {
            work.push(members);
        }
    }
    let mut expected_current = std::collections::BTreeMap::new();
    let mut held_current = std::collections::BTreeMap::new();
    for (path, (held, stamp)) in &initial {
        expected_current.insert(path.clone(), stamp.clone());
        held_current.insert(path.clone(), held.try_clone()?);
    }
    let mut group_custody = None;
    // All authorization is validated before the first filesystem mutation.
    for members in work {
        let anchor = &members[0];
        let (ap, an) = certified_parent(&copy, anchor)?;
        let anchor_fd = open_at(&ap, &an, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let mut anchor_stamp = initial[anchor].1.clone();
        if physical_stamp(&anchor_fd)? != anchor_stamp
            || identity(&anchor_fd)? != copy.files[anchor].0
            || planned_hash(&anchor_fd)? != copy.files[anchor].1
        {
            return Err(fail("Certified anchor changed"));
        }
        for path in &members[1..] {
            copy.guard()?;
            if physical_stamp(&copy.marker)? != marker_before
                || physical_stamp(&anchor_fd)? != anchor_stamp
                || identity(&open_at(&ap, &an, libc::O_RDONLY | libc::O_NONBLOCK)?)?
                    != identity(&anchor_fd)?
            {
                return Err(fail("Certified anchor or marker changed"));
            }
            let (tp, tn) = certified_parent(&copy, path)?;
            let old = open_at(&tp, &tn, libc::O_RDONLY | libc::O_NONBLOCK)?;
            if physical_stamp(&old)? != initial[path].1
                || identity(&old)? != copy.files[path].0
                || planned_hash(&old)? != copy.files[path].1
                || old.metadata()?.nlink() != 1
            {
                return Err(fail("Certified independent target changed"));
            }
            let temporary = format!(".starlane-link-{}.pending", uuid::Uuid::new_v4());
            let ca = CString::new(an.as_str()).map_err(|_| fail("Invalid anchor"))?;
            let ct =
                CString::new(temporary.as_str()).map_err(|_| fail("Invalid temporary name"))?;
            let cn = CString::new(tn.as_str()).map_err(|_| fail("Invalid target"))?;
            // SAFETY: both endpoints use held no-follow directory descriptors; a
            // random new leaf is exclusive. Verify the new link against held FD.
            if unsafe { libc::linkat(ap.as_raw_fd(), ca.as_ptr(), tp.as_raw_fd(), ct.as_ptr(), 0) }
                != 0
            {
                return Err(io::Error::last_os_error());
            }
            tp.sync_all()?;
            let linked_stamp = physical_stamp(&anchor_fd)?;
            if linked_stamp.links != anchor_stamp.links + 1
                || linked_stamp.device != anchor_stamp.device
                || linked_stamp.inode != anchor_stamp.inode
                || linked_stamp.length != anchor_stamp.length
                || planned_hash(&anchor_fd)? != copy.files[anchor].1
            {
                return Err(fail("Certified anchor link changed"));
            }
            hook("after-link-before-exchange", path)?;
            copy.guard()?;
            let (new_parent, _) = certified_parent(&copy, path)?;
            let (new_anchor_parent, _) = certified_parent(&copy, anchor)?;
            if identity(&new_parent)? != identity(&tp)?
                || identity(&new_anchor_parent)? != identity(&ap)?
            {
                return Err(fail("Certified ancestry raced"));
            }
            let new_link = open_at(&tp, &temporary, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let target = open_at(&tp, &tn, libc::O_RDONLY | libc::O_NONBLOCK)?;
            if physical_stamp(&copy.marker)? != marker_before
                || physical_stamp(&new_link)? != linked_stamp
                || physical_stamp(&anchor_fd)? != linked_stamp
                || physical_stamp(&target)? != initial[path].1
                || identity(&new_link)? != identity(&anchor_fd)?
                || identity(&target)? != identity(&old)?
                || planned_hash(&new_link)? != copy.files[anchor].1
                || planned_hash(&target)? != copy.files[path].1
            {
                return Err(fail("Certified exchange raced"));
            }
            // SAFETY: atomic exchange preserves both old and new bytes even on
            // interruption. The old target remains recoverable at temporary.
            if unsafe {
                libc::renameat2(
                    tp.as_raw_fd(),
                    ct.as_ptr(),
                    tp.as_raw_fd(),
                    cn.as_ptr(),
                    libc::RENAME_EXCHANGE,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            tp.sync_all()?;
            let exchanged = open_at(&tp, &tn, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let temporary_old = open_at(&tp, &temporary, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let exchanged_stamp = physical_stamp(&exchanged)?;
            let retired_stamp = physical_stamp(&temporary_old)?;
            hook("after-exchange-before-cleanup", path)?;
            copy.guard()?;
            let (new_parent, _) = certified_parent(&copy, path)?;
            let (new_anchor_parent, _) = certified_parent(&copy, anchor)?;
            if identity(&new_parent)? != identity(&tp)?
                || identity(&new_anchor_parent)? != identity(&ap)?
            {
                return Err(fail("Certified ancestry raced"));
            }
            let installed = open_at(&tp, &tn, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let retired = open_at(&tp, &temporary, libc::O_RDONLY | libc::O_NONBLOCK)?;
            if physical_stamp(&copy.marker)? != marker_before
                || physical_stamp(&installed)? != exchanged_stamp
                || physical_stamp(&exchanged)? != exchanged_stamp
                || physical_stamp(&retired)? != retired_stamp
                || physical_stamp(&temporary_old)? != retired_stamp
                || identity(&installed)? != identity(&anchor_fd)?
                || identity(&retired)? != identity(&old)?
                || planned_hash(&installed)? != copy.files[anchor].1
                || planned_hash(&retired)? != copy.files[path].1
            {
                return Err(fail("Certified exchange changed"));
            }
            // SAFETY: only our exchanged, pinned old target is removed; every
            // failure above retains the incomplete marker and both copies.
            if unsafe { libc::unlinkat(tp.as_raw_fd(), ct.as_ptr(), 0) } != 0 {
                return Err(io::Error::last_os_error());
            }
            anchor_fd.sync_all()?;
            tp.sync_all()?;
            copy.guard()?;
            if physical_stamp(&anchor_fd)? != exchanged_stamp {
                return Err(fail("Certified group changed after cleanup"));
            }
            anchor_stamp = exchanged_stamp;
        }
        // Construction is finished for this inode. Arm irreversible protection
        // before the certified observation and hook; never clear old events to
        // permit later groups or publication's unrelated authorized mutations.
        if group_custody.is_none() {
            group_custody = Some(crate::storage::file_read_custody::CustodySet::new(materialized_group_local_vfs)?);
        }
        let custody = group_custody.as_ref().expect("group custody just initialized");
        custody.seal(&anchor_fd)?;
        custody.fence()?;
        for path in &members {
            let (parent, name) = certified_parent(&copy, path)?;
            let held = open_at(&parent, &name, libc::O_RDONLY | libc::O_NONBLOCK)?;
            if physical_stamp(&held)? != anchor_stamp {
                return Err(fail("Certified group changed before token assembly"));
            }
            expected_current.insert(path.clone(), anchor_stamp.clone());
            held_current.insert(path.clone(), held);
        }
        hook("after-certified-group", anchor)?;
        custody.fence()?;
    }
    if physical_stamp(&copy.marker)? != marker_before {
        return Err(fail("Materialization marker changed"));
    }
    for (path, (held, stamp)) in &initial {
        if !approved.contains(path) {
            let (parent, name) = certified_parent(&copy, path)?;
            let fresh = open_at(&parent, &name, libc::O_RDONLY | libc::O_NONBLOCK)?;
            if physical_stamp(held)? != *stamp
                || physical_stamp(&fresh)? != *stamp
                || planned_hash(&fresh)? != copy.files[path].1
            {
                return Err(fail("Unapproved copied evidence changed"));
            }
        }
    }
    let mut leaves = std::collections::BTreeMap::new();
    let mut identities = std::collections::BTreeMap::<Identity, Vec<String>>::new();
    for (path, (_, bytes)) in &copy.files {
        let (parent, name) = certified_parent(&copy, path)?;
        let held = open_at(&parent, &name, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let stamp = physical_stamp(&held)?;
        if stamp != expected_current[path]
            || physical_stamp(&held_current[path])? != expected_current[path]
            || planned_hash(&held)? != *bytes
        {
            return Err(fail("Materialized bytes changed"));
        }
        identities
            .entry((stamp.device, stamp.inode))
            .or_default()
            .push(path.clone());
        leaves.insert(
            path.clone(),
            PlannedLeaf {
                held,
                stamp,
                bytes: bytes.clone(),
            },
        );
    }
    for paths in identities.values() {
        let first = &leaves[&paths[0]];
        if first.stamp.links != paths.len() as u64 {
            return Err(fail("Unapproved external hardlink"));
        }
        if paths.len() > 1 {
            let group = plan
                .groups()
                .iter()
                .find(|g| paths.iter().all(|p| g.binding_members.contains(p)))
                .ok_or_else(|| fail("Unapproved alias group"))?;
            if paths.len() != group.binding_members.len() {
                return Err(fail("Incomplete certified alias group"));
            }
        }
    }
    let mut directories = std::collections::BTreeMap::new();
    for (path, expected) in &copy.directories {
        let held = destination_directory(copy.root.fd(), path)?;
        if identity(&held)? != *expected {
            return Err(fail("Materialized directory changed"));
        }
        let stamp = physical_stamp(&held)?;
        directories.insert(path.clone(), (held, stamp));
    }
    hook("before-token-assembly", "")?;
    copy.root.fd().sync_all()?;
    copy.parent.fd().sync_all()?;
    bundle.fence()?;
    copy.guard()?;
    let root_stamp = physical_stamp(copy.root.fd())?;
    // No authorized step changes the marker; carry its original fingerprint.
    let marker_stamp = marker_before;
    let planned = PlannedDestination {
        copied: copy,
        plan,
        leaves,
        directories,
        root_stamp,
        marker_stamp,
        minted: std::collections::BTreeMap::new(),
        group_custody,
    };
    planned.fence()?;
    bundle.fence()?;
    planned.fence()?;
    Ok(planned)
}
