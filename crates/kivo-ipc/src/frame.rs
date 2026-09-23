//! Framing (ARCHITECTURE §3): each message is a little-endian `u32` length followed by that many
//! bytes of JSON. Frames over 1 MiB are refused and close the connection (SECURITY §9).

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub const MAX_FRAME: usize = 1024 * 1024;

pub type Frames<S> = Framed<S, LengthDelimitedCodec>;

pub fn frames<S: AsyncRead + AsyncWrite>(stream: S) -> Frames<S> {
    Framed::new(stream, codec(MAX_FRAME))
}

/// The frame codec (1 MiB limit), for relaying frames byte-for-byte (the browser bridge's native
/// messaging host: Chrome frames messages the same way).
pub fn frame_codec() -> LengthDelimitedCodec {
    codec(MAX_FRAME)
}

pub(crate) fn codec(max: usize) -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_type::<u32>()
        .max_frame_length(max)
        .new_codec()
}
