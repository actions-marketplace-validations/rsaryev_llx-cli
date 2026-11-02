use super::*;

#[test]
fn the_first_who_asks_is_who_stopped_it() {
    let stop = Stop::default();
    assert!(!stop.asked());
    stop.ask(By::Person);
    stop.ask(By::Run);
    assert_eq!(stop.by(), Some(By::Person));
    let stop = Stop::default();
    stop.ask(By::Run);
    stop.ask(By::Person);
    assert_eq!(stop.by(), Some(By::Run));
}
