//! The position automaton's construction: each class occurrence a position, numbered in the
//! order the text meets them, so most followers are the next position (`p` to `p + 1`) or the
//! position itself (`x+`), which the simulation takes by a shift and a mask; each `^`, `$`,
//! `\b` and `\B` a marker, resolved by `anchors.rs`.

use super::super::bits::Bits;
use super::super::class::Class;
use super::super::parse::{Assert, Node};
use super::anchors::{self, Ends, Markers};

/// The automaton as built, before the simulation's tables.
pub(super) struct Built<B> {
    /// Each position's followers, between characters.
    pub(super) follow: Vec<B>,
    /// Each position's class (none for an anchor's marker).
    pub(super) classes: Vec<Class>,
    /// Where matches start and end.
    pub(super) ends: Ends<B>,
}

/// The automaton of `node`, whose positions (its assertions' markers among them) fit in `B`.
pub(super) fn build<B: Bits>(node: &Node) -> Built<B> {
    let mut builder = Builder {
        follow: vec![],
        classes: vec![],
        markers: Markers {
            starts: B::none(),
            ends: B::none(),
            bounds: B::none(),
            insides: B::none(),
        },
    };
    let frag = builder.node(node);
    let ends = anchors::resolve(
        &mut builder.follow,
        &builder.markers,
        (frag.first, frag.last, frag.nullable),
    );
    Built {
        follow: builder.follow,
        classes: builder.classes,
        ends,
    }
}

/// What a piece of the pattern contributes: whether it matches the empty text, the positions it
/// may start and end at.
#[derive(Clone, Copy)]
struct Frag<B> {
    nullable: bool,
    first: B,
    last: B,
}

impl<B: Bits> Frag<B> {
    fn empty() -> Self {
        Self {
            nullable: true,
            first: B::none(),
            last: B::none(),
        }
    }
}

struct Builder<B> {
    follow: Vec<B>,
    classes: Vec<Class>,
    markers: Markers<B>,
}

impl<B: Bits> Builder<B> {
    fn node(&mut self, node: &Node) -> Frag<B> {
        match node {
            Node::Class(class) => self.position(class.clone()),
            Node::Assert(assert) => {
                // A marker: a position no character enters, resolved by `anchors.rs`.
                let frag = self.position(Class::empty());
                match assert {
                    Assert::Start => self.markers.starts = self.markers.starts.or(frag.first),
                    Assert::End => self.markers.ends = self.markers.ends.or(frag.first),
                    Assert::Word => self.markers.bounds = self.markers.bounds.or(frag.first),
                    Assert::NotWord => self.markers.insides = self.markers.insides.or(frag.first),
                }
                frag
            }
            Node::Concat(items) => self.sequence(items),
            Node::Alt(items) => {
                let mut out = Frag {
                    nullable: false,
                    first: B::none(),
                    last: B::none(),
                };
                for item in items {
                    let frag = self.node(item);
                    out.nullable |= frag.nullable;
                    out.first = out.first.or(frag.first);
                    out.last = out.last.or(frag.last);
                }
                out
            }
            Node::Repeat(inner, min, max) => self.repeat(inner, *min, *max),
        }
    }

    fn position(&mut self, class: Class) -> Frag<B> {
        let position = B::none().with(self.classes.len());
        self.classes.push(class);
        self.follow.push(B::none());
        Frag {
            nullable: false,
            first: position,
            last: position,
        }
    }

    fn sequence(&mut self, items: &[Node]) -> Frag<B> {
        let mut out = Frag::empty();
        for item in items {
            let next = self.node(item);
            out = self.concat(out, next);
        }
        out
    }

    /// `inner{min,max}`, written out: `x{2,4}` is `x x (x (x)?)?`, nested so that no two
    /// copies are active for one count; `x{2,}` is `x x+`.
    fn repeat(&mut self, inner: &Node, min: u32, max: Option<u32>) -> Frag<B> {
        let plain = if max.is_some() {
            min
        } else {
            min.saturating_sub(1)
        };
        let mut out = Frag::empty();
        for _ in 0..plain {
            let copy = self.node(inner);
            out = self.concat(out, copy);
        }
        let tail = match max {
            None => {
                let copy = self.node(inner);
                self.link(copy.last, copy.first);
                Frag {
                    nullable: copy.nullable || min == 0,
                    ..copy
                }
            }
            Some(max) => {
                // Built in the text's order, then nested from the right.
                let mut copies = vec![];
                for _ in min..max {
                    copies.push(self.node(inner));
                }
                let mut tail = Frag::empty();
                for copy in copies.into_iter().rev() {
                    let joined = self.concat(copy, tail);
                    tail = Frag {
                        nullable: true,
                        ..joined
                    };
                }
                tail
            }
        };
        self.concat(out, tail)
    }

    fn concat(&mut self, left: Frag<B>, right: Frag<B>) -> Frag<B> {
        self.link(left.last, right.first);
        Frag {
            nullable: left.nullable && right.nullable,
            first: if left.nullable {
                left.first.or(right.first)
            } else {
                left.first
            },
            last: if right.nullable {
                left.last.or(right.last)
            } else {
                right.last
            },
        }
    }

    /// Every position of `from` followed by every one of `to`.
    fn link(&mut self, from: B, to: B) {
        let follow = &mut self.follow;
        from.each(|position| {
            if let Some(set) = follow.get_mut(position) {
                *set = set.or(to);
            }
        });
    }
}
