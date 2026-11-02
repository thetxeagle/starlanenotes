use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tantivy::{
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{Field, Schema, Value, STORED, STRING, TEXT},
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument,
};

pub struct DocEntry {
    pub doc_id: String,
    pub space_id: String,
    pub title: String,
    pub body: String,
    pub tags: String,
}

pub struct SearchHit {
    pub doc_id: String,
    pub space_id: String,
    pub title: String,
    pub excerpt: String,
    pub score: f32,
}

/// One immutable candidate snapshot per request; SQL remains the authority.
pub(crate) struct SearchSnapshot {
    searcher: tantivy::Searcher,
    query: Box<dyn tantivy::query::Query>,
    inner: Arc<SearchIndexInner>,
}
pub(crate) struct SearchCandidate {
    pub doc_id: String,
    pub space_id: String,
    pub title: String,
    pub body: String,
    pub tags: String,
}
impl SearchSnapshot {
    pub(crate) fn page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<SearchCandidate>, anyhow::Error> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let hits = self
            .searcher
            .search(&self.query, &TopDocs::with_limit(limit).and_offset(offset))?;
        hits.into_iter()
            .map(|(_, address)| {
                let doc: TantivyDocument = self.searcher.doc(address)?;
                let value = |field| {
                    doc.get_first(field)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                };
                Ok(SearchCandidate {
                    doc_id: value(self.inner.f_doc_id),
                    space_id: value(self.inner.f_space_id),
                    title: value(self.inner.f_title),
                    body: value(self.inner.f_body),
                    tags: value(self.inner.f_tags),
                })
            })
            .collect()
    }
}
pub(crate) fn excerpt(body: &str, limit: usize) -> String {
    let mut chars = body.chars();
    let prefix: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{prefix}...")
    } else {
        prefix
    }
}

#[derive(Clone)]
pub struct SearchIndex {
    inner: Arc<SearchIndexInner>,
}

struct SearchIndexInner {
    index: Index,
    reader: IndexReader,
    writer: RwLock<IndexWriter>,
    f_doc_id: Field,
    f_space_id: Field,
    f_title: Field,
    f_body: Field,
    f_tags: Field,
}

impl SearchIndex {
    /// Initialize the search index at the given path.
    /// Creates the index if it doesn't exist.
    pub fn new(index_path: PathBuf) -> Result<Self, anyhow::Error> {
        std::fs::create_dir_all(&index_path)?;

        let mut schema_builder = Schema::builder();
        let f_doc_id = schema_builder.add_text_field("doc_id", STRING | STORED);
        let f_space_id = schema_builder.add_text_field("space_id", STRING | STORED);
        let f_title = schema_builder.add_text_field("title", TEXT | STORED);
        let f_body = schema_builder.add_text_field("body", TEXT | STORED);
        let f_tags = schema_builder.add_text_field("tags", TEXT | STORED);
        let schema = schema_builder.build();

        let index = match Index::open_in_dir(&index_path) {
            Ok(existing) => {
                // If schema changed, recreate the index
                if existing.schema() != schema {
                    tracing::info!("Search index schema changed, recreating...");
                    std::fs::remove_dir_all(&index_path)?;
                    std::fs::create_dir_all(&index_path)?;
                    Index::create_in_dir(&index_path, schema.clone())?
                } else {
                    existing
                }
            }
            Err(_) => Index::create_in_dir(&index_path, schema.clone())?,
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let writer = index.writer(50_000_000)?; // 50MB heap

        Ok(Self {
            inner: Arc::new(SearchIndexInner {
                index,
                reader,
                writer: RwLock::new(writer),
                f_doc_id,
                f_space_id,
                f_title,
                f_body,
                f_tags,
            }),
        })
    }

    /// Index or update a single document
    pub fn upsert(&self, entry: DocEntry) -> Result<(), anyhow::Error> {
        let i = &self.inner;
        let mut writer = i.writer.write().map_err(|e| anyhow::anyhow!("{}", e))?;

        // Delete existing doc with this ID
        let term = tantivy::Term::from_field_text(i.f_doc_id, &entry.doc_id);
        writer.delete_term(term);

        // Add updated doc
        writer.add_document(doc!(
            i.f_doc_id => entry.doc_id,
            i.f_space_id => entry.space_id,
            i.f_title => entry.title,
            i.f_body => entry.body,
            i.f_tags => entry.tags,
        ))?;

        writer.commit()?;
        Ok(())
    }

    /// Delete a document from the index
    pub fn delete(&self, doc_id: &str) -> Result<(), anyhow::Error> {
        let i = &self.inner;
        let mut writer = i.writer.write().map_err(|e| anyhow::anyhow!("{}", e))?;

        let term = tantivy::Term::from_field_text(i.f_doc_id, doc_id);
        writer.delete_term(term);
        writer.commit()?;
        Ok(())
    }

    pub(crate) fn snapshot(&self, query_str: &str) -> Result<SearchSnapshot, anyhow::Error> {
        let i = &self.inner;
        i.reader.reload()?;
        let parser = QueryParser::for_index(&i.index, vec![i.f_title, i.f_body, i.f_tags]);
        let query = parser.parse_query(query_str).unwrap_or_else(|_| {
            Box::new(tantivy::query::TermQuery::new(
                tantivy::Term::from_field_text(i.f_title, query_str),
                tantivy::schema::IndexRecordOption::Basic,
            ))
        });
        Ok(SearchSnapshot {
            searcher: i.reader.searcher(),
            query,
            inner: i.clone(),
        })
    }

    /// Search the index
    pub fn search(
        &self,
        query_str: &str,
        space_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, anyhow::Error> {
        let i = &self.inner;
        let searcher = i.reader.searcher();

        let query_parser = QueryParser::for_index(&i.index, vec![i.f_title, i.f_body, i.f_tags]);
        let query = query_parser.parse_query(query_str).unwrap_or_else(|_| {
            // Fall back to simple term query if parsing fails
            Box::new(tantivy::query::TermQuery::new(
                tantivy::Term::from_field_text(i.f_title, query_str),
                tantivy::schema::IndexRecordOption::Basic,
            ))
        });

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit * 2))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let doc_id = doc
                .get_first(i.f_doc_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let sid = doc
                .get_first(i.f_space_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = doc
                .get_first(i.f_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = doc
                .get_first(i.f_body)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Filter by space_id if specified
            if let Some(filter_space) = space_id {
                if sid != filter_space {
                    continue;
                }
            }

            // Generate excerpt — first 200 chars of body
            let excerpt = excerpt(&body, 200);

            results.push(SearchHit {
                doc_id,
                space_id: sid,
                title,
                excerpt,
                score,
            });

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// Publish a replacement snapshot with a single commit.
    pub fn rebuild(&self, entries: Vec<DocEntry>) -> Result<(), anyhow::Error> {
        self.rebuild_inner(entries, || Ok(()))
    }

    fn rebuild_inner(
        &self,
        entries: Vec<DocEntry>,
        before_commit: impl FnOnce() -> Result<(), anyhow::Error>,
    ) -> Result<(), anyhow::Error> {
        let i = &self.inner;
        let mut writer = i.writer.write().map_err(|e| anyhow::anyhow!("{}", e))?;
        let staged = (|| -> Result<(), anyhow::Error> {
            writer.delete_all_documents()?;
            for entry in entries {
                writer.add_document(doc!(
                    i.f_doc_id => entry.doc_id,
                    i.f_space_id => entry.space_id,
                    i.f_title => entry.title,
                    i.f_body => entry.body,
                    i.f_tags => entry.tags,
                ))?;
            }
            before_commit()?;
            writer.commit()?;
            Ok(())
        })();
        if let Err(error) = staged {
            writer.rollback().map_err(|rollback| anyhow::anyhow!(
                "Rebuild failed: {error}; rollback failed: {rollback}"
            ))?;
            return Err(error);
        }
        i.reader.reload()?;
        tracing::info!("Search index rebuilt successfully");
        Ok(())
    }
}

#[cfg(test)]
mod rebuild_tests {
    use super::*;
    fn entry(id: &str, body: &str) -> DocEntry {
        DocEntry { doc_id: id.into(), space_id: "space".into(), title: id.into(), body: body.into(), tags: String::new() }
    }
    #[test]
    fn rebuild_is_immediately_visible_and_survives_reopen() {
        let path = std::env::temp_dir().join(format!("search-rebuild-{}", uuid::Uuid::new_v4()));
        let index = SearchIndex::new(path.clone()).unwrap();
        index.rebuild(vec![entry("old", "previous")]).unwrap();
        index.rebuild(vec![entry("new", "nebula")]).unwrap();
        assert_eq!(index.search("nebula", None, 10).unwrap()[0].doc_id, "new");
        assert!(index.search("previous", None, 10).unwrap().is_empty());
        drop(index);
        let reopened = SearchIndex::new(path.clone()).unwrap();
        assert_eq!(reopened.search("nebula", None, 10).unwrap()[0].doc_id, "new");
        drop(reopened);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn interrupted_staged_rebuild_preserves_committed_snapshot_and_future_writes() {
        let path = std::env::temp_dir().join(format!("search-rebuild-{}", uuid::Uuid::new_v4()));
        let index = SearchIndex::new(path.clone()).unwrap();
        index.rebuild(vec![entry("old", "previous")]).unwrap();
        assert!(index.rebuild_inner(vec![entry("new", "nebula")], || {
            assert_eq!(index.search("previous", None, 10).unwrap()[0].doc_id, "old");
            assert!(index.search("nebula", None, 10).unwrap().is_empty());
            Err(anyhow::anyhow!("injected precommit interruption"))
        }).is_err());
        assert_eq!(index.search("previous", None, 10).unwrap()[0].doc_id, "old");
        drop(index);
        let index = SearchIndex::new(path.clone()).unwrap();
        assert_eq!(index.search("previous", None, 10).unwrap()[0].doc_id, "old");
        index.rebuild(vec![entry("retry", "retrytoken")]).unwrap();
        assert!(index.search("nebula", None, 10).unwrap().is_empty());
        assert_eq!(index.search("retrytoken", None, 10).unwrap()[0].doc_id, "retry");
        drop(index);
        let reopened = SearchIndex::new(path.clone()).unwrap();
        assert!(reopened.search("nebula", None, 10).unwrap().is_empty());
        assert_eq!(reopened.search("retrytoken", None, 10).unwrap()[0].doc_id, "retry");
        drop(reopened);
        std::fs::remove_dir_all(path).unwrap();
    }
}
