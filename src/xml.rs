//! A small namespace-aware XML tree, read with `quick-xml` (#325).
//!
//! An XML element name only means something together with the namespace URI
//! its prefix is bound to: `<D:href>`, `<d:href>`, `<a:href>` and
//! `<href xmlns="DAV:">` are one and the same WebDAV element, while
//! `<href xmlns="urn:example"/>` is some other vocabulary's `href`.
//! [`parse`] turns a document into a tree whose elements carry their
//! namespace URI and local name, so callers match on what an element is
//! rather than on how a server happened to spell it.
//!
//! The input comes from remote servers, so the reader is bounded: the input
//! size, the nesting depth, the number of elements and the namespace
//! declarations in scope are capped, and going over a cap is an error, as is
//! malformed markup (an unclosed or mismatched tag, text before the root
//! element). Reading stops once the root element closes, so whatever a
//! server appends after it (PHP warnings on a sabre/dav install with
//! `display_errors` on) is never looked at. Nothing is ever expanded beyond the five
//! predefined entities and character references: a `<!DOCTYPE>` is skipped
//! unread, so neither external entities nor nested entity definitions
//! ("billion laughs") can do anything, and an unknown `&name;` stays as it
//! was written. A stray `&` that starts no reference is kept as text, as the
//! hand-written extractor this replaces did.

use std::borrow::Cow;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::NsReader;

/// Largest document [`parse`] accepts, and the most the CalDAV client reads
/// of a response body. A REPORT of a calendar with tens of thousands of
/// events fits; the tree of a document this size already costs several
/// times the input in memory, which matters on a small VPS.
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// Deepest element nesting [`parse`] accepts. A multistatus is about eight
/// levels deep; the cap also keeps dropping the tree off a deep stack.
pub const MAX_DEPTH: usize = 128;

/// Most elements [`parse`] accepts in one document. A REPORT takes about
/// seven per event (a 5000-event calendar about 35 000), so this allows
/// some 140 000 events. Names and namespaces are shared between elements,
/// so an element costs about 120 bytes of tree: a hostile document at the
/// cap costs about 120 MB on top of its body (64 MB at most), text included.
pub const MAX_ELEMENTS: usize = 1_000_000;

/// Most namespace declarations (`xmlns`, `xmlns:p`) in scope at once.
/// quick-xml resolves each name by scanning the bindings in scope, so
/// without a cap a root declaring a million prefixes over a million
/// elements is quadratic. A real multistatus declares fewer than ten.
pub const MAX_NAMESPACE_BINDINGS: usize = 256;

/// Most attributes [`parse`] keeps in one document. A FindItem answer has
/// about three per item; the cap bounds the memory a document of nothing
/// but attributes can cost, as [`MAX_ELEMENTS`] does for elements.
pub const MAX_ATTRIBUTES: usize = 1_000_000;

/// Distinct names and namespace URIs shared between elements; past this
/// many, each further element gets its own copy.
const MAX_INTERNED: usize = 1024;

/// Longest server-controlled name or message quoted in an error.
const MAX_QUOTED: usize = 64;

/// An element: its namespace, its local name, its unqualified attributes and
/// its children in document order.
#[derive(Debug)]
pub struct Element {
    ns: Option<Arc<str>>,
    name: Arc<str>,
    /// Attributes written without a prefix (`Id="..."`), by name, values
    /// decoded like text. Namespace declarations and prefixed attributes
    /// (`xml:lang`) are not kept: nothing calrs reads needs them (#369).
    attrs: Box<[(Arc<str>, String)]>,
    children: Vec<Node>,
}

/// A child of an [`Element`]. Adjacent text, CDATA sections and references
/// are merged into one `Text`, and text that is only whitespace is dropped.
#[derive(Debug)]
pub enum Node {
    Element(Element),
    Text(String),
}

impl Element {
    /// The element's local name, without any prefix.
    pub fn local_name(&self) -> &str {
        &self.name
    }

    /// The namespace URI the element's name is bound to, or `None` when it
    /// is in no namespace: unprefixed with no default namespace in scope, or
    /// under a prefix the document never declared.
    pub fn namespace(&self) -> Option<&str> {
        self.ns.as_deref()
    }

    /// Whether this is the element `name` of namespace `ns`. An element in
    /// no namespace matches on its local name alone, the tolerance the old
    /// prefix-based extractor had for servers (and test fixtures) that leave
    /// a prefix undeclared. An element bound to another namespace never
    /// matches.
    pub fn is(&self, ns: &str, name: &str) -> bool {
        &*self.name == name && self.ns.as_deref().is_none_or(|own| own == ns)
    }

    /// The child elements, in document order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(el) => Some(el),
            Node::Text(_) => None,
        })
    }

    /// The child elements that are `name` of namespace `ns`.
    pub fn children<'a>(&'a self, ns: &'a str, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.elements().filter(move |el| el.is(ns, name))
    }

    /// The first child element that is `name` of namespace `ns`.
    pub fn child(&self, ns: &str, name: &str) -> Option<&Element> {
        self.elements().find(|el| el.is(ns, name))
    }

    /// Every element below this one, depth first in document order, without
    /// recursion.
    pub fn descendants(&self) -> Descendants<'_> {
        Descendants {
            stack: vec![self.children.iter()],
        }
    }

    /// The first element below this one that is `name` of namespace `ns`.
    pub fn find(&self, ns: &str, name: &str) -> Option<&Element> {
        self.descendants().find(|el| el.is(ns, name))
    }

    /// The value of the unqualified attribute `name`, decoded.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| &**key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Whether the element has any child, element or text.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// The element's own text: its text children joined, CDATA unwrapped
    /// and references decoded. Text inside child elements is not included.
    pub fn text(&self) -> String {
        self.text_ref().into_owned()
    }

    /// [`Element::text`], borrowed when the element holds a single text
    /// node (always, unless child elements split it), so reading a value
    /// need not copy it.
    pub fn text_ref(&self) -> Cow<'_, str> {
        let mut texts = self.children.iter().filter_map(|node| match node {
            Node::Text(text) => Some(text.as_str()),
            Node::Element(_) => None,
        });
        let Some(first) = texts.next() else {
            return Cow::Borrowed("");
        };
        match texts.next() {
            None => Cow::Borrowed(first),
            Some(second) => {
                let mut out = format!("{first}{second}");
                out.extend(texts);
                Cow::Owned(out)
            }
        }
    }

    /// [`Element::text`] with surrounding whitespace trimmed, or `None` when
    /// nothing is left.
    pub fn trimmed_text(&self) -> Option<String> {
        let text = self.text_ref();
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}

/// Iterator returned by [`Element::descendants`].
pub struct Descendants<'a> {
    stack: Vec<std::slice::Iter<'a, Node>>,
}

impl<'a> Iterator for Descendants<'a> {
    type Item = &'a Element;

    fn next(&mut self) -> Option<&'a Element> {
        while let Some(top) = self.stack.last_mut() {
            match top.next() {
                Some(Node::Element(el)) => {
                    self.stack.push(el.children.iter());
                    return Some(el);
                }
                Some(Node::Text(_)) => {}
                None => {
                    self.stack.pop();
                }
            }
        }
        None
    }
}

/// Parse a document into its root element. `Ok(None)` means there is no
/// element at all (an empty body, or only whitespace, comments and an XML
/// declaration). Text outside the root element is ignored. A UTF-8 byte
/// order mark is skipped.
pub fn parse(xml: &str) -> Result<Option<Element>> {
    parse_with(xml, &LIMITS)
}

/// The caps [`parse`] enforces, separate so tests can lower them.
struct Limits {
    input_bytes: usize,
    depth: usize,
    elements: usize,
    bindings: usize,
    attributes: usize,
}

const LIMITS: Limits = Limits {
    input_bytes: MAX_INPUT_BYTES,
    depth: MAX_DEPTH,
    elements: MAX_ELEMENTS,
    bindings: MAX_NAMESPACE_BINDINGS,
    attributes: MAX_ATTRIBUTES,
};

/// `text` cut to [`MAX_QUOTED`] characters, for an error message: names and
/// parser messages can be as long as the server likes.
pub fn quoted(text: &str) -> String {
    match text.char_indices().nth(MAX_QUOTED) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text.to_string(),
    }
}

fn parse_with(xml: &str, limits: &Limits) -> Result<Option<Element>> {
    if xml.len() > limits.input_bytes {
        bail!(
            "XML document too large ({} bytes, the limit is {})",
            xml.len(),
            limits.input_bytes
        );
    }
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let mut reader = NsReader::from_str(xml);
    let config = reader.config_mut();
    config.allow_dangling_amp = true;
    config.check_end_names = true;
    config.allow_unmatched_ends = false;
    config.expand_empty_elements = false;

    let mut builder = Builder::new(limits);
    loop {
        let (resolved, event) = match reader.read_resolved_event() {
            Ok(read) => read,
            Err(e) => {
                let at = reader.error_position();
                bail!("malformed XML at byte {at}: {}", quoted(&e.to_string()));
            }
        };
        match event {
            Event::Start(start) => {
                let ns = builder.namespace(&resolved)?;
                builder.open(ns, &start)?;
            }
            Event::Empty(start) => {
                let ns = builder.namespace(&resolved)?;
                builder.open(ns, &start)?;
                builder.close()?;
            }
            Event::End(_) => builder.close()?,
            Event::Text(text) => {
                let text = text.decode().map_err(|e| anyhow!("XML text: {e}"))?;
                builder.push_text(&text)?;
            }
            Event::CData(cdata) => {
                let text = cdata.decode().map_err(|e| anyhow!("XML CDATA: {e}"))?;
                builder.push_text(&text)?;
            }
            Event::GeneralRef(reference) => {
                let name = reference
                    .decode()
                    .map_err(|e| anyhow!("XML reference: {e}"))?;
                match decode_reference(&name) {
                    Some(c) => builder.push_text(c.encode_utf8(&mut [0; 4]))?,
                    // Unknown or invalid: keep it as written, never expand it.
                    None => builder.push_text(&format!("&{name};"))?,
                }
            }
            Event::Eof => break,
            // The DOCTYPE is skipped unread: no entity it declares is ever
            // expanded. Comments, processing instructions and the XML
            // declaration carry nothing calrs reads.
            Event::DocType(_) | Event::Comment(_) | Event::PI(_) | Event::Decl(_) => {}
        }
        if builder.root.is_some() {
            // The root closed: what follows is not part of the document.
            break;
        }
    }
    builder.finish()
}

/// The character a reference stands for: one of the five predefined
/// entities, or a character reference (`&#65;`, `&#x41;`, `&#X41;`). `None`
/// for anything else, including a reference to NUL, a surrogate or a code
/// point past U+10FFFF, which the caller keeps as written.
fn decode_reference(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) if hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
                    u32::from_str_radix(hex, 16).ok()?
                }
                Some(_) => return None,
                None if number.bytes().all(|b| b.is_ascii_digit()) => number.parse().ok()?,
                None => return None,
            };
            char::from_u32(code).filter(|&c| c != '\0')
        }
    }
}

/// Longest reference name [`decode_attribute`] looks at, so a value full of `&`
/// and no `;` stays linear. Every reference it decodes is far shorter.
const MAX_REFERENCE: usize = 32;

/// An attribute value as XML 1.0 reads it (3.3.3): each literal tab, line
/// feed, carriage return or CRLF becomes a space, then references are
/// decoded the way [`parse`] decodes them in text (the predefined entities
/// and character references, anything else kept as written). A character
/// reference to a tab or line feed stays that character.
fn decode_attribute(raw: &str) -> String {
    let normalized;
    let raw = if raw.contains(['\t', '\n', '\r']) {
        normalized = raw.replace("\r\n", " ").replace(['\t', '\n', '\r'], " ");
        normalized.as_str()
    } else {
        raw
    };
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let window = &after.as_bytes()[..after.len().min(MAX_REFERENCE + 1)];
        let decoded = window
            .iter()
            .position(|&b| b == b';')
            .and_then(|semi| Some((semi, decode_reference(&after[..semi])?)));
        match decoded {
            Some((semi, c)) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn utf8(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|e| anyhow!("XML name is not UTF-8: {e}"))
}

/// Builds the tree from the reader's events, enforcing the caps.
struct Builder<'l> {
    limits: &'l Limits,
    stack: Vec<Element>,
    /// Namespace declarations made by each open element, and their sum.
    declarations: Vec<usize>,
    bindings: usize,
    root: Option<Element>,
    text: String,
    elements: usize,
    attributes: usize,
    interned: std::collections::HashSet<Arc<str>>,
}

impl<'l> Builder<'l> {
    fn new(limits: &'l Limits) -> Self {
        Self {
            limits,
            stack: Vec::new(),
            declarations: Vec::new(),
            bindings: 0,
            root: None,
            text: String::new(),
            elements: 0,
            attributes: 0,
            interned: std::collections::HashSet::new(),
        }
    }

    /// A shared copy of `text`, so the names and namespaces repeated on
    /// every response cost one allocation each.
    fn intern(&mut self, text: &str) -> Arc<str> {
        if let Some(known) = self.interned.get(text) {
            return known.clone();
        }
        let shared: Arc<str> = Arc::from(text);
        if self.interned.len() < MAX_INTERNED {
            self.interned.insert(shared.clone());
        }
        shared
    }

    fn namespace(&mut self, resolved: &ResolveResult<'_>) -> Result<Option<Arc<str>>> {
        let ResolveResult::Bound(ns) = resolved else {
            return Ok(None);
        };
        let uri = std::str::from_utf8(ns.into_inner())
            .map_err(|e| anyhow!("XML namespace is not UTF-8: {e}"))?;
        if uri.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.intern(uri)))
    }

    fn open(&mut self, ns: Option<Arc<str>>, start: &BytesStart<'_>) -> Result<()> {
        let name = utf8(start.local_name().into_inner())?;
        // The namespace declarations this element brings into scope, and
        // the unqualified attributes it keeps.
        let mut declarations = 0;
        let mut attrs = Vec::new();
        for attr in start.attributes().with_checks(false) {
            let attr =
                attr.map_err(|e| anyhow!("malformed XML attribute: {}", quoted(&e.to_string())))?;
            let key = attr.key.into_inner();
            if key == b"xmlns" || key.starts_with(b"xmlns:") {
                declarations += 1;
            } else if !key.contains(&b':') {
                self.attributes += 1;
                if self.attributes > self.limits.attributes {
                    bail!(
                        "XML document has more than {} attributes",
                        self.limits.attributes
                    );
                }
                let value = std::str::from_utf8(&attr.value)
                    .map_err(|e| anyhow!("XML attribute is not UTF-8: {e}"))?;
                attrs.push((self.intern(utf8(key)?), decode_attribute(value)));
            }
        }
        self.bindings += declarations;
        if self.bindings > self.limits.bindings {
            bail!(
                "XML declares more than {} namespaces in scope",
                self.limits.bindings
            );
        }
        if self.stack.len() >= self.limits.depth {
            bail!("XML nested deeper than {} elements", self.limits.depth);
        }
        self.elements += 1;
        if self.elements > self.limits.elements {
            bail!(
                "XML document has more than {} elements",
                self.limits.elements
            );
        }
        self.flush_text();
        let name = self.intern(name);
        self.stack.push(Element {
            ns,
            name,
            attrs: attrs.into_boxed_slice(),
            children: Vec::new(),
        });
        self.declarations.push(declarations);
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        self.flush_text();
        let mut el = self
            .stack
            .pop()
            .ok_or_else(|| anyhow!("malformed XML: close tag without an open one"))?;
        self.bindings -= self.declarations.pop().unwrap_or(0);
        el.children.shrink_to_fit();
        match self.stack.last_mut() {
            Some(parent) => parent.children.push(Node::Element(el)),
            None => self.root = Some(el),
        }
        Ok(())
    }

    fn push_text(&mut self, text: &str) -> Result<()> {
        if !self.stack.is_empty() {
            self.text.push_str(text);
        } else if !text.trim().is_empty() {
            // Before the root (reading stops after it): not an XML document.
            bail!(
                "malformed XML: text before the root element: {}",
                quoted(text.trim())
            );
        }
        Ok(())
    }

    fn flush_text(&mut self) {
        if self.text.trim().is_empty() {
            self.text.clear();
        } else if let Some(parent) = self.stack.last_mut() {
            parent
                .children
                .push(Node::Text(std::mem::take(&mut self.text)));
        }
    }

    fn finish(self) -> Result<Option<Element>> {
        if let Some(open) = self.stack.last() {
            bail!(
                "malformed XML: document ends inside <{}>",
                quoted(open.local_name())
            );
        }
        Ok(self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(xml: &str) -> Element {
        parse(xml).unwrap().unwrap()
    }

    #[test]
    fn resolves_prefixes_and_default_namespaces() {
        let doc = root(
            r#"<a:multistatus xmlns:a="DAV:" xmlns:z="urn:ietf:params:xml:ns:caldav">
<a:response><href>no default here</href><z:calendar/></a:response>
<response xmlns="DAV:"><href>/x</href><calendar xmlns="http://groupdav.org/"/></response>
</a:multistatus>"#,
        );
        assert_eq!(doc.local_name(), "multistatus");
        assert_eq!(doc.namespace(), Some("DAV:"));
        let responses: Vec<_> = doc.children("DAV:", "response").collect();
        assert_eq!(responses.len(), 2);
        // An unprefixed name with no default namespace in scope is in none.
        let href = responses[0].child("DAV:", "href").unwrap();
        assert_eq!(href.namespace(), None);
        let cal = responses[0].elements().nth(1).unwrap();
        assert_eq!(cal.namespace(), Some("urn:ietf:params:xml:ns:caldav"));
        assert!(cal.is("urn:ietf:params:xml:ns:caldav", "calendar"));
        // The default namespace applies to the element and its children,
        // until a child declares another one.
        assert_eq!(
            responses[1].child("DAV:", "href").unwrap().namespace(),
            Some("DAV:")
        );
        let groupdav = responses[1].elements().nth(1).unwrap();
        assert_eq!(groupdav.namespace(), Some("http://groupdav.org/"));
        assert!(!groupdav.is("urn:ietf:params:xml:ns:caldav", "calendar"));
    }

    #[test]
    fn undeclared_prefix_matches_by_local_name_only() {
        let doc = root("<d:multistatus><d:href>/a</d:href></d:multistatus>");
        assert_eq!(doc.namespace(), None);
        assert!(doc.is("DAV:", "multistatus"));
        assert!(doc.is("anything", "multistatus"));
        assert!(!doc.is("DAV:", "response"));
    }

    #[test]
    fn text_unwraps_cdata_and_decodes_references() {
        let doc = root(
            "<x>Sales &amp; Marketing &#x2764;&#65; &lt;&gt;&quot;&apos;<![CDATA[ <b>&amp;</b> ]]>tail</x>",
        );
        assert_eq!(
            doc.text(),
            "Sales & Marketing \u{2764}A <>\"' <b>&amp;</b> tail"
        );
    }

    #[test]
    fn unknown_and_invalid_references_are_kept_as_written() {
        let doc = root("<x>a &unknown; b &#0; c &#xD800; d & e</x>");
        assert_eq!(doc.text(), "a &unknown; b &#0; c &#xD800; d & e");
    }

    #[test]
    fn doctype_entities_are_never_expanded() {
        // A billion-laughs document: the entities stay unexpanded references.
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [
 <!ENTITY lol "lol">
 <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
 <!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">
]>
<lolz>&lol3;</lolz>"#;
        assert_eq!(root(xml).text(), "&lol3;");
        // An external entity is not fetched or read either.
        let xml = r#"<!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><foo>&xxe;</foo>"#;
        assert_eq!(root(xml).text(), "&xxe;");
    }

    #[test]
    fn unqualified_attributes_are_kept_and_decoded() {
        let doc = root(
            r#"<t:ItemId xmlns:t="urn:t" Id="AA+/=" ChangeKey='a&amp;b &#x41; &bogus; & c' xml:lang="fr" t:Other="x"/>"#,
        );
        assert_eq!(doc.attr("Id"), Some("AA+/="));
        assert_eq!(doc.attr("ChangeKey"), Some("a&b A &bogus; & c"));
        // Prefixed attributes and namespace declarations are not kept.
        assert_eq!(doc.attr("lang"), None);
        assert_eq!(doc.attr("Other"), None);
        assert_eq!(doc.attr("t"), None);
        assert_eq!(doc.attr("xmlns:t"), None);
        assert_eq!(doc.attr("Missing"), None);
        assert_eq!(root("<a/>").attr("Id"), None);
    }

    #[test]
    fn attribute_whitespace_is_normalised() {
        let doc = root("<a v=\"x\ty\nz\r\nw\rq\" r=\"&#x9;&#xA;&#13;\"/>");
        assert_eq!(doc.attr("v"), Some("x y z w q"));
        assert_eq!(doc.attr("r"), Some("\t\n\r"));
    }

    #[test]
    fn attribute_decoding_stays_linear() {
        let value = "&".repeat(200_000);
        let doc = root(&format!(
            "<a v=\"{value}\" w=\"&#x41;&{long};\"/>",
            long = "x".repeat(40)
        ));
        assert_eq!(doc.attr("v").unwrap().len(), 200_000);
        assert_eq!(doc.attr("w"), Some(&*format!("A&{};", "x".repeat(40))));
    }

    #[test]
    fn malformed_attributes_are_errors() {
        for xml in ["<a Id=x/>", "<a Id/>", "<a Id=\"1\" =\"2\"/>"] {
            assert!(parse(xml).is_err(), "{xml:?} should be refused");
        }
    }

    #[test]
    fn whitespace_only_text_is_dropped_and_mixed_text_kept() {
        let doc = root("<a>\n  <b> x </b>\n  <c/>\n</a>");
        assert_eq!(doc.children.len(), 2);
        assert_eq!(doc.child("", "b").unwrap().text(), " x ");
        assert_eq!(
            doc.child("", "b").unwrap().trimmed_text().as_deref(),
            Some("x")
        );
        assert_eq!(doc.child("", "c").unwrap().trimmed_text(), None);
        assert!(!doc.child("", "c").unwrap().has_children());
    }

    #[test]
    fn text_is_borrowed_unless_split() {
        let doc = root("<a><b>one</b><c>x<d/>y</c><e/></a>");
        let kids: Vec<_> = doc.elements().collect();
        assert!(matches!(kids[0].text_ref(), Cow::Borrowed("one")));
        assert_eq!(kids[1].text_ref(), "xy");
        assert!(matches!(kids[1].text_ref(), Cow::Owned(_)));
        assert!(matches!(kids[2].text_ref(), Cow::Borrowed("")));
    }

    #[test]
    fn descendants_walk_in_document_order() {
        let doc = root("<a><b><c/><d/></b><e><f/></e></a>");
        let names: Vec<_> = doc.descendants().map(|el| el.local_name()).collect();
        assert_eq!(names, ["b", "c", "d", "e", "f"]);
        assert_eq!(doc.find("", "f").unwrap().local_name(), "f");
        assert!(doc.find("", "a").is_none());
    }

    #[test]
    fn byte_order_mark_declaration_and_comments_are_skipped() {
        let doc = root("\u{feff}<?xml version=\"1.0\"?><!-- hi --><a><?pi x?>t</a><!-- bye -->");
        assert_eq!(doc.local_name(), "a");
        assert_eq!(doc.text(), "t");
    }

    #[test]
    fn empty_documents_have_no_root() {
        assert!(parse("").unwrap().is_none());
        assert!(parse("  \n ").unwrap().is_none());
        assert!(parse("<?xml version=\"1.0\"?><!-- nothing -->")
            .unwrap()
            .is_none());
    }

    #[test]
    fn malformed_documents_are_errors() {
        for xml in [
            "<a><b></a>",
            "<a>",
            "<a></b>",
            "</a>",
            "Not Found",
            "garbage<a/>",
            "<a><b></b>",
            "<a x=\"1></a>",
            "<a><![CDATA[never closed</a>",
            "<a><!-- never closed</a>",
            "<html><body><br></body></html>",
        ] {
            assert!(parse(xml).is_err(), "{xml:?} should be refused");
        }
    }

    #[test]
    fn caps_are_enforced_as_errors() {
        let deep = "<a>".repeat(MAX_DEPTH + 1) + &"</a>".repeat(MAX_DEPTH + 1);
        let err = parse(&deep).unwrap_err().to_string();
        assert!(err.contains("nested deeper"), "{err}");
        // At the cap itself the document is fine.
        let ok = "<a>".repeat(MAX_DEPTH) + &"</a>".repeat(MAX_DEPTH);
        assert!(parse(&ok).unwrap().is_some());

        // A depth bomb far past the cap fails fast instead of overflowing.
        let bomb = "<a>".repeat(1_000_000);
        assert!(parse(&bomb).is_err());
    }

    #[test]
    fn element_count_and_size_caps_are_errors() {
        let limits = Limits {
            input_bytes: 1000,
            depth: 8,
            elements: 10,
            bindings: 4,
            attributes: 3,
        };
        // The root and nine children fit; one more does not.
        let fits = format!("<r>{}</r>", "<a/>".repeat(9));
        assert!(parse_with(&fits, &limits).unwrap().is_some());
        let many = format!("<r>{}</r>", "<a/>".repeat(10));
        let err = parse_with(&many, &limits).unwrap_err().to_string();
        assert!(err.contains("more than 10 elements"), "{err}");
        // Attributes are capped across the document; namespace
        // declarations do not count.
        let attrs = r#"<r xmlns:p="urn:p" a="1" p:z="2"><a b="2"/><a c="3"/></r>"#;
        assert!(parse_with(attrs, &limits).unwrap().is_some());
        let err = parse_with(r#"<r a="1"><a b="2"/><a c="3" d="4"/></r>"#, &limits)
            .unwrap_err()
            .to_string();
        assert!(err.contains("more than 3 attributes"), "{err}");
        let big = format!("<r>{}</r>", "x".repeat(1000));
        let err = parse_with(&big, &limits).unwrap_err().to_string();
        assert!(err.contains("too large"), "{err}");
    }

    #[test]
    fn reading_stops_when_the_root_closes() {
        // Baikal or Nextcloud with PHP `display_errors` on append warnings
        // after the document.
        let xml = "<d:multistatus xmlns:d=\"DAV:\"><d:response/></d:multistatus>\n<br />\n<b>Deprecated</b>:  Creation of dynamic property Sabre\\DAV\\Server::$debugExceptions is deprecated in <b>/var/www/baikal/vendor/sabre/dav/lib/DAV/Server.php</b> on line <b>233</b><br />\n";
        let doc = root(xml);
        assert_eq!(doc.local_name(), "multistatus");
        assert_eq!(doc.elements().count(), 1);
        // A second root, or broken markup after the first, is never read.
        assert_eq!(root("<a/><b/>").local_name(), "a");
        assert_eq!(root("<a></a></b><c").local_name(), "a");
    }

    #[test]
    fn namespace_declarations_in_scope_are_capped() {
        let declare =
            |n: usize| -> String { (0..n).map(|i| format!(" xmlns:p{i}=\"urn:{i}\"")).collect() };
        let at_cap = format!("<r{}/>", declare(MAX_NAMESPACE_BINDINGS));
        assert!(parse(&at_cap).unwrap().is_some());
        let over = format!("<r{}/>", declare(MAX_NAMESPACE_BINDINGS + 1));
        let err = parse(&over).unwrap_err().to_string();
        assert!(err.contains("namespaces in scope"), "{err}");

        // Counted across the open elements, and released when they close.
        let limits = Limits {
            input_bytes: 10_000,
            depth: 8,
            elements: 100,
            bindings: 4,
            attributes: 3,
        };
        let siblings = r#"<a xmlns:x="urn:1" xmlns:y="urn:2"><b xmlns:z="urn:3" xmlns="urn:4"/><c xmlns:w="urn:5" xmlns:v="urn:6"/></a>"#;
        assert!(parse_with(siblings, &limits).unwrap().is_some());
        let nested = r#"<a xmlns:x="urn:1" xmlns:y="urn:2"><b xmlns:z="urn:3" xmlns="urn:4"><c xmlns:w="urn:5"/></b></a>"#;
        assert!(parse_with(nested, &limits).is_err());
    }

    #[test]
    fn names_and_namespaces_are_shared() {
        let doc = root(r#"<r xmlns="DAV:"><href>/a</href><href>/b</href></r>"#);
        let hrefs: Vec<_> = doc.elements().collect();
        assert!(Arc::ptr_eq(&hrefs[0].name, &hrefs[1].name));
        assert!(Arc::ptr_eq(
            hrefs[0].ns.as_ref().unwrap(),
            hrefs[1].ns.as_ref().unwrap()
        ));
    }

    #[test]
    fn character_references_in_every_form() {
        assert_eq!(
            root("<x>&#65;&#x42;&#X43;&#x1F600;</x>").text(),
            "ABC\u{1F600}"
        );
        // Invalid code points stay as written, and nothing panics.
        for reference in [
            "&#0;",
            "&#x0;",
            "&#xD800;",
            "&#x110000;",
            "&#99999999999999999999;",
            "&#xFFFFFFFFFF;",
            "&#;",
            "&#x;",
            "&#xZZ;",
            "&#-1;",
            "&#+65;",
        ] {
            assert_eq!(root(&format!("<x>{reference}</x>")).text(), reference);
        }
    }

    #[test]
    fn server_controlled_names_are_bounded_in_errors() {
        let long = "n".repeat(10_000);
        for xml in [
            format!("<{long}>"),
            format!("<a></{long}>"),
            format!("{long}<a/>"),
        ] {
            let err = parse(&xml).unwrap_err().to_string();
            assert!(err.len() < 300, "{} bytes: {}", err.len(), &err[..100]);
        }
    }

    #[test]
    fn many_namespaces_stay_linear() {
        let mut xml = String::from("<r>");
        for i in 0..5000 {
            xml.push_str(&format!("<x xmlns=\"urn:ns:{i}\"/>"));
        }
        xml.push_str("</r>");
        let doc = root(&xml);
        assert_eq!(doc.elements().count(), 5000);
        assert_eq!(
            doc.elements().last().unwrap().namespace(),
            Some("urn:ns:4999")
        );
    }
}
