//! Read-only admission shared by canonical commits and derived mirror writes.
//! Disk races after admission still use the existing canonical/mirror_pending receipt.
use super::*;

const FOLDER_PATH_NODE_LIMIT: usize = 10_000;
const MIRROR_SCAN_ENTRY_LIMIT: usize = 100_000;

struct Folder<'a> {
    name: &'a str,
    space: &'a str,
    parent: Option<&'a str>,
}

fn folder_index(snapshot: &Value) -> Result<HashMap<&str, Folder<'_>>, String> {
    let mut folders = HashMap::new();
    for row in value_array(snapshot, "folders") {
        let id = value_str(row, "id").filter(|id| !id.is_empty()).ok_or("Folder has no identity")?;
        let space = value_str(row, "space_id").filter(|id| !id.is_empty()).ok_or("Folder has no space")?;
        let folder = Folder { name: value_str(row, "name").unwrap_or(id), space, parent: value_str(row, "parent_id") };
        if folders.insert(id, folder).is_some() { return Err("Folder hierarchy contains duplicate identity".into()); }
    }
    // Each parent edge is inspected once; depth is memoized without retaining
    // O(depth²) copies of ancestor paths. Unrelated roots do not consume a budget.
    let mut depths = HashMap::<&str, usize>::new();
    for start in folders.keys().copied() {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut current = Some(start);
        let mut depth = 0;
        while let Some(id) = current {
            if let Some(known) = depths.get(id) { depth = *known; break; }
            if !seen.insert(id) { return Err("Folder hierarchy contains a cycle".into()); }
            if chain.len() >= FOLDER_PATH_NODE_LIMIT { return Err("Folder path exceeds 10000 participating nodes".into()); }
            let folder = folders.get(id).ok_or("Folder parent is unavailable")?;
            chain.push(id);
            current = folder.parent;
            if let Some(parent) = current {
                if folders.get(parent).map(|row| row.space) != Some(folder.space) {
                    return Err("Folder parent is unavailable in this space".into());
                }
            }
        }
        for id in chain.into_iter().rev() {
            depth += 1;
            if depth > FOLDER_PATH_NODE_LIMIT { return Err("Folder path exceeds 10000 participating nodes".into()); }
            depths.insert(id, depth);
        }
    }
    Ok(folders)
}

fn folder_path(root: &Path, space: &str, id: &str, folders: &HashMap<&str, Folder<'_>>) -> Result<PathBuf, String> {
    let mut chain = Vec::new();
    let mut current = Some(id);
    // The index has already proved acyclic, same-space parent links. This loop
    // remains independently bounded for callers and never uses the Rust stack.
    while let Some(id) = current {
        if chain.len() >= FOLDER_PATH_NODE_LIMIT { return Err("Folder path exceeds 10000 participating nodes".into()); }
        let folder = folders.get(id).ok_or("Page folder is unavailable")?;
        if folder.space != space { return Err("Page folder is unavailable in this space".into()); }
        chain.push(sanitize_path_segment(folder.name, id));
        current = folder.parent;
    }
    let mut path = root.to_path_buf();
    for segment in chain.into_iter().rev() { path.push(segment); }
    Ok(path)
}

/// Page parents share the owning space/folder; they are logical metadata, never
/// extra physical path components. Validate once, memoizing depth per record.
pub(super) fn validate_page_hierarchy(snapshot: &Value) -> Result<(),String> {
    const PAGE_PATH_NODE_LIMIT:usize=10_000;
    let mut pages=HashMap::new();
    for row in value_array(snapshot,"documents") {
        let id=value_str(row,"id").ok_or("Page has no identity")?;
        if pages.insert(id,row).is_some() {return Err("Page hierarchy contains duplicate identity".into());}
    }
    let mut depths=HashMap::<&str,usize>::new();
    for start in pages.keys().copied() {
        let mut trail=Vec::new();let mut seen=HashSet::new();let mut current=Some(start);let mut depth=0;
        while let Some(id)=current {
            if let Some(known)=depths.get(id) {depth=*known;break;}
            if !seen.insert(id) {return Err("Page hierarchy contains a cycle".into());}
            if trail.len()>=PAGE_PATH_NODE_LIMIT {return Err("Page ancestry exceeds 10000 participating nodes".into());}
            let row=pages.get(id).ok_or("Page parent is unavailable")?;trail.push(id);
            current=value_str(row,"parent_id");
            if let Some(parent)=current {
                let parent=pages.get(parent).ok_or("Page parent is unavailable")?;
                if value_str(parent,"space_id")!=value_str(row,"space_id") || value_str(parent,"folder_id")!=value_str(row,"folder_id") {
                    return Err("Page parent is unavailable in this space and folder".into());
                }
            }
        }
        for id in trail.into_iter().rev() {
            depth+=1;if depth>PAGE_PATH_NODE_LIMIT {return Err("Page ancestry exceeds 10000 participating nodes".into());}depths.insert(id,depth);
        }
    }
    Ok(())
}

pub(super) struct Entry<'a> {
    pub path: PathBuf,
    pub item: &'a Value,
    pub title_key: &'static str,
}
pub(super) struct Plan<'a> {
    pub cleanup: mirror_cleanup::Plan,
    pub space_dirs: Vec<PathBuf>,
    pub entries: Vec<Entry<'a>>,
    pub generated_paths: HashSet<String>,
}

/// Query the selected filesystem instead of substituting a product depth limit.
/// This proves current path/type/permission constraints, not future disk success.
fn check_native_path(path: &Path, directory: bool) -> Result<(), String> {
    let mut existing = if directory { path } else { path.parent().ok_or("Mirror path has no parent")? };
    loop {
        match fs::metadata(existing) {
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => return Err("Mirror ancestor is not a directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                existing = existing.parent().ok_or("Mirror has no accessible ancestor")?;
            }
            Err(error) => return Err(format!("Mirror path admission failed: {error}")),
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let parent = std::ffi::CString::new(existing.as_os_str().as_bytes()).map_err(|_| "Invalid mirror ancestor")?;
        let name_max = unsafe { libc::pathconf(parent.as_ptr(), libc::_PC_NAME_MAX) };
        let path_max = unsafe { libc::pathconf(parent.as_ptr(), libc::_PC_PATH_MAX) };
        if name_max > 0 && path.components().any(|part| part.as_os_str().as_bytes().len() > name_max as usize) {
            return Err("Mirror component exceeds this filesystem's name limit".into());
        }
        if path_max > 0 && path.as_os_str().as_bytes().len() >= path_max as usize {
            return Err("Mirror path exceeds this filesystem's path limit".into());
        }
        if unsafe { libc::access(parent.as_ptr(), libc::W_OK | libc::X_OK) } != 0 {
            return Err(format!("Mirror ancestor is not writable: {}", std::io::Error::last_os_error()));
        }
        if !directory {
            match fs::symlink_metadata(path) {
                Ok(_) => {
                    let file = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid mirror path")?;
                    if unsafe { libc::access(file.as_ptr(), libc::W_OK) } != 0 { return Err("Existing mirror file is not writable".into()); }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(format!("Mirror leaf admission failed: {error}")),
            }
        }
    }
    Ok(())
}

impl<'a> Plan<'a> {
    pub fn prepare(root: &Path, snapshot: &'a Value) -> Result<Self, String> {
        let cleanup = mirror_cleanup::Plan::prepare(root)?;
        let folders = folder_index(snapshot)?;
        validate_page_hierarchy(snapshot)?;
        let mut spaces = HashMap::new();
        for space in value_array(snapshot, "spaces") {
            let id = value_str(space, "id").unwrap_or("space");
            if spaces.insert(id, space_dir_path(root, space)).is_some() { return Err("Duplicate space identity".into()); }
        }
        // Legacy snapshots can omit their space catalog. Keep the established
        // fallback directory while enforcing folder/page space identity.
        for collection in ["folders", "documents", "drafts", "templates"] {
            for item in value_array(snapshot, collection) {
                let id = value_str(item, "space_id").unwrap_or("personal");
                spaces.entry(id).or_insert_with(|| root.join(MIRROR_SPACES_DIR).join(sanitize_space_dir_name(id, "space")));
            }
        }
        let mut directories = HashSet::new();
        for space in spaces.values() {
            for bucket in [MIRROR_DOCUMENTS_DIR, MIRROR_ATTACHMENTS_DIR, MIRROR_TEMPLATES_DIR, MIRROR_DRAFTS_DIR, MIRROR_ARCHIVED_DIR, MIRROR_TRASHED_DIR] {
                directories.insert(space.join(bucket));
            }
        }
        for (id, folder) in &folders {
            let path = folder_path(&spaces[folder.space].join(MIRROR_DOCUMENTS_DIR), folder.space, id, &folders)?;
            cleanup.check_directory(&path)?;
            check_native_path(&path, true)?;
            directories.insert(path);
        }
        let mut entries = Vec::new();
        let mut generated_paths = HashSet::new();
        for (collection, bucket, fallback, title_key) in [
            ("documents", MIRROR_DOCUMENTS_DIR, "document", "title"),
            ("drafts", MIRROR_DRAFTS_DIR, "draft", "title"),
            ("templates", MIRROR_TEMPLATES_DIR, "template", "name"),
        ] {
            let mut identities = HashSet::new();
            for item in value_array(snapshot, collection) {
                if let Some(id) = value_str(item, "id") {
                    if !identities.insert(id) { return Err(format!("Duplicate {collection} identity")); }
                }
                let space = value_str(item, "space_id").unwrap_or("personal");
                let mut directory = spaces[space].join(bucket);
                if collection == "documents" {
                    // Even archived/trashed pages cannot hide foreign placement.
                    let folder_dir = match value_str(item, "folder_id") {
                        Some(id) => Some(folder_path(&directory, space, id, &folders)?),
                        None => None,
                    };
                    directory = if value_str(item, "deleted_at").is_some() { spaces[space].join(MIRROR_TRASHED_DIR) }
                        else if value_str(item, "archived_at").is_some() { spaces[space].join(MIRROR_ARCHIVED_DIR) }
                        else { folder_dir.unwrap_or(directory) };
                }
                let path = directory.join(markdown_file_name(item, fallback));
                let relative = relative_path(root, &path).ok_or("Mirror path escaped the selected vault")?;
                if !generated_paths.insert(relative.clone()) { return Err("Distinct records collide in the Markdown mirror".into()); }
                cleanup.check(&relative)?;
                check_native_path(&path, false)?;
                entries.push(Entry { path, item, title_key });
            }
        }
        if entries.iter().any(|entry| directories.contains(&entry.path)) {
            return Err("A mirror file collides with a generated directory".into());
        }
        for directory in directories {
            cleanup.check_directory(&directory)?;
            check_native_path(&directory, true)?;
        }
        cleanup.verify_root()?;
        Ok(Self { cleanup, space_dirs: spaces.into_values().collect(), entries, generated_paths })
    }
}

pub(super) fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => return Err("Mirror import root is not an ordinary directory".into()),
        Ok(_) => {},
    }
    let mut pending = vec![root.to_path_buf()];
    let mut found = Vec::new();
    let mut count = 0;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            count += 1;
            if count > MIRROR_SCAN_ENTRY_LIMIT { return Err("Mirror import exceeds 100000 filesystem entries; no snapshot was changed".into()); }
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() { return Err("Mirror import contains a symbolic link; no snapshot was changed".into()); }
            if kind.is_dir() { pending.push(entry.path()); }
            else if kind.is_file() && entry.path().extension().and_then(|ext| ext.to_str()) == Some("md") { found.push(entry.path()); }
        }
    }
    files.extend(found);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("starlane-deep-folder-{}-{}", std::process::id(), now_nanos().unwrap()));
        fs::create_dir(&root).unwrap(); root
    }
    fn snapshot(depth: usize, name: &str) -> Value {
        let folders = (0..depth).map(|i| json!({"id":format!("f{i}"),"space_id":"s","name":name,"parent_id":if i==0 {Value::Null} else {json!(format!("f{}",i-1))}})).collect::<Vec<_>>();
        json!({"schema_version":1,"spaces":[{"id":"s","name":"S"}],"folders":folders,
            "documents":[{"id":"page","space_id":"s","title":"Page","content":"exact body\n<!-- retained -->","folder_id":format!("f{}",depth-1)}],"drafts":[],"templates":[],"assets":[]})
    }
    fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut result = BTreeMap::new(); let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(dir).unwrap() {
                let entry=entry.unwrap(); let path=entry.path(); let kind=entry.file_type().unwrap();
                if kind.is_symlink() { result.insert(path.clone(), fs::read_link(path).unwrap().as_os_str().as_encoded_bytes().to_vec()); }
                else if kind.is_dir() {result.insert(path.clone(),Vec::new());pending.push(path);}
                else { result.insert(path.clone(), fs::read(path).unwrap()); }
            }
        }
        result
    }
    #[test]
    fn deep_mirror_actual_write_reload_reconcile_archive_restore_and_iterative_scan() {
        for depth in [4,32,256] {
            let root=root(); let mut source=snapshot(depth,"x");
            write_vault_snapshot_at(&root,"none",&source).unwrap();
            let restored:Value=serde_json::from_slice(&fs::read(root.join(snapshot_file_name("none"))).unwrap()).unwrap();
            assert_eq!(restored,source);
            let paths=read_mirror_manifest(&root).unwrap(); assert_eq!(paths.len(),1);
            let original_path=root.join(paths.iter().next().unwrap()); assert!(original_path.is_file());
            assert_eq!(original_path.strip_prefix(&root).unwrap().components().count(),depth+4);
            // Modify the actual Markdown file, then reconcile through existing parser.
            let markdown=fs::read_to_string(&original_path).unwrap().replace("exact body", "edited body");
            fs::write(&original_path,markdown).unwrap();
            let reconciled=reconcile_filesystem_mirror_snapshot(&root,source.clone()).unwrap();
            assert_eq!(reconciled.documents_updated,1); assert!(reconciled.snapshot["documents"][0]["content"].as_str().unwrap().contains("edited body"));
            assert_eq!(reconciled.snapshot["folders"],source["folders"]);
            let mut scanned=Vec::new(); collect_markdown_files(&root.join("spaces"),&mut scanned).unwrap(); assert_eq!(scanned,vec![original_path.clone()]);
            // Untracked filesystem import retains every folder component.
            fs::write(original_path.parent().unwrap().join("External-untracked.md"),"external exact body").unwrap();
            let imported=reconcile_filesystem_mirror_snapshot(&root,source.clone()).unwrap();
            assert_eq!(imported.documents_imported,1);
            assert_eq!(imported.snapshot["documents"][1]["folder_id"],source["documents"][0]["folder_id"]);
            source["documents"][0]["archived_at"]=json!("2026-10-15T00:00:00Z");
            write_vault_snapshot_at(&root,"none",&source).unwrap(); assert!(!original_path.exists());
            assert!(root.join("spaces/S/archived/Page-page.md").exists());
            source["documents"][0].as_object_mut().unwrap().remove("archived_at");
            write_vault_snapshot_at(&root,"none",&source).unwrap(); assert!(original_path.exists());
            assert!(!root.join("spaces/S/archived/Page-page.md").exists());
            assert!(fs::read_to_string(original_path.parent().unwrap().join("External-untracked.md")).unwrap().contains("external exact body"));
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn invalid_graph_and_long_paths_refuse_before_snapshot_or_mirror_mutation() {
        for mode in ["cycle","orphan","foreign-parent","foreign-page","duplicate","long-path","file-directory-collision"] {
            let root=root(); let source=snapshot(4,"x"); write_vault_snapshot_at(&root,"none",&source).unwrap(); let before=tree(&root);
            let mut candidate=source.clone();
            match mode {
                "cycle" => candidate["folders"][0]["parent_id"]=json!("f3"),
                "orphan" => candidate["folders"][0]["parent_id"]=json!("absent"),
                "foreign-parent" => candidate["folders"][0]["space_id"]=json!("foreign"),
                "foreign-page" => candidate["documents"][0]["space_id"]=json!("foreign"),
                "duplicate" => {let duplicate=candidate["folders"][0].clone();candidate["folders"].as_array_mut().unwrap().push(duplicate);},
                "long-path" => candidate=snapshot(256,&"long".repeat(20)),
                _ => {candidate["documents"][0]["folder_id"]=Value::Null;candidate["folders"][0]["name"]=json!("Page-page.md");},
            }
            assert!(write_vault_snapshot_at(&root,"none",&candidate).is_err(),"{mode}"); assert_eq!(tree(&root),before,"{mode}");
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn graph_budget_is_per_chain_and_unrelated_roots_remain_admissible() {
        let mut source=snapshot(10_000,"x"); assert!(folder_index(&source).is_ok());
        source["folders"].as_array_mut().unwrap().push(json!({"id":"too-deep","space_id":"s","name":"x","parent_id":"f9999"}));
        assert!(folder_index(&source).err().unwrap().contains("10000"));
        source["folders"]=json!((0..10_001).map(|i|json!({"id":format!("root{i}"),"space_id":"s","name":"x"})).collect::<Vec<_>>());
        assert_eq!(folder_index(&source).unwrap().len(),10_001);
    }
    #[cfg(unix)]
    #[test]
    fn prospective_symlink_and_unwritable_ancestor_leave_all_state_unchanged() {
        use std::os::unix::fs::{symlink,PermissionsExt};
        for mode in ["symlink","directory-permission","file-permission"] {
            let root=root(); let outside=root.with_extension("outside"); fs::create_dir(&outside).unwrap();fs::write(outside.join("sentinel"),b"outside").unwrap();
            let source=snapshot(4,"x"); write_vault_snapshot_at(&root,"none",&source).unwrap();
            let mut candidate=source.clone(); candidate["spaces"][0]["name"]=json!("Other");
            match mode {
                "symlink" => symlink(&outside,root.join("spaces/Other")).unwrap(),
                "directory-permission" => {fs::create_dir(root.join("spaces/Other")).unwrap();fs::set_permissions(root.join("spaces/Other"),fs::Permissions::from_mode(0o500)).unwrap();},
                _ => {candidate=source.clone();let p=root.join(read_mirror_manifest(&root).unwrap().into_iter().next().unwrap());fs::set_permissions(p,fs::Permissions::from_mode(0o400)).unwrap();},
            }
            let before=tree(&root);assert!(write_vault_snapshot_at(&root,"none",&candidate).is_err(),"{mode}");assert_eq!(tree(&root),before);assert_eq!(fs::read(outside.join("sentinel")).unwrap(),b"outside");
            if mode=="directory-permission" {fs::set_permissions(root.join("spaces/Other"),fs::Permissions::from_mode(0o700)).unwrap();}
            fs::remove_dir_all(root).unwrap();fs::remove_dir_all(outside).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn import_cycle_symlink_refuses_without_partial_scan_or_snapshot_mutation() {
        use std::os::unix::fs::symlink;
        let root=root();fs::write(root.join("good.md"),b"good").unwrap();symlink(&root,root.join("loop")).unwrap();
        let before=tree(&root);let mut files=vec![PathBuf::from("already-retained.md")];assert!(collect_markdown_files(&root,&mut files).is_err());assert_eq!(files,vec![PathBuf::from("already-retained.md")]);assert_eq!(tree(&root),before);fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod page_tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    fn root()->PathBuf {let path=std::env::temp_dir().join(format!("starlane-deep-pages-{}-{}",std::process::id(),now_nanos().unwrap()));fs::create_dir(&path).unwrap();path}
    fn snapshot(depth:usize)->Value {
        json!({"schema_version":1,"spaces":[{"id":"s","name":"S"}],"folders":[],
            "documents":(0..depth).map(|i|json!({"id":format!("p{i}"),"space_id":"s","title":format!("Page {:05}",depth-i),"content":format!("  Body {i} Ω\r\n[asset](asset://kept)\n<!-- starlane:custom-css -->\r\np {{ color: red; }}\r\n<!-- /starlane:custom-css -->\n\n"),"parent_id":if i==0 {Value::Null} else {json!(format!("p{}",i-1))}})).collect::<Vec<_>>(),
            "drafts":[{"id":"d","document_id":format!("p{}",depth-1),"space_id":"s","title":"Saved private draft","content":"unsaved-to-readers body\r\n"}],"templates":[],"assets":[],"raw_history":{"revision":"retain"}})
    }
    fn by_id(rows:&Value)->BTreeMap<String,Value> {rows.as_array().unwrap().iter().map(|row|(row["id"].as_str().unwrap().to_string(),row.clone())).collect()}
    #[test]
    fn deep_page_native_snapshot_mirror_reconcile_and_mirror_only_reconstruction_keep_parent_and_body() {
        for depth in [4,32,256] {
            let root=root();let source=snapshot(depth);write_vault_snapshot_at(&root,"none",&source).unwrap();
            let read:Value=serde_json::from_slice(&fs::read(root.join(snapshot_file_name("none"))).unwrap()).unwrap();assert_eq!(read,source);
            let paths=read_mirror_manifest(&root).unwrap();assert_eq!(paths.len(),depth+1);
            let reconciled=reconcile_filesystem_mirror_snapshot(&root,source.clone()).unwrap();assert_eq!(reconciled.documents_updated,0);assert_eq!(reconciled.snapshot,source);
            // Copy only generated document files, with no JSON/manifest. Reverse
            // titles put deepest child first; admission waits for all candidates.
            let target=root.with_extension("reconstructed");fs::create_dir(&target).unwrap();
            for path in paths.iter().filter(|path|path.contains("/documents/")) {let p=target.join(path);fs::create_dir_all(p.parent().unwrap()).unwrap();fs::copy(root.join(path),p).unwrap();}
            let empty=json!({"schema_version":1,"spaces":source["spaces"],"folders":[],"documents":[],"drafts":[],"templates":[],"assets":[]});
            let rebuilt=reconcile_filesystem_mirror_snapshot(&target,empty).unwrap();assert_eq!(rebuilt.documents_imported,depth);
            let original=by_id(&source["documents"]);let rebuilt=by_id(&rebuilt.snapshot["documents"]);
            for (id,row) in original {let actual=&rebuilt[&id];assert_eq!(actual["parent_id"],row["parent_id"]);assert_eq!(actual["content"],row["content"]);assert_eq!(actual["space_id"],row["space_id"]);}
            assert_eq!(reconciled.snapshot["drafts"],source["drafts"]);assert_eq!(reconciled.snapshot["raw_history"],source["raw_history"]);
            fs::remove_dir_all(root).unwrap();fs::remove_dir_all(target).unwrap();
        }
    }
    // Reproduce the old generator: no parent_id field and trim() on body.
    fn legacy_generated_mirror(item:&Value,title_key:&str)->String {
        use std::fmt::Write as _;
        let mut output=String::from("---\n");
        for key in ["id",title_key,"space_id","folder_id","created_by","last_updated_by","created_at","updated_at","owner_id","previous_folder_id","previous_parent_id","starlane_lifecycle"] {
            if let Some(value)=value_str(item,key) {writeln!(output,"{key}: {}",yaml_string(value)).unwrap();}
        }
        output.push_str("---\n\n");output.push_str(value_str(item,"content").unwrap_or(""));output.push('\n');output
    }
    #[test]
    fn old_generated_page_draft_and_template_mirrors_retain_exact_original_whitespace_on_upgrade() {
        for raw in ["  Original Ω\r\n<!-- retained -->\n\n", "\r\nOriginal Ω\r\n\r\n", " \t\r\n  \t"] {
            let root=root();let mut source=snapshot(4);
            for page in source["documents"].as_array_mut().unwrap() {page["content"]=json!(raw);}
            source["drafts"][0]["content"]=json!(raw);
            source["templates"]=json!([{"id":"t","space_id":"s","name":"Original template","content":raw}]);
            write_vault_snapshot_at(&root,"none",&source).unwrap();
            for path in read_mirror_manifest(&root).unwrap() {
                let contents=fs::read_to_string(root.join(&path)).unwrap();let(meta,_)=parse_markdown_mirror_file(&contents).unwrap();
                let collection=if path.contains("/documents/") {"documents"} else if path.contains("/drafts/") {"drafts"} else {"templates"};
                let row=source[collection].as_array().unwrap().iter().find(|row|row["id"]==meta["id"]).unwrap();
                let old=legacy_generated_mirror(row,if collection=="templates" {"name"} else {"title"});
                assert!(!old.contains("\nparent_id:"));let(_,projected)=parse_markdown_mirror_file(&old).unwrap();assert_eq!(projected,raw.trim());
                fs::write(root.join(path),old).unwrap();
            }
            let canonical=fs::read(root.join(snapshot_file_name("none"))).unwrap();
            let reconciled=reconcile_filesystem_mirror_snapshot(&root,source.clone()).unwrap();
            assert_eq!(reconciled.documents_updated,0);assert_eq!(reconciled.templates_updated,0);assert_eq!(reconciled.snapshot,source);
            assert_eq!(fs::read(root.join(snapshot_file_name("none"))).unwrap(),canonical);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn genuinely_changed_legacy_bodies_apply_but_new_parent_metadata_compares_whitespace_exactly() {
        let root=root();let mut source=snapshot(4);let raw="  Original Ω\r\n\n";
        source["documents"][3]["content"]=json!(raw);source["drafts"][0]["content"]=json!(raw);
        source["templates"]=json!([{"id":"t","space_id":"s","name":"Template","content":raw}]);
        write_vault_snapshot_at(&root,"none",&source).unwrap();
        for path in read_mirror_manifest(&root).unwrap() {
            let contents=fs::read_to_string(root.join(&path)).unwrap();let(meta,_)=parse_markdown_mirror_file(&contents).unwrap();let id=&meta["id"];
            if id!="p3" && id!="t" {continue;}
            let collection=if id=="t" {"templates"} else {"documents"};let title_key=if id=="t" {"name"} else {"title"};
            let row=source[collection].as_array().unwrap().iter().find(|row|row["id"]==*id).unwrap();
            let old=legacy_generated_mirror(row,title_key).replace(raw.trim(),"  Edited external Ω\r\n\n");fs::write(root.join(path),old).unwrap();
        }
        let changed=reconcile_filesystem_mirror_snapshot(&root,source.clone()).unwrap();
        assert_eq!(changed.documents_updated,1);assert_eq!(changed.templates_updated,1);
        assert_eq!(changed.snapshot["documents"][3]["content"],"  Edited external Ω\r\n\n");assert_eq!(changed.snapshot["templates"][0]["content"],"  Edited external Ω\r\n\n");
        // Draft mirrors are not an external-edit import surface. Exercise the
        // shared composition rule without adding a new draft import authority.
        let legacy=legacy_generated_mirror(&source["drafts"][0],"title");let(meta,body)=parse_markdown_mirror_file(&legacy).unwrap();let mut draft=source.clone();
        assert!(!reconcile_snapshot_collection(&mut draft,"drafts","d","title","content",&meta,&body));assert_eq!(draft["drafts"][0]["content"],raw);
        assert!(reconcile_snapshot_collection(&mut draft,"drafts","d","title","content",&meta,"Changed draft body"));assert_eq!(draft["drafts"][0]["content"],"Changed draft body");
        // A new-format explicit parent field removes the legacy trim ambiguity.
        let mut exact=source.clone();let mut modern=meta;modern.insert("parent_id".into(),String::new());
        assert!(reconcile_snapshot_collection(&mut exact,"drafts","d","title","content",&modern,raw.trim()));assert_eq!(exact["drafts"][0]["content"],raw.trim());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_missing_parent_metadata_retains_canonical_edge_but_explicit_empty_can_clear_it() {
        let root=root();let source=snapshot(4);write_vault_snapshot_at(&root,"none",&source).unwrap();
        let paths=read_mirror_manifest(&root).unwrap();let path=paths.iter().find(|path|path.ends_with("-p3.md")).unwrap();let file=root.join(path);let raw=fs::read_to_string(&file).unwrap();
        let legacy=raw.split_inclusive('\n').filter(|line|!line.starts_with("parent_id:")).collect::<String>();fs::write(&file,legacy).unwrap();
        let retained=reconcile_filesystem_mirror_snapshot(&root,source.clone()).unwrap();assert_eq!(retained.snapshot["documents"][3]["parent_id"],"p2");
        fs::write(&file,raw.replace("parent_id: \"p2\"","parent_id: \"\"")).unwrap();
        let cleared=reconcile_filesystem_mirror_snapshot(&root,source.clone()).unwrap();assert!(cleared.snapshot["documents"][3]["parent_id"].is_null());
        assert_eq!(cleared.snapshot["documents"][3]["content"],source["documents"][3]["content"]);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_parent_graph_refusals_preserve_canonical_json_mirrors_and_queue_before_any_write() {
        for mode in ["cycle","missing","foreign-space","foreign-folder","duplicate"] {
            let root=root();let source=snapshot(4);write_vault_snapshot_at(&root,"none",&source).unwrap();
            let before=fs::read(root.join(snapshot_file_name("none"))).unwrap();let manifest=fs::read(root.join(MIRROR_MANIFEST_FILE)).unwrap();
            let old_files=read_mirror_manifest(&root).unwrap().iter().map(|path|(path.clone(),fs::read(root.join(path)).unwrap())).collect::<BTreeMap<_,_>>();let mut candidate=source.clone();
            match mode {
                "cycle"=>candidate["documents"][0]["parent_id"]=json!("p3"),
                "missing"=>candidate["documents"][3]["parent_id"]=json!("unavailable"),
                "foreign-space"=>candidate["documents"][0]["space_id"]=json!("foreign"),
                "foreign-folder"=>candidate["documents"][0]["folder_id"]=json!("foreign-folder"),
                _=>{let duplicate=candidate["documents"][0].clone();candidate["documents"].as_array_mut().unwrap().push(duplicate);},
            }
            assert!(write_vault_snapshot_at(&root,"none",&candidate).is_err(),"{mode}");assert_eq!(fs::read(root.join(snapshot_file_name("none"))).unwrap(),before);assert_eq!(fs::read(root.join(MIRROR_MANIFEST_FILE)).unwrap(),manifest);for (path,bytes) in old_files {assert_eq!(fs::read(root.join(path)).unwrap(),bytes);}
            // An invalid metadata edit is retained on disk for review, never installed.
            let path=read_mirror_manifest(&root).unwrap().into_iter().find(|path|path.ends_with("-p3.md")).unwrap();let file=root.join(path);let raw=fs::read_to_string(&file).unwrap();let edited=raw.replace("parent_id: \"p2\"","parent_id: \"p3\"");fs::write(&file,&edited).unwrap();assert!(reconcile_filesystem_mirror_snapshot(&root,source.clone()).is_err());assert_eq!(fs::read(root.join(snapshot_file_name("none"))).unwrap(),before);assert_eq!(fs::read_to_string(file).unwrap(),edited);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn archive_inspection_refuses_invalid_page_graph_before_destination_creation_and_keeps_original() {
        use std::io::Write;
        let _guard=archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
        for mode in ["cycle","missing","foreign"] {
            let root=root();let mut source=snapshot(4);match mode {"cycle"=>source["documents"][0]["parent_id"]=json!("p3"),"missing"=>source["documents"][3]["parent_id"]=json!("missing"),_=>source["documents"][0]["space_id"]=json!("foreign")};
            let path=root.join("source.zip");let mut zip=zip::ZipWriter::new(fs::File::create(&path).unwrap());zip.start_file(ARCHIVE_SNAPSHOT_FILE,zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(source.to_string().as_bytes()).unwrap();zip.finish().unwrap();let before=fs::read(&path).unwrap();
            let destination=root.join("absent-vault/assets");assert!(archive_admission::prepare("none",&path,&destination).is_err());assert!(!root.join("absent-vault").exists());assert_eq!(fs::read(&path).unwrap(),before);fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn page_ancestry_budget_allows_10000_and_unrelated_roots_but_refuses_one_more_involved_parent() {
        let mut source=snapshot(10_000);assert!(validate_page_hierarchy(&source).is_ok());source["documents"].as_array_mut().unwrap().push(json!({"id":"over","space_id":"s","parent_id":"p9999"}));assert!(validate_page_hierarchy(&source).unwrap_err().contains("10000"));
        source["documents"]=json!((0..10_001).map(|i|json!({"id":format!("root{i}"),"space_id":"s"})).collect::<Vec<_>>());assert!(validate_page_hierarchy(&source).is_ok());
    }
}


#[cfg(test)]
mod mixed_hierarchy_tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    fn fixture()->PathBuf {
        let root=std::env::temp_dir().join(format!("starlane-mixed-native-{}-{}",std::process::id(),now_nanos().unwrap()));
        fs::create_dir(&root).unwrap();root
    }
    fn source()->Value {
        let folders=(0..32).map(|i|json!({"id":format!("f{i}"),"space_id":"s","name":format!("Folder{i}"),"parent_id":if i==0 {Value::Null}else{json!(format!("f{}",i-1))}})).collect::<Vec<_>>();
        let mut pages=(0..32).map(|i|json!({"id":format!("p{i}"),"space_id":"s","folder_id":"f31","title":format!("Page{i}"),"parent_id":if i==0 {Value::Null}else{json!(format!("p{}",i-1))},"content":format!("  Exact page {i} Ω\r\n[asset](asset://retained)\n<!-- starlane:custom-css -->\r\np {{ color: red; }}\r\n<!-- /starlane:custom-css -->\n\n"),"future":{"history_reference":"opaque retained reference, not server history"}})).collect::<Vec<_>>();
        pages.extend((0..32).map(|i|json!({"id":format!("sibling{i}"),"space_id":"s","folder_id":"f31","title":format!("Sibling{i}"),"parent_id":"p31","content":format!("Sibling {i} exact\r\n\n")})));
        json!({"schema_version":1,"spaces":[{"id":"s","name":"Mixed"}],"folders":folders,"documents":pages,
            "drafts":[{"id":"private-draft","document_id":"p31","space_id":"s","owner_id":"local-user","title":"Private saved work","content":"  Private Ω\r\n\n"}],
            "templates":[{"id":"template","space_id":"s","name":"Retained template","content":"  Template Ω\r\n\n"}],
            "assets":[{"id":"reference","space_id":"s","document_id":"p0","source":"browser","name":"retained","url":"asset://retained","size":0}],
            "sync_state":{"state":"idle","queued_operations":[{"id":"original-queued","entity_type":"document","entity_id":"p31","action":"update_content","queued_at":"2026-10-15T12:00:00Z","payload":{"content":"original queued body"}}]}})
    }
    fn by_id(rows:&Value)->BTreeMap<String,Value> {rows.as_array().unwrap().iter().map(|row|(row["id"].as_str().unwrap().into(),row.clone())).collect()}
    fn tree(root:&Path)->BTreeMap<PathBuf,Vec<u8>> {
        let mut result=BTreeMap::new();let mut pending=vec![root.to_path_buf()];
        while let Some(dir)=pending.pop(){for entry in fs::read_dir(dir).unwrap(){let entry=entry.unwrap();let path=entry.path();if entry.file_type().unwrap().is_dir(){result.insert(path.clone(),vec![]);pending.push(path);}else{result.insert(path.clone(),fs::read(path).unwrap());}}}result
    }
    #[test]
    fn mixed_32folder_32page_32sibling_json_markdown_and_target_only_lifecycle_projection() {
        let root=fixture();let source=source();write_vault_snapshot_at(&root,"none",&source).unwrap();
        assert_eq!(read_mirror_manifest(&root).unwrap().len(),66);
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(root.join(snapshot_file_name("none"))).unwrap()).unwrap(),source);
        let original=by_id(&source["documents"]);
        // These are native projection states, not a new lifecycle mutation API.
        for state in ["archived","active","trashed","active"] {
            let mut candidate=source.clone();let target=&mut candidate["documents"][15];
            if state=="archived"{target["archived_at"]=json!("2026-10-15T12:00:00Z");}
            if state=="trashed"{target["deleted_at"]=json!("2026-10-15T12:01:00Z");}
            write_vault_snapshot_at(&root,"none",&candidate).unwrap();
            let read:Value=serde_json::from_slice(&fs::read(root.join(snapshot_file_name("none"))).unwrap()).unwrap();assert_eq!(read,candidate);
            let manifest=read_mirror_manifest(&root).unwrap();let path=manifest.iter().find(|path|path.ends_with("-p15.md")).unwrap();
            assert!(path.contains(match state{"archived"=>"/archived/","trashed"=>"/trashed/",_=>"/documents/"}));
            let(meta,body)=parse_markdown_mirror_file(&fs::read_to_string(root.join(path)).unwrap()).unwrap();assert_eq!(meta["parent_id"],"p14");assert_eq!(meta["folder_id"],"f31");assert_eq!(body,original["p15"]["content"].as_str().unwrap());
            let reconciled=reconcile_filesystem_mirror_snapshot(&root,candidate.clone()).unwrap();assert_eq!(reconciled.snapshot,candidate);
            for(id,row)in by_id(&read["documents"]){if id!="p15"{assert_eq!(row,original[&id]);}}
            assert_eq!(read["drafts"],source["drafts"]);assert_eq!(read["assets"],source["assets"]);assert_eq!(read["sync_state"],source["sync_state"]);
        }
        // Rebuild only published page files with the original folder catalog;
        // absent draft/template/asset bytes are not invented by Markdown import.
        let rebuilt_root=root.with_extension("rebuilt");fs::create_dir(&rebuilt_root).unwrap();
        for path in read_mirror_manifest(&root).unwrap().iter().filter(|path|path.contains("/documents/")) {let target=rebuilt_root.join(path);fs::create_dir_all(target.parent().unwrap()).unwrap();fs::copy(root.join(path),target).unwrap();}
        let rebuilt=reconcile_filesystem_mirror_snapshot(&rebuilt_root,json!({"schema_version":1,"spaces":source["spaces"],"folders":source["folders"],"documents":[],"drafts":[],"templates":[],"assets":[]})).unwrap();
        assert_eq!(rebuilt.documents_imported,64);let pages=by_id(&rebuilt.snapshot["documents"]);
        for(id,row)in original{for key in ["id","space_id","folder_id","parent_id","title","content"]{assert_eq!(pages[&id][key],row[key],"{id}/{key}");}}
        assert_eq!(rebuilt.snapshot["folders"],source["folders"]);assert_eq!(rebuilt.snapshot["drafts"],json!([]));assert_eq!(rebuilt.snapshot["assets"],json!([]));
        fs::remove_dir_all(rebuilt_root).unwrap();fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mixed_candidate_graph_path_and_budget_refusals_leave_all_existing_bytes_unchanged() {
        let root=fixture();let source=source();write_vault_snapshot_at(&root,"none",&source).unwrap();let before=tree(&root);
        let mut modes=vec!["cycle","missing","foreign-space","foreign-folder","duplicate","page-budget"];
        if cfg!(unix){modes.push("native-path");}
        for mode in modes {
            let mut candidate=source.clone();match mode {
                "cycle"=>candidate["documents"][0]["parent_id"]=json!("p31"),
                "missing"=>candidate["documents"][15]["parent_id"]=json!("absent"),
                "foreign-space"=>candidate["documents"][15]["space_id"]=json!("foreign"),
                "foreign-folder"=>candidate["documents"][15]["folder_id"]=json!("f30"),
                "duplicate"=>{let row=candidate["documents"][0].clone();candidate["documents"].as_array_mut().unwrap().push(row);},
                "page-budget"=>candidate["documents"]=json!((0..10001).map(|i|json!({"id":format!("budget{i}"),"space_id":"s","folder_id":"f31","parent_id":if i==0{Value::Null}else{json!(format!("budget{}",i-1))}})).collect::<Vec<_>>()),
                _=>{candidate["folders"]=json!((0..64).map(|i|json!({"id":format!("f{i}"),"space_id":"s","name":"x".repeat(80),"parent_id":if i==0{Value::Null}else{json!(format!("f{}",i-1))}})).collect::<Vec<_>>());},
            }
            assert!(write_vault_snapshot_at(&root,"none",&candidate).is_err(),"{mode}");assert_eq!(tree(&root),before,"{mode}");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
