//! Every integration test of the binary, in one test binary: one link, one
//! run, shared helpers in `common/`, the tests in `suite/` by what they cover.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::string_slice,
    reason = "test code: a panic fails the test"
)]

mod common;
mod suite;
