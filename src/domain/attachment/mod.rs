//! Attachment module - file upload and storage handling
//!
//! Currently supports local file storage. S3/MinIO support planned for future.

mod error;
mod local_storage;
mod service;
mod storage;
mod thumbnail;
mod types;

pub use error::AttachmentError;
pub use local_storage::LocalFileStorage;
pub use service::AttachmentService;
pub use storage::FileStorage;
pub use thumbnail::ThumbnailGenerator;
pub use types::{Attachment, AttachmentResponse, StorageInfo, UploadRequest};
