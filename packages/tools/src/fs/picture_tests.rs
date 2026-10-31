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
