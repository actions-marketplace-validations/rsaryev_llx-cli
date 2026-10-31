use super::*;

#[test]
fn what_came_back_from_the_background_reads_as_where_it_came_from() {
    let goal = crate::arrivals::schedule("hello", "say hello").render();
    assert_eq!(shown(&goal), "◷ hello · say hello");
    assert_eq!(shown("fix the parser"), "fix the parser");
}

#[test]
fn init_reads_as_the_command_the_person_typed() {
    assert_eq!(shown(crate::commands::extras::INIT), "/init");
}
