use super::*;

fn end(text: &str) -> Option<usize> {
    html(text.as_bytes(), 0)
}

#[test]
fn tags_are_read_by_their_shape() {
    assert_eq!(end("<a>x"), Some(3));
    assert_eq!(end("<br />"), Some(6));
    assert_eq!(end("</div >"), Some(7));
    assert_eq!(end("<a href=\"x>y\" b='c' d=e f>"), Some(26));
    assert_eq!(end("<a\n href='x'>"), Some(13));
    assert_eq!(end("<a href=>"), None);
    assert_eq!(end("<a h*ref>"), None);
    assert_eq!(end("</a href>"), None);
    assert_eq!(end("<33>"), None);
    assert_eq!(end("< a>"), None);
}

#[test]
fn the_other_forms_end_at_their_close() {
    assert_eq!(end("<!-- a -- b -->x"), Some(15));
    assert_eq!(end("<!-->x"), Some(5));
    assert_eq!(end("<!--->x"), Some(6));
    assert_eq!(end("<?php ?>"), Some(8));
    assert_eq!(end("<!DOCTYPE html>"), Some(15));
    assert_eq!(end("<![CDATA[a]]b]]>"), Some(16));
    assert_eq!(end("<!-- open"), None);
}
