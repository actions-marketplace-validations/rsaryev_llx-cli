//! A picture as a model takes it: a PNG, JPEG, GIF or WebP known by its first bytes, its size
//! read from its header (the file is not decoded), encoded as base64. The one place that knows
//! these formats: the app reads what the person attaches with it.

use base64::Engine as _;
use contracts::tool::Image;

/// The largest picture sent to a model, in bytes.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy)]
enum Format {
    Png,
    Jpeg,
    Gif,
    Webp,
}

/// The picture `bytes` hold, ready for a model; `None` when they are not a PNG, JPEG, GIF or
/// WebP whose header gives its size.
#[must_use]
pub fn image(bytes: &[u8]) -> Option<Image> {
    let format = Format::detect(bytes)?;
    let (width, height) = format.dimensions(bytes)?;
    Some(Image {
        media_type: format.media_type().into(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
        width,
        height,
    })
}

impl Format {
    fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            Some(Self::Webp)
        } else {
            None
        }
    }

    fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }

    fn dimensions(self, bytes: &[u8]) -> Option<(u32, u32)> {
        match self {
            Self::Png if bytes.get(12..16)? == b"IHDR" => {
                Some((be32(bytes.get(16..20)?)?, be32(bytes.get(20..24)?)?))
            }
            Self::Gif => Some((le16(bytes.get(6..8)?)?, le16(bytes.get(8..10)?)?)),
            Self::Jpeg => jpeg_dimensions(bytes),
            Self::Webp => webp_dimensions(bytes),
            Self::Png => None,
        }
        .filter(|&(width, height)| width > 0 && height > 0)
    }
}

fn le16(bytes: &[u8]) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(bytes.try_into().ok()?)))
}

fn be16(bytes: &[u8]) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(bytes.try_into().ok()?)))
}

fn le24(bytes: &[u8]) -> Option<u32> {
    let [a, b, c]: [u8; 3] = bytes.try_into().ok()?;
    Some(u32::from_le_bytes([a, b, c, 0]))
}

fn be32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

/// The size in the first start-of-frame segment, walking the segments before it.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 2;
    while pos < bytes.len() {
        if *bytes.get(pos)? != 0xff {
            return None;
        }
        // Fill bytes: any number of 0xff before a marker.
        while *bytes.get(pos)? == 0xff {
            pos += 1;
        }
        let marker = *bytes.get(pos)?;
        pos += 1;
        // The end of the image, or the start of its data: no frame header came first.
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        // Markers without a length: TEM and the restarts.
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        let length = usize::try_from(be16(bytes.get(pos..pos + 2)?)?).ok()?;
        if length < 2 || pos.checked_add(length)? > bytes.len() {
            return None;
        }
        // SOF0 to SOF15, less DHT (c4), JPG (c8) and DAC (cc).
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if length < 7 {
                return None;
            }
            return Some((
                be16(bytes.get(pos + 5..pos + 7)?)?,
                be16(bytes.get(pos + 3..pos + 5)?)?,
            ));
        }
        pos += length;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match bytes.get(12..16)? {
        b"VP8X" => Some((1 + le24(bytes.get(24..27)?)?, 1 + le24(bytes.get(27..30)?)?)),
        b"VP8L" if *bytes.get(20)? == 0x2f => {
            let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8 " if bytes.get(23..26)? == b"\x9d\x01\x2a" => Some((
            le16(bytes.get(26..28)?)? & 0x3fff,
            le16(bytes.get(28..30)?)? & 0x3fff,
        )),
        _ => None,
    }
}

#[cfg(test)]
#[path = "picture_tests.rs"]
mod tests;
