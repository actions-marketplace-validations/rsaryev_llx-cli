use super::*;

#[test]
fn a_kernel_without_truncate_control_cannot_confine_a_command() {
    for abi in [-1, 0, 1, 2] {
        let why = supported(abi).unwrap_err();
        assert_eq!(why.kind(), io::ErrorKind::Unsupported);
        assert!(why.to_string().contains("ABI 3"), "{why}");
    }
    for abi in [3, 4, 6] {
        assert!(supported(abi).is_ok());
    }
}
