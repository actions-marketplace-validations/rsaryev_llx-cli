use super::*;

#[test]
fn a_place_is_where_the_process_is_with_its_environment_and_the_given_command_line() {
    let place = here(Path::new("/work/project"), vec!["-y".into()]);
    assert_eq!(place.cwd, "/work/project");
    assert_eq!(place.args, ["-y"]);
    let dir = std::env::current_dir().unwrap();
    assert_eq!(place.dir, dir.display().to_string());
    let path = std::env::var("PATH").unwrap();
    assert!(place.env.contains(&("PATH".to_string(), path)));
}
