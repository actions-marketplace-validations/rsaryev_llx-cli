use std::path::Path;
use std::time::Instant;

use protocol::Event;

use super::shown;
use crate::app::App;
use crate::mention::Files;
use crate::test_support::{call, done, history, label, ran, result};

#[test]
fn paths_in_the_project_read_from_it_and_its_cd_is_dropped() {
    let root = Some(Path::new("/work/stats"));
    let edited = label("Editing", "Edited", "/work/stats/src/stats.py");
    assert_eq!(shown(edited, root).subject, "src/stats.py");
    let command = ran("cd /work/stats && python3 -m unittest /work/stats/test_stats.py");
    assert_eq!(
        shown(command, root).subject,
        "python3 -m unittest test_stats.py"
    );
    // Another project's paths, and one that only starts like this one's, stay whole.
    let other = ran("cat /work/statsx/a /work/other/b");
    assert_eq!(shown(other.clone(), root).subject, other.subject);
    assert_eq!(shown(ran("ls /work/stats"), root).subject, "ls .");
    assert_eq!(
        shown(ran("ls /work/stats/x"), None).subject,
        "ls /work/stats/x"
    );
}

#[test]
fn the_transcript_shows_a_finished_command_by_its_paths_in_the_project() {
    let now = Instant::now();
    let mut app = App::new(60, 20, vec![]);
    app.files = Files::new("/work/stats".into(), Box::new(drop));
    app.event(Event::TaskStarted("fix it".into()), now);
    let command = ran("cd /work/stats && cat /work/stats/stats.py");
    app.event(call("1", "bash", &command), now);
    let report = done("Exit code: 0\nWall time: 0.0 seconds\nOutput:\nimport sys");
    app.event(result("1", "bash", &command, report), now);
    let shown = history(&mut app);
    assert!(shown.contains("cat stats.py"), "{shown}");
    assert!(!shown.contains("/work/stats"), "{shown}");
}
