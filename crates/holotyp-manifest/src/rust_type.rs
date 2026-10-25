//! Rust type names, as `std::any::type_name` prints them.
//!
//! Cast keys are strings, and the only thing that ever happens to a type name is that a
//! transparent wrapper is dropped. Doing that through a parser keeps the two shapes of
//! the operation honest instead of matching brackets by hand:
//!
//! - [`RustType::strip_leading`]: drop the wrapper while it is the *outermost* generic,
//!   used when looking up the shape of a type (`Option<Stroke>` borrows `Stroke`'s
//!   dictionary);
//! - [`RustType::strip_all`]: drop every occurrence, used when a wrapper is transparent
//!   everywhere (`Arc<Stroke>` is `Stroke`).
//!
//! A tree keeps the source span of every node, so a name that is not changed comes back
//! exactly as it went in. Nothing is ever re-formatted.

use std::fmt;
use std::ops::Range;

/// A parsed type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustType<'a> {
    text: &'a str,
    span: Range<usize>,
    kind: Kind<'a>,
}

/// What a type name is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind<'a> {
    /// `a::B`, `a::B<C, D>`.
    Path { path: &'a str, args: Vec<RustType<'a>> },
    /// `(A, B)`.
    Tuple(Vec<RustType<'a>>),
    /// `[A]`.
    Slice(Box<RustType<'a>>),
    /// `[A; N]`.
    Array(Box<RustType<'a>>),
    /// `&A`, `&mut A`.
    Reference(Box<RustType<'a>>),
    /// `*const A`, `*mut A`.
    Pointer(Box<RustType<'a>>),
    /// `fn(A, B) -> C`.
    Fn { args: Vec<RustType<'a>>, ret: Option<Box<RustType<'a>>> },
    /// Anything else (`4`, `false`, `!`), kept verbatim.
    Other,
}

impl<'a> RustType<'a> {
    /// Parse a type name.
    pub fn parse(src: &'a str) -> Self {
        Parser { src, pos: 0 }.ty()
    }

    /// The source text of this type.
    pub fn as_str(&self) -> &'a str {
        self.text
    }

    /// What the type is made of.
    pub fn kind(&self) -> &Kind<'a> {
        &self.kind
    }

    fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// This type with any of `paths` dropped while it is the outermost generic.
    ///
    /// Repeated: `Option<Option<A>>` becomes `A`, because each step is still the
    /// outermost wrapper.
    pub fn strip_leading<'b>(&'b self, paths: &[&str]) -> &'b RustType<'a> {
        let mut node = self;
        'outer: loop {
            for path in paths {
                if let Some(inner) = wrapped(node, path) {
                    node = inner;
                    continue 'outer;
                }
            }
            return node;
        }
    }

    /// This type with every `path<…>` occurrence dropped.
    ///
    /// Nesting is stitched back together: `Box<Arc<A>>` becomes `Box<A>`.
    pub fn strip_all(&self, path: &str) -> String {
        let mut cuts = Vec::new();
        collect_cuts(self, path, &mut cuts);
        if cuts.is_empty() {
            return self.text.to_string();
        }
        cuts.sort_unstable();

        // Spans are absolute within the parsed source; splice against this node's own
        // text so a subtree works as well as a whole tree.
        let base = self.span.start;
        let mut out = String::with_capacity(self.text.len());
        let mut pos = base;
        for (start, end) in cuts {
            if start < pos {
                // Already inside a dropped span.
                continue;
            }
            out.push_str(&self.text[pos - base..start - base]);
            pos = end;
        }
        out.push_str(&self.text[pos - base..]);
        out
    }

    /// Every type nested in this one.
    fn children(&self) -> Vec<&RustType<'a>> {
        match &self.kind {
            Kind::Path { args, .. } => args.iter().collect(),
            Kind::Tuple(items) => items.iter().collect(),
            Kind::Slice(inner)
            | Kind::Array(inner)
            | Kind::Reference(inner)
            | Kind::Pointer(inner) => vec![inner],
            Kind::Fn { args, ret } => {
                let mut all: Vec<&RustType<'a>> = args.iter().collect();
                if let Some(ret) = ret {
                    all.push(ret);
                }
                all
            }
            Kind::Other => Vec::new(),
        }
    }
}

impl fmt::Display for RustType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.text)
    }
}

/// The argument of a complete `path<…>`, if this node is exactly that.
fn wrapped<'a, 'b>(ty: &'b RustType<'a>, path: &str) -> Option<&'b RustType<'a>> {
    // A missing `>` must not count as a wrapper: parsing stops at the break, and dropping
    // half a generic would corrupt the name.
    if !ty.as_str().ends_with('>') {
        return None;
    }
    match ty.kind() {
        Kind::Path { path: name, args } if *name == path => match args.as_slice() {
            [inner] => Some(inner),
            _ => None,
        },
        _ => None,
    }
}

/// Record the spans to drop for every `path<…>` in the tree.
fn collect_cuts(ty: &RustType<'_>, path: &str, cuts: &mut Vec<(usize, usize)>) {
    if let Some(inner) = wrapped(ty, path) {
        let span = ty.span();
        let inner_span = inner.span();
        cuts.push((span.start, inner_span.start));
        cuts.push((inner_span.end, span.end));
        collect_cuts(inner, path, cuts);
        return;
    }
    for child in ty.children() {
        collect_cuts(child, path, cuts);
    }
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn ty(&mut self) -> RustType<'a> {
        let start = self.pos;
        let kind = if self.eat(b'&') {
            self.ws();
            if self.keyword("mut") {
                self.ws();
            }
            Kind::Reference(Box::new(self.ty()))
        } else if self.eat(b'*') {
            self.ws();
            let _ = self.keyword("const") || self.keyword("mut");
            self.ws();
            Kind::Pointer(Box::new(self.ty()))
        } else if self.eat(b'(') {
            self.tuple()
        } else if self.eat(b'[') {
            self.bracket()
        } else if self.src[self.pos..].starts_with("fn(") {
            self.pos += 3;
            self.function()
        } else if self.peek().is_some_and(is_path_start) {
            self.path()
        } else {
            self.other()
        };
        RustType {
            text: &self.src[start..self.pos],
            span: start..self.pos,
            kind,
        }
    }

    /// The rest of a tuple, after its opening paren.
    fn tuple(&mut self) -> Kind<'a> {
        let mut items = Vec::new();
        loop {
            self.ws();
            if self.eat(b')') {
                break;
            }
            items.push(self.ty());
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.eat(b')');
            break;
        }
        Kind::Tuple(items)
    }

    /// The rest of an array or slice, after its opening bracket.
    fn bracket(&mut self) -> Kind<'a> {
        self.ws();
        let elem = Box::new(self.ty());
        self.ws();
        if self.eat(b';') {
            // A const length: not a type, skip it.
            while self.peek().is_some_and(|b| b != b']') {
                self.pos += 1;
            }
            self.eat(b']');
            Kind::Array(elem)
        } else {
            self.eat(b']');
            Kind::Slice(elem)
        }
    }

    /// The rest of a function pointer, after `fn(`.
    fn function(&mut self) -> Kind<'a> {
        let mut args = Vec::new();
        loop {
            self.ws();
            if self.eat(b')') {
                break;
            }
            args.push(self.ty());
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.eat(b')');
            break;
        }
        let ret = if self.src[self.pos..].starts_with(" -> ") {
            self.pos += 4;
            Some(Box::new(self.ty()))
        } else {
            None
        };
        Kind::Fn { args, ret }
    }

    /// A path, with its generic arguments if it has any.
    fn path(&mut self) -> Kind<'a> {
        let start = self.pos;
        while self.peek().is_some_and(is_path_byte) {
            self.pos += 1;
        }
        let path = &self.src[start..self.pos];

        let mut args = Vec::new();
        if self.eat(b'<') {
            loop {
                self.ws();
                if self.eat(b'>') {
                    break;
                }
                args.push(self.ty());
                self.ws();
                if self.eat(b',') {
                    continue;
                }
                self.eat(b'>');
                break;
            }
        }
        Kind::Path { path, args }
    }

    /// Anything the grammar does not cover, up to the next delimiter.
    fn other(&mut self) -> Kind<'a> {
        while self
            .peek()
            .is_some_and(|b| !matches!(b, b',' | b'>' | b')' | b']' | b';'))
        {
            self.pos += 1;
        }
        Kind::Other
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn ws(&mut self) {
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
    }

    fn keyword(&mut self, word: &str) -> bool {
        if self.src[self.pos..].starts_with(word) {
            self.pos += word.len();
            true
        } else {
            false
        }
    }
}

/// The path prefix of a wrapper type, e.g. `alloc::sync::Arc` for `Arc<T>`.
pub fn wrapper_prefix<T: ?Sized>() -> &'static str {
    std::any::type_name::<T>().split('<').next().unwrap_or_default()
}

fn is_path_start(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b':'
}

fn is_path_byte(b: u8) -> bool {
    is_path_start(b)
}

#[cfg(test)]
mod tests {
    use super::{Kind, RustType};

    const ARC: &str = "alloc::sync::Arc";

    fn leading(ty: &str) -> String {
        RustType::parse(ty).strip_leading(&[ARC]).to_string()
    }

    fn all(ty: &str) -> String {
        RustType::parse(ty).strip_all(ARC)
    }

    #[test]
    fn parses_a_path_with_arguments() {
        let ty = RustType::parse("core::option::Option<typst_library::Stroke>");
        let Kind::Path { path, args } = ty.kind() else {
            panic!("expected a path, got {:?}", ty.kind());
        };
        assert_eq!(*path, "core::option::Option");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].as_str(), "typst_library::Stroke");
    }

    #[test]
    fn parses_other_shapes() {
        for text in [
            "u8",
            "&str",
            "&mut u8",
            "*const u8",
            "(u8, u16)",
            "[u8]",
            "[u8; 4]",
            "fn(u8) -> u8",
            "foo::Bar<u8, 4, false>",
        ] {
            // Round-trips: a parsed name renders as itself.
            assert_eq!(RustType::parse(text).as_str(), text);
        }
        assert!(matches!(RustType::parse("[u8; 4]").kind(), Kind::Array(_)));
        assert!(matches!(RustType::parse("[u8]").kind(), Kind::Slice(_)));
        assert!(matches!(RustType::parse("(u8, u16)").kind(), Kind::Tuple(_)));
        assert!(matches!(
            RustType::parse("fn(u8) -> u8").kind(),
            Kind::Fn { ret: Some(_), .. }
        ));
    }

    #[test]
    fn strip_leading_only_takes_the_outermost_chain() {
        assert_eq!(leading("alloc::sync::Arc<u8>"), "u8");
        assert_eq!(leading("alloc::sync::Arc<alloc::sync::Arc<u8>>"), "u8");
        // Not the outermost type: left alone.
        assert_eq!(
            leading("core::option::Option<alloc::sync::Arc<u8>>"),
            "core::option::Option<alloc::sync::Arc<u8>>"
        );
    }

    #[test]
    fn strip_all_takes_every_occurrence() {
        assert_eq!(
            all("core::option::Option<alloc::sync::Arc<u8>>"),
            "core::option::Option<u8>"
        );
        assert_eq!(
            all(
                "typst_library::layout::sides::Sides<core::option::Option<core::option::Option<alloc::sync::Arc<u8>>>>"
            ),
            "typst_library::layout::sides::Sides<core::option::Option<core::option::Option<u8>>>"
        );
        assert_eq!(all("(alloc::sync::Arc<u8>, alloc::sync::Arc<u16>)"), "(u8, u16)");
        assert_eq!(all("[alloc::sync::Arc<u8>; 4]"), "[u8; 4]");
        assert_eq!(
            all("fn(alloc::sync::Arc<u8>) -> alloc::sync::Arc<u16>"),
            "fn(u8) -> u16"
        );
    }

    #[test]
    fn strip_all_keeps_other_names_alone() {
        for text in [
            "u8",
            "foo::Bar<u8, 4, false>",
            "my_crate::Arc<u8>",
            "alloc::sync::ArcSet<u8>",
            "typst_syntax::span::Spanned<u8, false>",
        ] {
            assert_eq!(all(text), text);
        }
    }

    #[test]
    fn unmatched_generic_is_kept_verbatim() {
        assert_eq!(all("foo::Bar<u8"), "foo::Bar<u8");
        assert_eq!(all("alloc::sync::Arc<u8"), "alloc::sync::Arc<u8");
    }
}
