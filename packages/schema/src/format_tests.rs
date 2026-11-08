use super::*;

#[track_caller]
fn valid(name: &str, texts: &[&str]) {
    for text in texts {
        assert_eq!(check(name, text), Some(true), "{name} should take {text:?}");
    }
}

#[track_caller]
fn invalid(name: &str, texts: &[&str]) {
    for text in texts {
        assert_eq!(
            check(name, text),
            Some(false),
            "{name} should refuse {text:?}"
        );
    }
}

#[test]
fn a_format_not_known_here_is_not_judged() {
    for name in ["idn-hostname", "idn-email", "unknown", "", "DATE"] {
        assert_eq!(check(name, "anything"), None, "{name}");
    }
}

#[test]
fn a_date_is_a_real_calendar_day() {
    valid(
        "date",
        &[
            "2026-11-07",
            "2020-02-29",
            "0400-02-29",
            "0001-01-01",
            "1999-12-31",
        ],
    );
    invalid(
        "date",
        &[
            "2021-02-29",
            "2100-02-29",
            "2020-04-31",
            "2020-01-32",
            "2020-13-01",
            "2020-00-10",
            "2020-01-00",
            "20-01-01",
            "2020-1-01",
            "2020-01-1",
            " 2020-01-01",
            "2020-01-01Z",
            "2020/01/01",
            "2020-01-0:",
            "\u{9e8}020-01-01",
            "",
            "+2020-01-01",
        ],
    );
}

#[test]
fn a_time_needs_its_offset_and_keeps_a_leap_second_to_the_last_utc_minute() {
    valid(
        "time",
        &[
            "08:30:06Z",
            "08:30:06z",
            "23:59:60Z",
            "23:59:60+00:00",
            "15:59:60-08:00",
            "01:29:60+01:30",
            "00:29:60-23:30",
            "23:20:50.52Z",
            "12:34:56-00:00",
            "00:59:59.999999999999999Z",
        ],
    );
    invalid(
        "time",
        &[
            "12:00:00",
            "12:00:00.52",
            "22:59:60Z",
            "23:58:60Z",
            "23:59:60+01:00",
            "24:00:00Z",
            "00:60:00Z",
            "00:00:61Z",
            "01:02:03+24:00",
            "01:02:03+00:60",
            "01:02:03Z+00:30",
            "08:30:06.Z",
            "08:30:06+0130",
            "08:30:06+01",
            "8:30:06Z",
            "08:30:06Z\n",
            "12:00Z",
            "08:30:06,5Z",
        ],
    );
}

#[test]
fn a_date_time_is_a_date_and_a_time_joined_by_t() {
    valid(
        "date-time",
        &[
            "2026-11-07T18:39:00Z",
            "1963-06-19t08:30:06.283185z",
            "1998-12-31T23:59:60Z",
            "1998-12-31T15:59:60.123-08:00",
            "1937-01-01T12:00:27.87+00:20",
        ],
    );
    invalid(
        "date-time",
        &[
            "1998-12-31T23:58:60Z",
            "1990-02-31T15:59:59Z",
            "1990-12-31T15:59:59-24:00",
            "2026-11-07 18:39:00Z",
            "2026-11-07T18:39:00",
            "2013-350T01:01:01",
            "1985-04-12T23:20:50+01",
            "2026-11-07",
            "2026-11-07T",
        ],
    );
}

#[test]
fn a_duration_follows_rfc_3339s_grammar() {
    valid(
        "duration",
        &[
            "P3DT4H",
            "P4Y",
            "PT0S",
            "P0D",
            "P1M",
            "PT1M",
            "PT36H",
            "P2W",
            "P1Y2M3DT4H5M6S",
            "P1Y2M",
            "PT1M2S",
            "P1M2D",
            "P01D",
        ],
    );
    invalid(
        "duration",
        &[
            "", "P", "PT", "P1YT", "PT1D", "P2D1Y", "P1D2H", "P2S", "P1Y2W", "P1", "PT0.5S",
            "P1Y2D", "PT1H2S", "-P1D", "P1D2T3H", "P1WT1H", "P0Y1W", "P1D ", "4D",
        ],
    );
}

#[test]
fn an_ipv4_is_a_dotted_quad_with_no_leading_zero() {
    valid(
        "ipv4",
        &["192.168.0.1", "0.0.0.0", "255.255.255.255", "10.20.30.40"],
    );
    invalid(
        "ipv4",
        &[
            "127.0.0.0.1",
            "256.1.1.1",
            "127.0",
            "087.10.0.1",
            "0x7f.0.0.1",
            "1.2.3.4 ",
            "192.168..1",
            "192.168.0.1.",
            "",
            "+1.2.3.4",
            "\u{ff11}.1.1.1",
            "1.2.3.4/24",
        ],
    );
}

#[test]
fn an_ipv6_has_eight_groups_or_a_double_colon() {
    valid(
        "ipv6",
        &[
            "::1",
            "::",
            "d6::",
            "1:d6::42",
            "1::d6:192.168.0.1",
            "::ffff:192.168.0.1",
            "1:2:3:4:5:6:7:8",
            "2001:DB8::1",
            "1000:1000:1000:1000:1000:1000:255.255.255.255",
            "::1:2:3:4:5:1.2.3.4",
            "1:2:3:4:5:6:7::",
        ],
    );
    invalid(
        "ipv6",
        &[
            "12345::",
            "::abcef",
            ":2:3:4:5:6:7:8",
            "1:2:3:4:5:6:7:",
            "1::d6::42",
            "1::2:192.168.256.1",
            "1:2:3:4:5:::8",
            "1:2:3:4:5:6:7",
            "1",
            "127.0.0.1",
            "1:2:3:4:1.2.3",
            "fe80::a%eth1",
            "[::1]",
            "::ffff:192.168.0.01",
            "1:2:3:4:5:6:7:8::",
            "100:100:100:100:100:100:100:255.255.255.255",
            "1.2.3.4::",
            "::1 ",
        ],
    );
}

#[test]
fn a_hostname_is_labels_of_letters_digits_and_inner_hyphens() {
    valid(
        "hostname",
        &[
            "www.example.com",
            "hostname",
            "1host",
            "a--b.com",
            "xn--9n2bp8q.xn--9t4b11yi5a",
            "xn--ll-0ea",
            "xn--wva3je",
            "xn--4dbc5h",
            "xn--k8j5u",
            "xn--ngba1o",
            "xn--11b2ezcw70k",
        ],
    );
    let long_label = format!("{}.com", "a".repeat(64));
    let long_name = format!("{}a", "abcdefghi.".repeat(26));
    invalid(
        "hostname",
        &[
            "",
            ".",
            ".example",
            "example.",
            "-host",
            "host-",
            "host_name",
            "ex ample.com",
            "\u{212a}elvin.example.com",
            "example\u{ff0e}com",
            "xn--X",
            "xn--hello-txk",
            "xn--hello-zed",
            "xn--hello-6bf",
            "xn--07jt112bpxg.xn--9t4b11yi5a",
            "xn--al-0ea",
            "xn--S-jib3p",
            "xn--A-2hc5h",
            "xn--vek",
            "xn--ngb6iyr",
            "xn--11b2er09f",
            "XN--aa---o47jg78q",
            "xn--a",
            "xn--ls8h",
            "xn--9n2p8q",
            "xn--abc-",
            "ab--cd",
            "xn--chb89f",
            &long_label,
            &long_name,
        ],
    );
}

#[test]
fn an_email_is_an_rfc_5321_mailbox() {
    valid(
        "email",
        &[
            "a.b@example.com",
            "te~st@example.com",
            "\"joe bloggs\"@example.com",
            "\"joe@bloggs\"@example.com",
            "joe@[127.0.0.1]",
            "joe@[IPv6:::1]",
            "a@[ipv6:::1]",
            "\"\"@iana.org",
            "\"\\\"\"@iana.org",
            "test@io",
            "test@255.255.255.255",
        ],
    );
    invalid(
        "email",
        &[
            "2962",
            ".test@example.com",
            "test.@example.com",
            "te..st@example.com",
            "joe@invalid=domain.com",
            "joe@[127.0.0.300]",
            "@example.com",
            "joe@",
            "joe bloggs@example.com",
            "a\u{e9}@iana.org",
            "a@\u{e9}.org",
            "\"test\\\u{a9}\"@iana.org",
            "test@iana.org\n",
            "test\"@iana.org",
            "\"test\"test@iana.org",
            "\"test@iana.org",
            "a@b@c.org",
            "[1.2.3.4]@iana.org",
            "test@iana.org.",
            "test@[1.2.3.4",
            "a@[]",
            "test@[RFC-5322-domain-literal]",
            "a,b@iana.org",
            "a@b(c).org",
        ],
    );
}

#[test]
fn a_uri_is_absolute_and_ascii() {
    valid(
        "uri",
        &[
            "https://example.com/a?b=c#d",
            "http://foo.com/blah_(wikipedia)_blah#cite-1",
            "http://-.~_!$&'()*+,;=:%40:80%2f::::::@example.com",
            "ldap://[2001:db8::7]/c=GB",
            "mailto:John.Doe@example.com",
            "urn:oasis:names:docbook:4.1.2",
            "http://999.999.9.9/",
            "http://[v1.fe:a]/",
            "file:/etc/hosts",
        ],
    );
    invalid(
        "uri",
        &[
            "//foo.bar/",
            "/abc",
            "abc",
            "http:// x.com",
            "bar,baz:foo",
            "https://[@example.org/",
            "https://example.org/foobar\u{ae}.txt",
            "https://e.org/a\\b",
            "https://e.org/a{}",
            "http://e.com/%6G",
            "http://e.com/%",
            "1http://e.com",
            "http://e.com:abc/",
            "http://[::ffff:01.2.3.4]",
            "http:/[::1]",
            "http://foo.bar/\n",
            "http://2001:0db8:85a3::7334",
            "http://e.com/#a#b",
            "bar%baz:foo",
            "http://a@b@c.org/",
            "http://[v1.f%41]/",
        ],
    );
}

#[test]
fn a_uri_reference_may_be_relative() {
    valid(
        "uri-reference",
        &[
            "",
            "//",
            "/abc",
            "abc",
            "#fragment",
            "?query=1",
            "./this:that",
            "//foo.bar/?b=q#q",
        ],
    );
    invalid(
        "uri-reference",
        &[
            "\\\\WINDOWS\\fileshare",
            "#frag\\ment",
            "/%A",
            "/a\"b",
            "/[::1]",
            "1:b",
            "/p\n",
        ],
    );
}

#[test]
fn an_iri_takes_unicode_where_iris_allow_it() {
    valid(
        "iri",
        &[
            "http://\u{192}\u{f8}\u{f8}.\u{df}\u{e5}r/?\u{2202}\u{e9}\u{153}=\u{3c0}#\u{3c0}",
            "http://\u{192}\u{f8}\u{f8}.\u{df}\u{e5}r/\u{10300}",
            "http://x.org/?q=\u{f0000}",
            "urn:example:resource",
        ],
    );
    invalid(
        "iri",
        &[
            "\u{e2}\u{3c0}\u{3c0}",
            "ht\u{3c4}p://example.com",
            "http://x.org/\u{f0000}",
            "http://x.org/\u{fffe}",
            "http://x.org/%6G",
            "http://\u{e2}@b@x.org/",
        ],
    );
    valid(
        "iri-reference",
        &[
            "\u{e2}\u{3c0}\u{3c0}",
            "#\u{192}r\u{e4}gm\u{ea}nt",
            "./\u{e2}:b",
        ],
    );
    invalid(
        "iri-reference",
        &["#\u{192}r\u{e4}g\\m\u{ea}nt", "\u{e2}:b", "/%"],
    );
}

#[test]
fn a_uuid_is_8_4_4_4_12_hex_digits() {
    valid(
        "uuid",
        &[
            "550e8400-e29b-41d4-a716-446655440000",
            "2EB8AA08-AA98-11EA-B4AA-73B441D16380",
        ],
    );
    invalid(
        "uuid",
        &[
            "550e8400-e29b-41d4-a716-44665544000",
            "550e8400e29b41d4a716446655440000",
            "550e8400-e29b-41d4-a716-44665544000g",
            "550e840-0e29b-41d4-a716-446655440000",
            "{550e8400-e29b-41d4-a716-446655440000}",
            "550e8400-e29b-41d4-a716-446655440000\n",
        ],
    );
}

#[test]
fn a_json_pointer_escapes_every_tilde() {
    valid(
        "json-pointer",
        &[
            "",
            "/",
            "/foo/bar~0/baz~1/%a",
            "/foo//bar",
            "/\u{1f60e}",
            "/a\0b",
        ],
    );
    invalid(
        "json-pointer",
        &["#", "#/", "a", "/foo/bar~", "/~2", "/~0~", "/~~"],
    );
    valid(
        "relative-json-pointer",
        &["1", "0/foo/bar", "0#", "120/foo", "100", "0//"],
    );
    invalid(
        "relative-json-pointer",
        &[
            "", "/foo", "1foo", "-1/foo", "+1/foo", "0##", "01/a", "01#", "0/~2", "1#/a", "1\n",
        ],
    );
}

#[test]
fn a_uri_template_follows_rfc_6570() {
    valid(
        "uri-template",
        &[
            "",
            "http://example.com/dictionary/{term:1}/{term}",
            "a%41b",
            "a\u{1f600}b",
            "a\u{e000}b",
            "{+var}",
            "{#var}",
            "{var*}",
            "{x,y}",
            "{a.b}",
            "{%41}",
            "{v:1000}",
            "a'b",
        ],
    );
    invalid(
        "uri-template",
        &[
            "{term",
            "{}",
            "{a,,b}",
            "{v:0}",
            "{v:10000}",
            "foo}bar",
            "a%",
            "a%4",
            "a%GG",
            "{var:1*}",
            "{a.}",
            "{a..b}",
            "{%}",
            "{v:01}",
            "a b",
            "a\u{7f}b",
            "{a,}",
            "{var=def}",
            "{,+var}",
            "a^b",
        ],
    );
}

#[test]
fn drafts_4_and_6_read_a_hostname_as_rfc_1123_alone() {
    let old = resolve("hostname", Hostnames::Rfc1123).unwrap();
    assert!(old(b"ab--cd.example") && old(b"xn--4gbwdl.xn--wgbh1c"));
    assert!(!old(b"-hostname") && !old(b"hostname-") && !old(b""));
    let new = resolve("hostname", Hostnames::Idna).unwrap();
    assert!(!new(b"XN--aa---o47jg78q") && new(b"a--b.com"));
}

#[test]
fn a_regex_is_ecma_262_whether_or_not_it_is_run_here() {
    valid("regex", &["^[a-z]+$", "(?=a)b", "\\d{2,3}", "(a)\\1"]);
    invalid("regex", &["^(abc]", "a**", "\\a", "[z-a]"]);
}
