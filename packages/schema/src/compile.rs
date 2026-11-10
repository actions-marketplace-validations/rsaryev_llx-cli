//! A schema compiled once into nodes, so a check reads no keyword: each schema object a node of
//! what it asks, read by the draft the document names (`draft.rs`), each `$ref` the node it lands
//! on (`index.rs`), compiled once however often it is named; one that points at itself is a cycle
//! the check counts. What this check cannot hold data to is noted, never dropped unsaid.

mod containers;
mod evaluated;
mod keywords;
mod kinds;
pub(crate) mod tags;

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::draft::Draft;
use crate::format;
use crate::hash::Table;
use crate::index::Index;
use crate::node::{HAS_KIND, Id, Node, Rules, kind_of};
use crate::uri::{self, BASE};

use keywords::{count, limits, types};

/// A schema compiled: its nodes (the root's the first), its patterns' sources, and what it
/// does not check.
pub(crate) struct Compiled {
    pub(crate) nodes: Vec<Node>,
    pub(crate) patterns: Vec<Box<str>>,
    pub(crate) notes: Vec<&'static str>,
    pub(crate) draft: Draft,
}

/// `root` compiled, its references landing in it or in `documents`.
pub(crate) fn compile(root: &Value, documents: &[(String, Value)]) -> Compiled {
    let draft = Draft::of(root);
    let index = Index::build(root, documents, draft);
    let mut compiler = Compiler {
        draft,
        validation: draft < Draft::Nineteen || index.validates(root),
        index,
        nodes: vec![],
        refs: HashMap::new(),
        patterns: vec![],
        notes: vec![],
    };
    compiler.node(root, BASE);
    tags::mark(&mut compiler.nodes);
    kinds::fold(&mut compiler.nodes);
    evaluated::settle(&mut compiler.nodes);
    Compiled {
        nodes: compiler.nodes,
        patterns: compiler.patterns,
        notes: compiler.notes,
        draft,
    }
}

/// One pass over the built nodes (`tags::mark`, `kinds::fold`, `evaluated::settle`): what
/// `find` finds in each node's rules, all found before `put` gives any of it, so the pass reads
/// the nodes as they stood. Whether it found any.
fn each<T>(
    nodes: &mut [Node],
    find: impl Fn(&[Node], &Rules) -> Option<T>,
    put: impl Fn(&mut Rules, T),
) -> bool {
    let found: Vec<(usize, T)> = (nodes.iter().enumerate())
        .filter_map(|(at, node)| match node {
            Node::Rules(rules) => find(nodes, rules).map(|it| (at, it)),
            Node::Any | Node::Never => None,
        })
        .collect();
    let any = !found.is_empty();
    for (at, it) in found {
        if let Some(Node::Rules(rules)) = nodes.get_mut(at) {
            put(rules, it);
        }
    }
    any
}

struct Compiler<'j> {
    draft: Draft,
    /// Whether the validation vocabulary is in force (the metaschema's `$vocabulary`).
    validation: bool,
    index: Index<'j>,
    nodes: Vec<Node>,
    /// Each schema a `$ref` landed on, by its address: landed on again, it is the same node.
    refs: HashMap<usize, Id>,
    patterns: Vec<Box<str>>,
    notes: Vec<&'static str>,
}

impl<'j> Compiler<'j> {
    /// `schema`'s node, read with `base`; its place taken before what it holds is compiled, so a
    /// `$ref` inside it to itself finds it.
    fn node(&mut self, schema: &'j Value, base: &str) -> Id {
        let id = self.nodes.len();
        self.nodes.push(Node::Any);
        let node = match schema {
            Value::Bool(false) => Node::Never,
            Value::Object(rules) => Node::Rules(Box::new(self.rules(rules, base))),
            Value::Bool(true) | Value::Null | Value::Number(_) | Value::String(_) => Node::Any,
            Value::Array(_) => Node::Any,
        };
        if let Some(slot) = self.nodes.get_mut(id) {
            *slot = node;
        }
        id
    }

    fn note(&mut self, what: &'static str) {
        if !self.notes.contains(&what) {
            self.notes.push(what);
        }
    }

    /// The node a reference lands on, compiled once; none (noted) where it lands outside the
    /// document.
    fn reference(&mut self, text: &str, base: &str) -> Option<Id> {
        let Some((target, landed)) = self.index.find(&uri::resolve(base, text)) else {
            self.note("$ref outside the schema");
            return None;
        };
        let address = std::ptr::from_ref(target) as usize;
        if let Some(id) = self.refs.get(&address) {
            return Some(*id);
        }
        self.refs.insert(address, self.nodes.len());
        Some(self.node(target, &landed))
    }

    fn many(&mut self, schemas: Option<&'j Value>, base: &str) -> Box<[Id]> {
        let schemas = schemas
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        schemas
            .iter()
            .map(|schema| self.node(schema, base))
            .collect()
    }

    fn pattern(&mut self, source: &str) -> usize {
        match self.patterns.iter().position(|known| **known == *source) {
            Some(place) => place,
            None => {
                self.patterns.push(source.into());
                self.patterns.len() - 1
            }
        }
    }

    fn rules(&mut self, rules: &'j Map<String, Value>, base: &str) -> Rules {
        let draft = self.draft;
        let get = |key: &str| rules.get(key).filter(|_| draft.knows(key));
        let referred = get("$ref").and_then(Value::as_str);
        // Up to draft 7, a `$ref` is the whole schema: what is beside it, its id too, is not read.
        if let (Some(text), true) = (referred, draft.ref_alone()) {
            return Rules {
                reference: self.reference(text, base),
                ..Rules::default()
            };
        }
        let id = get(draft.id_keyword()).and_then(Value::as_str);
        let resolved = id.map(|id| uri::split(&uri::resolve(base, id)).0.to_string());
        let base = resolved
            .as_deref()
            .filter(|base| !base.is_empty())
            .unwrap_or(base);
        // `$dynamicRef` is read even beside a `$ref`, so what it cannot hold to is noted.
        let dynamic = self.dynamic(rules);
        let reference = referred
            .or(dynamic)
            .and_then(|text| self.reference(text, base));
        let (types, type_names) = types(get("type"));
        let condition = get("if").map(|when| {
            let when = self.node(when, base);
            let then = get("then").map(|schema| self.node(schema, base));
            (
                when,
                then,
                get("else").map(|schema| self.node(schema, base)),
            )
        });
        let options = get("enum").filter(|options| options.is_array()).cloned();
        let listed = options.as_ref().and_then(Value::as_array);
        let table = listed.and_then(|o| Table::of(o));
        let option_kinds = (listed.into_iter().flatten()).fold(0, |all, o| all | kind_of(o, false));
        let format = get("format").and_then(Value::as_str);
        if format.is_some_and(format::unchecked) {
            self.note("format");
        }
        let pattern = get("pattern")
            .and_then(Value::as_str)
            .map(|p| self.pattern(p));
        let mut built = Rules {
            has: 0,
            reference,
            types,
            type_names,
            table,
            option_kinds,
            options,
            constant: get("const").cloned(),
            all_of: self.many(get("allOf"), base),
            any_of: self.many(get("anyOf"), base),
            one_of: self.many(get("oneOf"), base),
            not: get("not").map(|schema| self.node(schema, base)),
            condition,
            limits: limits(rules, draft),
            multiple: match get("multipleOf") {
                Some(Value::Number(m)) => Some(m.clone()),
                Some(_) | None => None,
            },
            min_length: count(rules, "minLength"),
            max_length: count(rules, "maxLength"),
            pattern,
            format: format.and_then(|name| {
                let hostnames = match draft {
                    Draft::Four | Draft::Six => format::Hostnames::Rfc1123,
                    Draft::Seven | Draft::Nineteen | Draft::Twenty => format::Hostnames::Idna,
                };
                Some((name.into(), format::resolve(name, hostnames)?))
            }),
            array: self.arrays(rules, base),
            object: self.objects(rules, base),
            unevaluated_fields: get("unevaluatedProperties").map(|s| self.node(s, base)),
            unevaluated_items: get("unevaluatedItems").map(|s| self.node(s, base)),
            // The rest is set once every node is built: `tags::mark`, `kinds::fold`,
            // `evaluated::settle`.
            ..Rules::default()
        };
        built.has = built.groups();
        if !self.validation {
            built.without_validation();
        }
        // A schema that is only a `$ref` (a description beside it asks nothing) is its target:
        // no group bit, so no `enum` or `const` either (`HAS_KIND`).
        built.alias = (built.has == 0).then_some(built.reference).flatten();
        built.only_types = (built.has == HAS_KIND
            && built.reference.is_none()
            && built.options.is_none()
            && built.constant.is_none())
        .then_some(built.types)
        .flatten();
        built
    }

    /// `$dynamicRef` (2020-12) or `$recursiveRef` (2019-09), read as the plain reference it is
    /// while one resource gives its anchor; past that, noted as not checked here.
    fn dynamic(&mut self, rules: &'j Map<String, Value>) -> Option<&'j str> {
        let (text, many) = match self.draft {
            Draft::Twenty => {
                let text = rules.get("$dynamicRef").and_then(Value::as_str)?;
                let name = uri::split(text).1;
                (text, self.index.dynamic.get(name).copied().unwrap_or(0) > 1)
            }
            Draft::Nineteen => {
                let text = rules.get("$recursiveRef").and_then(Value::as_str)?;
                (text, self.index.recursive > 1)
            }
            Draft::Four | Draft::Six | Draft::Seven => return None,
        };
        if many {
            self.note("$dynamicRef across resources");
        }
        Some(text)
    }
}
