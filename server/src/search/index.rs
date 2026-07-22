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
            let excerpt = if body.len() > 200 {
                format!("{}...", &body[..200])
            } else {
                body
            };

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

    /// Rebuild the entire index from scratch
    pub fn rebuild(&self, entries: Vec<DocEntry>) -> Result<(), anyhow::Error> {
        let i = &self.inner;
        let mut writer = i.writer.write().map_err(|e| anyhow::anyhow!("{}", e))?;

        // Clear the index
        writer.delete_all_documents()?;
        writer.commit()?;

        // Re-add all documents
        for entry in entries {
            writer.add_document(doc!(
                i.f_doc_id => entry.doc_id,
                i.f_space_id => entry.space_id,
                i.f_title => entry.title,
                i.f_body => entry.body,
                i.f_tags => entry.tags,
            ))?;
        }

        writer.commit()?;
        tracing::info!("Search index rebuilt successfully");
        Ok(())
    }
}
