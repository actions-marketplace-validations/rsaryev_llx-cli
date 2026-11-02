use super::*;

fn size(bytes: &[u8]) -> Option<(&str, u32, u32)> {
    image(bytes).map(|image| {
        let media = match image.media_type.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpeg",
            "image/gif" => "gif",
            _ => "webp",
        };
        (media, image.width, image.height)
    })
}

#[test]
fn a_jpeg_is_measured_past_the_segments_before_its_frame() {
    // SOI, an APP0 segment of 4 bytes, then SOF0 with height 7 and width 9.
    let jpeg = [
        0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8, 0, 7, 0, 9, 3, 0, 0, 0,
    ];
    assert_eq!(size(&jpeg), Some(("jpeg", 9, 7)));
    // The image's data begins before any frame header: no size to give.
    assert_eq!(size(&[0xff, 0xd8, 0xff, 0xda, 0, 2]), None);
}

#[test]
fn what_is_not_a_measurable_picture_is_none() {
    assert_eq!(size(b"%PDF-1.7"), None);
    // A PNG's signature with no header chunk after it.
    assert_eq!(size(b"\x89PNG\r\n\x1a\n\0\0\0\rIEND"), None);
    // A GIF of no width.
    assert_eq!(size(b"GIF89a\0\0\x02\0"), None);
}

/// A WebP file's start: its RIFF header, then the first chunk's name and size.
fn webp(chunk: [u8; 4], rest: &[u8]) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
    bytes.extend_from_slice(&chunk);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(rest);
    bytes
}

#[test]
fn a_webp_is_measured_in_each_of_its_encodings() {
    // Extended: the canvas's width and height less one, 24 bits each, after 4 bytes of flags.
    let extended = webp(*b"VP8X", &[0, 0, 0, 0, 0x7f, 0x02, 0, 0xdf, 0x01, 0]);
    assert_eq!(size(&extended), Some(("webp", 640, 480)));
    // Lossless: a signature byte, then width and height less one in 14 bits each.
    let bits: u32 = 639 | (479 << 14);
    let mut lossless = vec![0x2f];
    lossless.extend_from_slice(&bits.to_le_bytes());
    assert_eq!(size(&webp(*b"VP8L", &lossless)), Some(("webp", 640, 480)));
    // Lossy: a frame tag, the start code, then 14 bits of each size under 2 of scale.
    let lossy = [0, 0, 0, 0x9d, 0x01, 0x2a, 0x80, 0x42, 0xe0, 0x01];
    assert_eq!(size(&webp(*b"VP8 ", &lossy)), Some(("webp", 640, 480)));
    // A lossless one whose signature is wrong, and a chunk no encoding names.
    assert_eq!(size(&webp(*b"VP8L", &[0, 0, 0, 0, 0])), None);
    assert_eq!(size(&webp(*b"ALPH", &[0; 10])), None);
}

#[test]
fn a_jpeg_s_fill_bytes_and_markers_without_a_length_are_stepped_over() {
    // SOI, fill bytes before a restart marker (no length), then SOF2 with height 7, width 9.
    let jpeg = [
        0xff, 0xd8, 0xff, 0xff, 0xd0, 0xff, 0xc2, 0, 11, 8, 0, 7, 0, 9, 3, 0, 0, 0,
    ];
    assert_eq!(size(&jpeg), Some(("jpeg", 9, 7)));
    // A segment whose length runs past the end.
    assert_eq!(size(&[0xff, 0xd8, 0xff, 0xe0, 0, 40, 0, 0]), None);
}
