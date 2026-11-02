// Retained SQLite transition artifacts, outside DATA_DIR and serving custody.
// No cleanup/unlink path exists. Provenance is durably recorded before SQL.
struct VerificationSqlSpool {
    parent:Root,
    name:String,
    directory:File,
    stamp:PhysicalStamp,
    records:std::collections::BTreeMap<String,PlannedLeaf>,
}
#[derive(Serialize)]
struct VerificationSpoolProvenance<'a> {
    version:u32,
    generation_id:&'a str,
    backup_id:&'a str,
    backup_manifest_sha256:&'a str,
    database_sha256:&'a str,
    destination_device:u64,
    destination_inode:u64,
    parent_device:u64,
    parent_inode:u64,
    spool_name:&'a str,
    purpose:&'a str,
    journal_source:&'a str,
    transition_wal_source:&'a str,
    transition_wal_retained_pattern:&'a str,
    retained_pattern:&'a str,
}
impl VerificationSqlSpool {
    fn create(selection:&OriginalTargetRecoverySelection<'_>)->io::Result<Self> {
        selection.fence().map_err(|e|fail(&e.to_string()))?;
        let parent=Root::open(selection.path().parent().ok_or_else(||fail("Recovery target parent absent"))?)?;
        let name=format!(".starlane-verified-startup-{}.retained",uuid::Uuid::new_v4());
        let c=CString::new(name.as_str()).unwrap();
        if unsafe{libc::mkdirat(parent.fd().as_raw_fd(),c.as_ptr(),0o700)}!=0{return Err(io::Error::last_os_error());}
        parent.fd().sync_all()?;
        let directory=open_at(parent.fd(),&name,libc::O_RDONLY|libc::O_DIRECTORY)?;
        let id=identity(parent.fd())?;
        let raw=publication_json(&VerificationSpoolProvenance{version:1,generation_id:&selection.active.ledger.generation_id,backup_id:&selection.bundle.manifest.backup_id,
            backup_manifest_sha256:&digest(&selection.bundle.raw_manifest),database_sha256:&planned_hash(&selection.database)?.sha256,
            destination_device:selection.root_stamp.device,destination_inode:selection.root_stamp.inode,parent_device:id.0,parent_inode:id.1,
            spool_name:&name,purpose:"original-activated-target-startup-journal-not-serving-or-cleanup-authority",
            journal_source:"starlane-notes.db-journal",transition_wal_source:"REFUSED",transition_wal_retained_pattern:"REFUSED",retained_pattern:"journal-<uuid>.retained"})?;
        let mut record=open_at(&directory,"provenance.json",libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
        {use std::io::Write;record.write_all(&raw)?;}record.sync_all()?;directory.sync_all()?;
        let leaf=PlannedLeaf{stamp:physical_stamp(&record)?,bytes:planned_hash(&record)?,held:record};
        let stamp=physical_stamp(&directory)?;
        let out=Self{parent,name,directory,stamp,records:std::collections::BTreeMap::from([("provenance.json".into(),leaf)])};
        out.fence()?;selection.fence().map_err(|e|fail(&e.to_string()))?;Ok(out)
    }
    fn path(&self)->std::path::PathBuf {self.parent.absolute.join(&self.name)}
    fn fence(&self)->io::Result<()> {
        self.parent.check()?;
        let current=open_at(self.parent.fd(),&self.name,libc::O_RDONLY|libc::O_DIRECTORY)?;
        if physical_stamp(&current)?!=self.stamp||physical_stamp(&self.directory)?!=self.stamp||self.stamp.mode&0o777!=0o700 {
            return Err(fail("Retained SQL spool exchanged"));
        }
        let mut names=std::collections::BTreeSet::new();
        for e in std::fs::read_dir(format!("/proc/self/fd/{}",self.directory.as_raw_fd()))? {
            let name=e?.file_name().into_string().map_err(|_|fail("Invalid spool entry"))?;
            let expected=self.records.get(&name).ok_or_else(||fail("Unknown retained SQL artifact"))?;
            let file=open_at(&self.directory,&name,libc::O_RDONLY)?;
            if physical_stamp(&file)?!=expected.stamp||physical_stamp(&expected.held)?!=expected.stamp||planned_hash(&file)?!=expected.bytes {
                return Err(fail("Retained SQL evidence changed"));
            }names.insert(name);
        }
        if names!=self.records.keys().cloned().collect(){return Err(fail("Retained SQL artifact missing"));}Ok(())
    }
    fn retain_journal(&mut self,root:&File,pin:&File)->io::Result<()> {self.retain_sidecar(root,pin,false)}
    fn retain_empty_wal(&mut self,_root:&File,_pin:&File)->io::Result<()> {Err(fail("Activated target mode transition refused"))}
    fn retain_sidecar(&mut self,root:&File,pin:&File,wal:bool)->io::Result<()> {
        self.fence()?;
        let old=physical_stamp(pin)?;
        let raw=planned_hash(pin)?;
        let role=if wal{"starlane-notes.db-wal"}else{"starlane-notes.db-journal"};
        if wal && (old.length!=0 || raw.size_bytes!=0 || raw.sha256!=digest(b"")){return Err(fail("Transition WAL not empty"));}
        if old.links!=1||physical_stamp(&open_at(root,role,libc::O_RDONLY)?)?!=old {
            return Err(fail("SQLite journal exchanged before retention"));
        }
        let name=format!("{}-{}.retained",if wal{"empty-wal"}else{"journal"},uuid::Uuid::new_v4());
        let source=CString::new(role).unwrap();let target=CString::new(name.as_str()).unwrap();

        if unsafe{libc::syscall(libc::SYS_renameat2,root.as_raw_fd(),source.as_ptr(),self.directory.as_raw_fd(),target.as_ptr(),libc::RENAME_NOREPLACE)}!=0{return Err(io::Error::last_os_error());}
        root.sync_all()?;self.directory.sync_all()?;
        let moved=open_at(&self.directory,&name,libc::O_RDONLY)?;
        let now=physical_stamp(&moved)?;
        if (now.device,now.inode,now.mode,now.links,now.length,now.mtime,now.mtime_ns)
            !=(old.device,old.inode,old.mode,old.links,old.length,old.mtime,old.mtime_ns)||planned_hash(&moved)?!=raw {
            // Wrong moved leaf is retained and NEVER deleted/certified; caller
            // stops SQL/completion and the original marker stays incomplete.
            return Err(fail("Moved SQLite journal did not match held owner FD"));
        }
        self.records.insert(name,PlannedLeaf{held:moved,stamp:now,bytes:raw});
        let changed=physical_stamp(&self.directory)?;
        if (changed.device,changed.inode,changed.mode)!=(self.stamp.device,self.stamp.inode,self.stamp.mode){return Err(fail("Retained spool changed during move"));}
        self.stamp=changed;self.fence()
    }
}
