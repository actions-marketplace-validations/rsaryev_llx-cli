use super::*;

#[test]
fn the_page_carries_its_mark_and_escapes_details() {
    let ok = success("Test app", "Provider");
    assert!(ok.contains("<svg") && ok.contains("Authentication successful"));
    assert!(ok.contains("Test app is signing in to Provider"));
    let failed = failure("Sign-in failed.", Some("<script>x</script>"));
    assert!(failed.contains("&lt;script&gt;") && !failed.contains("<script>x"));
}
