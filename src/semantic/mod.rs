use anyhow::Result;
use std::sync::Arc;

use crate::db::Database;

pub struct SemanticResult {
    pub file_path: String,
    pub snippet: String,
    pub score: f32,
}

pub struct SemanticEngine {
    model: fastembed::TextEmbedding,
}

impl SemanticEngine {
    pub fn new() -> Result<Self> {
        let model = fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(fastembed::EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(true),
        )?;
        Ok(Self { model })
    }

    pub fn embed(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let embeddings = self.model.embed(texts.to_vec(), None)?;
        Ok(embeddings)
    }

    pub async fn embed_and_store(
        &mut self,
        file_path: &str,
        chunks: &[String],
        db: &Arc<Database>,
    ) -> Result<()> {
        let refs: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
        let embeddings = self.embed(&refs)?;

        let db = Arc::clone(db);
        let file_path = file_path.to_string();
        let data: Vec<(String, Vec<f32>)> = chunks
            .iter()
            .zip(embeddings)
            .map(|(chunk, emb)| (chunk.clone(), emb))
            .collect();

        tokio::task::spawn_blocking(move || {
            db.with_conn(|conn| {
                // Delete old embeddings for this file
                conn.execute(
                    "DELETE FROM embeddings WHERE file_id = (SELECT id FROM files WHERE path = ?1)",
                    rusqlite::params![file_path],
                )?;

                for (chunk, embedding) in &data {
                    let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
                    conn.execute(
                        "INSERT INTO embeddings (file_id, chunk_text, embedding)
                         VALUES ((SELECT id FROM files WHERE path = ?1), ?2, ?3)",
                        rusqlite::params![file_path, chunk, blob],
                    )?;
                }
                Ok(())
            })
        })
        .await??;

        Ok(())
    }

    pub async fn search(
        &mut self,
        query: &str,
        limit: usize,
        db: &Arc<Database>,
    ) -> Result<Vec<SemanticResult>> {
        let query_embedding = self.embed(&[query])?;
        let query_vec = query_embedding
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("failed to embed query"))?;

        let db = Arc::clone(db);
        let results = tokio::task::spawn_blocking(move || {
            db.with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT f.path, e.chunk_text, e.embedding
                     FROM embeddings e
                     JOIN files f ON e.file_id = f.id",
                )?;

                let mut scored: Vec<SemanticResult> = Vec::new();

                let rows = stmt.query_map([], |row| {
                    let file_path: String = row.get(0)?;
                    let chunk_text: String = row.get(1)?;
                    let blob: Vec<u8> = row.get(2)?;
                    Ok((file_path, chunk_text, blob))
                })?;

                for row in rows {
                    let (file_path, chunk_text, blob) = row?;
                    let embedding: Vec<f32> = blob
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    let score = cosine_similarity(&query_vec, &embedding);
                    scored.push(SemanticResult {
                        file_path,
                        snippet: chunk_text,
                        score,
                    });
                }

                scored.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                scored.truncate(limit);
                Ok::<_, anyhow::Error>(scored)
            })
        })
        .await??;

        Ok(results)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
