//! Validate and encode a health batch before acquiring its SQLite write transaction.
use crate::{
    error::AppError,
    health::{HealthRecordInput, SyncRequest},
};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io::Write};

pub const MAX_BATCH_BYTES: usize = 40 * 1024 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct PreparedRecord {
    pub record_id: String,
    pub source_id: String,
    pub record_type: String,
    pub start_at: i64,
    pub end_at: i64,
    pub version: i64,
    pub deleted: bool,
    pub payload_json: String,
}
pub(crate) struct PreparedBatch {
    pub connection_id: String,
    pub batch_id: String,
    pub record_type: String,
    pub coverage_status: String,
    pub records: Vec<PreparedRecord>,
    pub digest: String,
}
struct BoundedDigest {
    bytes: usize,
    limit: usize,
    exceeded: bool,
    hash: Sha256,
}
impl BoundedDigest {
    fn new(limit: usize) -> Self {
        Self {
            bytes: 0,
            limit,
            exceeded: false,
            hash: Sha256::new(),
        }
    }
    fn encode<T: serde::Serialize>(&mut self, value: &T) -> Result<(), AppError> {
        if serde_json::to_writer(&mut *self, value).is_err() {
            return Err(if self.exceeded {
                AppError::TooLarge
            } else {
                AppError::Internal
            });
        }
        Ok(())
    }
}
impl Write for BoundedDigest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            self.exceeded = true;
            return Err(std::io::Error::other("JSON limit exceeded"));
        }
        self.bytes += bytes.len();
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn prepare(request: SyncRequest) -> Result<PreparedBatch, AppError> {
    if uuid::Uuid::parse_str(&request.batch_id).is_err()
        || request.records.len() > 500
        || request.record_type.is_empty()
        || request.record_type.len() > 128
        || !["observed", "no_visible_samples", "error", "unsupported"]
            .contains(&request.coverage_status.as_str())
    {
        return Err(AppError::Invalid("Invalid sync batch"));
    }
    // Streaming serialization computes the existing canonical digest without a second
    // full 40 MiB batch allocation. No raw field is normalized or removed from the digest.
    let mut batch_digest = BoundedDigest::new(MAX_BATCH_BYTES);
    batch_digest.encode(&request)?;
    let digest = hex::encode(batch_digest.hash.finalize());
    let mut seen = HashSet::new();
    let mut records = Vec::with_capacity(request.records.len());
    for record in request.records {
        if record.record_id.is_empty()
            || record.record_id.len() > 256
            || record.source_id.is_empty()
            || record.source_id.len() > 512
            || (!record.deleted && record.source_id == "*")
            || record.record_type != request.record_type
            || record.version < 1
            || record.end_at < record.start_at
            || !record.payload.is_object()
            || !seen.insert((record.source_id.clone(), record.record_id.clone()))
        {
            return Err(AppError::Invalid("Invalid health record"));
        }
        BoundedDigest::new(MAX_PAYLOAD_BYTES).encode(&record.payload)?;
        let payload_json = if record.deleted {
            "{}".to_owned()
        } else {
            serde_json::to_string(&record)?
        };
        let HealthRecordInput {
            record_id,
            source_id,
            record_type,
            start_at,
            end_at,
            version,
            deleted,
            payload: _,
        } = record;
        records.push(PreparedRecord {
            record_id,
            source_id,
            record_type,
            start_at,
            end_at,
            version,
            deleted,
            payload_json,
        });
    }
    Ok(PreparedBatch {
        connection_id: request.connection_id,
        batch_id: request.batch_id,
        record_type: request.record_type,
        coverage_status: request.coverage_status,
        records,
        digest,
    })
}
