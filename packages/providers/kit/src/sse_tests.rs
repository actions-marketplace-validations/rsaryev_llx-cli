use serde_json::json;

use super::*;

#[test]
fn each_events_data_is_read_as_json() {
    let body = ": keep-alive\n\nevent: a\ndata: {\"n\":1}\n\n\
                event: b\r\ndata: {\"n\":\r\ndata: 2}\r\n\r\ndata: not json\n\ndata: [DONE]\n\n\
                data: {\"n\":3}";
    assert_eq!(
        events(body).collect::<Vec<_>>(),
        [json!({"n": 1}), json!({"n": 2}), json!({"n": 3})]
    );
}
