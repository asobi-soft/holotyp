use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Message;

pub const DEFLATE_LEVEL: u8 = 6;
/// Large SVG/image snapshots favor latency over the last few percent of compression.
pub const FAST_DEFLATE_THRESHOLD: usize = 64 * 1024;
pub const FAST_DEFLATE_LEVEL: u8 = 1;

/// The wire frame: one deflated CBOR map.
///
/// ```text
/// frame = deflate( CBOR { seq: u32, message: Message } )
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    /// Number of the frame within its sender's own stream: each side counts the frames it
    /// sends, starting from its own counter.
    pub seq: u32,
    pub message: Message,
}

impl Frame {
    pub fn new(seq: u32, message: Message) -> Self {
        Self { seq, message }
    }

    /// Encode for the wire: CBOR, then deflate. Nothing else.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut cbor = Vec::new();
        ciborium::into_writer(self, &mut cbor).expect("frame serializes");
        let level = if cbor.len() >= FAST_DEFLATE_THRESHOLD {
            FAST_DEFLATE_LEVEL
        } else {
            DEFLATE_LEVEL
        };
        miniz_oxide::deflate::compress_to_vec(&cbor, level)
    }

    /// Decode from the wire.
    ///
    /// `None` means the bytes are not ours and must be dropped as-is: they do not
    /// inflate, or they inflate to something that is not this frame.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let raw = miniz_oxide::inflate::decompress_to_vec(bytes).ok()?;
        ciborium::from_reader(raw.as_slice()).ok()
    }
}
