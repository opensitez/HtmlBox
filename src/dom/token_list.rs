//! `DOMTokenList` — DOM §7.1.
//!
//! One type behind `classList`, `relList`, `iframe.sandbox`, `output.htmlFor`
//! and `element.part`: all of them are an ordered set of tokens serialized
//! into a single space-separated attribute. Writing it once is the point —
//! `classList` had four bespoke methods here, none of which deduped, took
//! several tokens, or answered `length`.
//!
//! Behaviour is Chrome-verified (`/tmp/webcore-html/dtl.html`). Two details
//! that are easy to get wrong and that the probe pinned down:
//!
//! * `value` is the attribute string **verbatim** — for `class="a a b  c"` it
//!   is `"a a b  c"`, while `length` is 3. The stringifier reflects the
//!   attribute; the indexed access reflects the parsed set.
//! * A mutation rewrites the attribute as the serialized set (`"a b c x y"`),
//!   but a mutation that changes nothing on an element with no such attribute
//!   does not create one: `remove()` on a class-less element leaves `class`
//!   null rather than empty.

use crate::types::Document;

/// The two exceptions `DOMTokenList` mutations throw.
///
/// The spec throws; this crate has no exception channel, so they are values.
/// They are kept distinct rather than folded into one "bad token" because the
/// distinction is observable — Chrome answers `SyntaxError` for the empty
/// string and `InvalidCharacterError` for a token containing whitespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenError {
    /// The token was the empty string.
    Syntax,
    /// The token contained ASCII whitespace.
    InvalidCharacter,
}

/// Validate one token the way DOM §7.1 does, before any mutation happens.
pub fn validate_token(token: &str) -> Result<(), TokenError> {
    if token.is_empty() {
        return Err(TokenError::Syntax);
    }
    if token.chars().any(|c| c.is_ascii_whitespace()) {
        return Err(TokenError::InvalidCharacter);
    }
    Ok(())
}

/// DOM §2.4 "ordered set parser" — split on ASCII whitespace, drop duplicates,
/// keep first-occurrence order.
pub fn parse_ordered_set(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in value.split_ascii_whitespace() {
        if !out.iter().any(|t| t == token) {
            out.push(token.to_string());
        }
    }
    out
}

/// DOM §2.4 "ordered set serializer".
pub fn serialize_ordered_set(tokens: &[String]) -> String {
    tokens.join(" ")
}

/// The tokens `supports()` recognises for a given attribute, or `None` when
/// the attribute has no supported-tokens definition — which is a `TypeError`,
/// not a `false`. `classList` is the `None` case, and Chrome throws for it.
fn supported_tokens(attr: &str) -> Option<&'static [&'static str]> {
    match attr {
        // HTML §4.6.7 link types, the subset that is not author-defined.
        "rel" => Some(&[
            "alternate",
            "author",
            "bookmark",
            "canonical",
            "dns-prefetch",
            "expect",
            "external",
            "help",
            "icon",
            "license",
            "manifest",
            "modulepreload",
            "next",
            "nofollow",
            "noopener",
            "noreferrer",
            "opener",
            "pingback",
            "preconnect",
            "prefetch",
            "preload",
            "prev",
            "privacy-policy",
            "search",
            "stylesheet",
            "tag",
            "terms-of-service",
        ]),
        // HTML §4.8.5 sandboxing flags.
        "sandbox" => Some(&[
            "allow-downloads",
            "allow-forms",
            "allow-modals",
            "allow-orientation-lock",
            "allow-pointer-lock",
            "allow-popups",
            "allow-popups-to-escape-sandbox",
            "allow-presentation",
            "allow-same-origin",
            "allow-scripts",
            "allow-top-navigation",
            "allow-top-navigation-by-user-activation",
            "allow-top-navigation-to-custom-protocols",
        ]),
        _ => None,
    }
}

/// A read-only view of one token-list attribute.
pub struct TokenList<'a> {
    pub(crate) doc: &'a Document,
    pub(crate) id: u32,
    pub(crate) attr: &'static str,
}

impl TokenList<'_> {
    fn tokens(&self) -> Vec<String> {
        parse_ordered_set(&self.value())
    }

    /// The attribute string, verbatim. `class="a a b  c"` stringifies to
    /// `"a a b  c"` and NOT to the serialized set.
    pub fn value(&self) -> String {
        self.doc
            .get_attribute(self.id, self.attr)
            .unwrap_or_default()
    }

    /// The number of DISTINCT tokens — 3 for `class="a a b  c"`.
    pub fn length(&self) -> usize {
        self.tokens().len()
    }

    pub fn item(&self, index: usize) -> Option<String> {
        self.tokens().into_iter().nth(index)
    }

    pub fn contains(&self, token: &str) -> bool {
        self.tokens().iter().any(|t| t == token)
    }

    /// `supports(token)`. `None` is the `TypeError` Chrome throws when the
    /// attribute defines no supported tokens, which is every `classList`.
    pub fn supports(&self, token: &str) -> Option<bool> {
        supported_tokens(self.attr).map(|list| list.iter().any(|t| t.eq_ignore_ascii_case(token)))
    }

    /// The parsed set, for iteration — `DOMTokenList` is `iterable<DOMString>`.
    pub fn values(&self) -> Vec<String> {
        self.tokens()
    }
}

/// A mutable view of one token-list attribute.
pub struct TokenListMut<'a> {
    pub(crate) doc: &'a mut Document,
    pub(crate) id: u32,
    pub(crate) attr: &'static str,
}

impl TokenListMut<'_> {
    fn view(&self) -> TokenList<'_> {
        TokenList {
            doc: self.doc,
            id: self.id,
            attr: self.attr,
        }
    }

    pub fn value(&self) -> String {
        self.view().value()
    }
    pub fn length(&self) -> usize {
        self.view().length()
    }
    pub fn item(&self, index: usize) -> Option<String> {
        self.view().item(index)
    }
    pub fn contains(&self, token: &str) -> bool {
        self.view().contains(token)
    }
    pub fn supports(&self, token: &str) -> Option<bool> {
        self.view().supports(token)
    }
    pub fn values(&self) -> Vec<String> {
        self.view().values()
    }

    /// The stringifier setter — writes the attribute verbatim, without
    /// normalising. Chrome on `classList.value = "  p   q "` leaves
    /// `class="  p   q "` and answers `length` 2.
    pub fn set_value(&mut self, value: &str) {
        self.doc.set_attribute(self.id, self.attr, value);
    }

    fn write(&mut self, tokens: &[String]) {
        self.doc
            .set_attribute(self.id, self.attr, &serialize_ordered_set(tokens));
    }

    /// Every token is validated BEFORE anything is written, so a bad token in
    /// the middle of a call leaves the attribute untouched rather than
    /// half-applied.
    fn validate_all(tokens: &[&str]) -> Result<(), TokenError> {
        for t in tokens {
            validate_token(t)?;
        }
        Ok(())
    }

    pub fn add(&mut self, tokens: &[&str]) -> Result<(), TokenError> {
        Self::validate_all(tokens)?;
        let mut set = self.view().values();
        for t in tokens {
            if !set.iter().any(|s| s == t) {
                set.push((*t).to_string());
            }
        }
        self.write(&set);
        Ok(())
    }

    pub fn remove(&mut self, tokens: &[&str]) -> Result<(), TokenError> {
        Self::validate_all(tokens)?;
        // An element with no such attribute keeps having none. Chrome:
        // `remove` on a class-less div leaves `getAttribute("class")` null.
        if self.doc.get_attribute(self.id, self.attr).is_none() {
            return Ok(());
        }
        let mut set = self.view().values();
        set.retain(|s| !tokens.iter().any(|t| s == t));
        self.write(&set);
        Ok(())
    }

    /// `toggle(token, force)` — returns whether the token is present AFTER.
    pub fn toggle(&mut self, token: &str, force: Option<bool>) -> Result<bool, TokenError> {
        validate_token(token)?;
        let present = self.contains(token);
        let want = force.unwrap_or(!present);
        if want && !present {
            self.add(&[token])?;
        }
        if !want && present {
            self.remove(&[token])?;
        }
        Ok(want)
    }

    /// `replace(token, newToken)` — false, and no write at all, when `token`
    /// is not in the list.
    pub fn replace(&mut self, token: &str, new_token: &str) -> Result<bool, TokenError> {
        validate_token(token)?;
        validate_token(new_token)?;
        let mut set = self.view().values();
        let Some(i) = set.iter().position(|s| s == token) else {
            return Ok(false);
        };
        match set.iter().position(|s| s == new_token) {
            // Already present elsewhere: the old token is dropped rather than
            // duplicating the new one.
            Some(j) if j != i => {
                set.remove(i);
            }
            _ => set[i] = new_token.to_string(),
        }
        self.write(&set);
        Ok(true)
    }
}
