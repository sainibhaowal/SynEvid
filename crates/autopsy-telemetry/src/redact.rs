//! Telemetry privacy and redaction (Step 40).
//!
//! # The ordering rule, and why it is the whole step
//!
//! **Redaction happens before hashing.** A digest computed over raw input is a
//! fingerprint of the secret it contained: two runs carrying the same token would
//! correlate, and anyone holding the digest could confirm a guessed token by
//! recomputing it. So the digest is taken over the *sanitized* value, and the
//! property that falls out is the one worth asserting —
//! **span identity does not depend on redacted material**. Two observations of
//! `GET /users?token=A` and `GET /users?token=B` are the same route and must be
//! indistinguishable in the output.
//!
//! # Redaction is recursive, or it is theatre
//!
//! A token can sit in a span attribute, inside an `arrayValue`, inside a
//! `kvlistValue`, in a span **event** attribute (`exception.message` is the
//! classic leak), in a **link** attribute, in the span **name**, or in the
//! `status.message` the specification explicitly says "MAY contain sensitive
//! information". A sanitizer that walks only the top-level `attributes` array
//! redacts the easy case and passes the leak through, while looking complete.
//! [`Redactor`] is therefore driven by a single recursive `value` walk, and every
//! call site goes through it.
//!
//! # Two detection layers, because either one alone has a hole
//!
//! *Table* rules key on the attribute's **name** (`authorization`,
//! `x-api-key`, `url.query`, `enduser.id`, …). They are exact and cannot
//! misfire, but they miss a token smuggled into a value under an innocuous key —
//! `url.full`, `code.filepath`, a span name, an exception message.
//!
//! *Shape* rules key on the value's **form** (a JWT, an `Authorization: Bearer`
//! scheme prefix, an AWS access key id, PEM key material, an IP literal, an
//! e-mail address, a Luhn-valid card number). They catch the smuggled case, but
//! the fuzzier ones can misfire on legitimate values.
//!
//! Both layers are always applied to ordinary keys. **Structural** keys — the
//! correlation surface of §4.1 (`code.function`, `code.filepath`, `http.route`,
//! `rpc.service`, `rpc.method`, `service.name`, …) — are checked against the
//! high-confidence shape rules only. The reason is that over-redacting a
//! structural key silently destroys the fusion this pillar exists to perform:
//! `service.name = "checkout@1.0.0"` must not be read as an e-mail address and
//! replaced, and `code.function = "send.1"` must not be read as an IPv4 literal.
//! A JWT, a `Bearer` prefix, an AWS key id or PEM material cannot plausibly be a
//! service name, so those four still apply.
//!
//! # What is not attempted
//!
//! Free-text PII **inside prose** is not scrubbed. `exception.message` may
//! contain a customer's name or an address, and no bounded pattern finds that.
//! Rather than claim otherwise, a free-text value is marked
//! [`SanitizedValue::FreeText`] and the span's coverage drops to `Partial`, which
//! blocks a clean verdict (FR-014, FR-030). The alternative — reporting a
//! sanitized trace while arbitrary prose went through unexamined — is the
//! `UNKNOWN`-as-`PASS` failure this codebase forbids.
//!
//! # Nothing here needs a regular expression
//!
//! Every detector is a hand-written linear scan. That keeps the crate free of a
//! regex engine (no supplied-chain edge) and, more importantly, makes
//! catastrophic backtracking structurally impossible: a redactor that can hang on
//! attacker-controlled input is a denial-of-service boundary, and trace attributes
//! are attacker-controlled.

use crate::otlp::{AnyValue, KeyValue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Why a value was removed.
///
/// Deliberately does **not** carry the removed value, or its length. A length is
/// a side channel: a 40-character secret and a 16-character one distinguish
/// provider formats, which is exactly the information a redactor is meant to
/// remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionKind {
    /// An `Authorization` / `Proxy-Authorization` header, by key or by scheme.
    AuthorizationHeader,
    /// A `Cookie` or `Set-Cookie` header.
    CookieHeader,
    /// An API-key header.
    ApiKey,
    /// A password, passphrase, or client secret.
    Credential,
    /// A bearer/refresh/CSRF token under a key that says so.
    Token,
    /// A session identifier.
    SessionKey,
    /// An opaque request or response body.
    Payload,
    /// A raw query string.
    QueryString,
    /// A URL fragment. Tokens ride here in implicit-flow OAuth.
    UrlFragment,
    /// A `user:password@` component of a URL.
    UrlUserInfo,
    /// An IP address or MAC address literal.
    NetworkAddress,
    /// An e-mail address.
    EmailAddress,
    /// A JSON Web Token, wherever it appeared.
    JsonWebToken,
    /// An AWS access key id.
    AwsAccessKeyId,
    /// PEM private-key material.
    PrivateKeyMaterial,
    /// A Luhn-valid payment card number.
    CreditCardNumber,
    /// An account or end-user identifier.
    UserIdentifier,
    /// A `bytesValue`, whose contents cannot be classified at all.
    OpaqueBinaryValue,
}

impl RedactionKind {
    /// Every variant, for exhaustive schema drift checking.
    pub const ALL: [RedactionKind; 18] = [
        RedactionKind::AuthorizationHeader,
        RedactionKind::CookieHeader,
        RedactionKind::ApiKey,
        RedactionKind::Credential,
        RedactionKind::Token,
        RedactionKind::SessionKey,
        RedactionKind::Payload,
        RedactionKind::QueryString,
        RedactionKind::UrlFragment,
        RedactionKind::UrlUserInfo,
        RedactionKind::NetworkAddress,
        RedactionKind::EmailAddress,
        RedactionKind::JsonWebToken,
        RedactionKind::AwsAccessKeyId,
        RedactionKind::PrivateKeyMaterial,
        RedactionKind::CreditCardNumber,
        RedactionKind::UserIdentifier,
        RedactionKind::OpaqueBinaryValue,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthorizationHeader => "authorization_header",
            Self::CookieHeader => "cookie_header",
            Self::ApiKey => "api_key",
            Self::Credential => "credential",
            Self::Token => "token",
            Self::SessionKey => "session_key",
            Self::Payload => "payload",
            Self::QueryString => "query_string",
            Self::UrlFragment => "url_fragment",
            Self::UrlUserInfo => "url_user_info",
            Self::NetworkAddress => "network_address",
            Self::EmailAddress => "email_address",
            Self::JsonWebToken => "json_web_token",
            Self::AwsAccessKeyId => "aws_access_key_id",
            Self::PrivateKeyMaterial => "private_key_material",
            Self::CreditCardNumber => "credit_card_number",
            Self::UserIdentifier => "user_identifier",
            Self::OpaqueBinaryValue => "opaque_binary_value",
        }
    }
}

/// A sanitized value. `Eq` and `Ord` are derivable because every variant is
/// comparable — which is why the reader canonicalises `doubleValue` to literal
/// text rather than carrying an `f64`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SanitizedValue {
    /// Kept, and classified as carrying no identified sensitive material.
    Text(String),
    /// Kept, but it is prose whose PII content could not be classified. Blocks a
    /// clean verdict.
    FreeText(String),
    /// A number, as canonical decimal text.
    Number(String),
    Flag(bool),
    /// Removed. The reason survives; the value does not.
    Redacted(RedactionKind),
    List(Vec<SanitizedValue>),
    Map(Vec<SanitizedEntry>),
    /// A node the reader could not represent. The variant names it declared are
    /// kept so the disclosure can name what was found.
    Unmodelled(BTreeSet<String>),
}

/// One sanitized `KeyValue`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SanitizedEntry {
    pub key: String,
    pub value: SanitizedValue,
}

/// Whether a key-table rule matches the whole attribute key or its final path
/// segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyMatch {
    /// `http.request.header.authorization` matches `authorization`.
    Segment,
    /// `enduser.id` must match whole: matching on the segment `id` would redact
    /// every key ending in `.id`, including the ones Step 41 correlates on.
    Whole,
}

struct KeyRule {
    needle: &'static str,
    mode: KeyMatch,
    kind: RedactionKind,
}

const KEY_RULES: &[KeyRule] = &[
    KeyRule {
        needle: "authorization",
        mode: KeyMatch::Segment,
        kind: RedactionKind::AuthorizationHeader,
    },
    KeyRule {
        needle: "proxy-authorization",
        mode: KeyMatch::Segment,
        kind: RedactionKind::AuthorizationHeader,
    },
    KeyRule {
        needle: "proxy_authorization",
        mode: KeyMatch::Segment,
        kind: RedactionKind::AuthorizationHeader,
    },
    KeyRule {
        needle: "cookie",
        mode: KeyMatch::Segment,
        kind: RedactionKind::CookieHeader,
    },
    KeyRule {
        needle: "set-cookie",
        mode: KeyMatch::Segment,
        kind: RedactionKind::CookieHeader,
    },
    KeyRule {
        needle: "set_cookie",
        mode: KeyMatch::Segment,
        kind: RedactionKind::CookieHeader,
    },
    KeyRule {
        needle: "x-api-key",
        mode: KeyMatch::Segment,
        kind: RedactionKind::ApiKey,
    },
    KeyRule {
        needle: "x_api_key",
        mode: KeyMatch::Segment,
        kind: RedactionKind::ApiKey,
    },
    KeyRule {
        needle: "api-key",
        mode: KeyMatch::Segment,
        kind: RedactionKind::ApiKey,
    },
    KeyRule {
        needle: "apikey",
        mode: KeyMatch::Segment,
        kind: RedactionKind::ApiKey,
    },
    KeyRule {
        needle: "password",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Credential,
    },
    KeyRule {
        needle: "passwd",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Credential,
    },
    KeyRule {
        needle: "secret",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Credential,
    },
    KeyRule {
        needle: "client_secret",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Credential,
    },
    KeyRule {
        needle: "client-secret",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Credential,
    },
    KeyRule {
        needle: "private_key",
        mode: KeyMatch::Segment,
        kind: RedactionKind::PrivateKeyMaterial,
    },
    KeyRule {
        needle: "private-key",
        mode: KeyMatch::Segment,
        kind: RedactionKind::PrivateKeyMaterial,
    },
    KeyRule {
        needle: "token",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Token,
    },
    KeyRule {
        needle: "access_token",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Token,
    },
    KeyRule {
        needle: "refresh_token",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Token,
    },
    KeyRule {
        needle: "id_token",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Token,
    },
    KeyRule {
        needle: "x-auth-token",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Token,
    },
    KeyRule {
        needle: "x-csrf-token",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Token,
    },
    KeyRule {
        needle: "csrf_token",
        mode: KeyMatch::Segment,
        kind: RedactionKind::Token,
    },
    KeyRule {
        needle: "session",
        mode: KeyMatch::Segment,
        kind: RedactionKind::SessionKey,
    },
    KeyRule {
        needle: "session_id",
        mode: KeyMatch::Segment,
        kind: RedactionKind::SessionKey,
    },
    KeyRule {
        needle: "sessionid",
        mode: KeyMatch::Segment,
        kind: RedactionKind::SessionKey,
    },
    KeyRule {
        needle: "query",
        mode: KeyMatch::Segment,
        kind: RedactionKind::QueryString,
    },
    KeyRule {
        needle: "enduser.id",
        mode: KeyMatch::Whole,
        kind: RedactionKind::UserIdentifier,
    },
    KeyRule {
        needle: "enduser.scope",
        mode: KeyMatch::Whole,
        kind: RedactionKind::UserIdentifier,
    },
    KeyRule {
        needle: "user.id",
        mode: KeyMatch::Whole,
        kind: RedactionKind::UserIdentifier,
    },
    KeyRule {
        needle: "db.statement",
        mode: KeyMatch::Whole,
        kind: RedactionKind::Payload,
    },
    KeyRule {
        needle: "http.request.body.content",
        mode: KeyMatch::Whole,
        kind: RedactionKind::Payload,
    },
    KeyRule {
        needle: "http.response.body.content",
        mode: KeyMatch::Whole,
        kind: RedactionKind::Payload,
    },
];

/// Keys whose value is a URL to be surgically stripped.
const URL_KEYS: &[&str] = &[
    "url.full",
    "http.url",
    "http.target",
    "url.query",
    "url.path",
    "http.request.header.referer",
    "http.request.header.referrer",
    "http.request.header.location",
];

/// Keys that carry the correlation surface. Value-shape redaction on these is
/// restricted to the high-confidence detectors, so a legitimate
/// `service.name = "checkout@1.0.0"` is not mistaken for an e-mail address and
/// silently erased — which would destroy the fusion rather than protect anyone.
const STRUCTURAL_KEYS: &[&str] = &[
    "code.function",
    "code.filepath",
    "code.namespace",
    "code.lineno",
    "http.route",
    "http.request.method",
    "http.response.status_code",
    "rpc.service",
    "rpc.method",
    "rpc.system",
    "messaging.destination.name",
    "messaging.system",
    "db.system",
    "db.namespace",
    "service.name",
    "service.namespace",
    "service.version",
];

/// Final path segments whose value is unclassified human prose.
///
/// A value here is kept and flagged, never silently trusted and never blanket
/// redacted: `exception.message` is the single most useful attribute for
/// understanding a failure, and replacing it outright would make the trace
/// useless while claiming to have sanitized it.
const FREE_TEXT_SEGMENTS: &[&str] = &["message", "stacktrace", "description", "detail"];

fn last_segment(key: &str) -> &str {
    key.rsplit(['.', '/']).next().unwrap_or(key)
}

/// The table rule for an attribute key, if any.
pub fn key_rule(key: &str) -> Option<RedactionKind> {
    let lower = key.to_ascii_lowercase();
    let segment = last_segment(&lower);
    for rule in KEY_RULES {
        let hit = match rule.mode {
            KeyMatch::Whole => lower == rule.needle,
            KeyMatch::Segment => segment == rule.needle || lower == rule.needle,
        };
        if hit {
            return Some(rule.kind);
        }
    }
    None
}

fn is_structural(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    STRUCTURAL_KEYS.contains(&lower.as_str())
}

fn is_free_text(key: &str) -> bool {
    FREE_TEXT_SEGMENTS.contains(&last_segment(&key.to_ascii_lowercase()))
}

fn is_url_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    URL_KEYS.contains(&lower.as_str())
}

// ---------------------------------------------------------------------------
// Shape detectors. Each one is a linear scan; none can backtrack.
// ---------------------------------------------------------------------------

/// Three dot-separated base64url segments, the first decoding to `{"`.
fn is_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if !header.starts_with("eyJ") || header.len() < 4 || payload.is_empty() || signature.is_empty()
    {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// An `Authorization` scheme prefix followed by an opaque credential.
fn has_auth_scheme(value: &str) -> bool {
    let trimmed = value.trim_start();
    let Some((scheme, credential)) = trimmed.split_once(' ') else {
        return false;
    };
    if !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "bearer" | "basic" | "digest" | "negotiate" | "apikey"
    ) {
        return false;
    }
    // `"bearer of good news"` is prose, not a credential. A scheme is followed by
    // exactly one non-empty token.
    !credential.is_empty() && !credential.chars().any(char::is_whitespace)
}

/// `AKIA`/`ASIA`/… followed by sixteen uppercase alphanumerics, at a boundary.
fn has_aws_access_key_id(value: &str) -> bool {
    const PREFIXES: [&str; 6] = ["AKIA", "ASIA", "AIDA", "AROA", "AGPA", "ANPA"];
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return false;
    }
    for start in 0..=bytes.len() - 20 {
        let window = &value[start..start + 20];
        if !PREFIXES.iter().any(|prefix| window.starts_with(*prefix)) {
            continue;
        }
        let body = &window[4..];
        let body_ok = body
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        let boundary_ok = bytes
            .get(start.wrapping_sub(1))
            .map(|c| !c.is_ascii_uppercase() && !c.is_ascii_digit())
            .unwrap_or(true);
        if body_ok && boundary_ok {
            return true;
        }
    }
    false
}

fn has_pem_private_key(value: &str) -> bool {
    value.contains("-----BEGIN ") && value.contains("PRIVATE KEY-----")
}

/// A whole value that is an e-mail address.
///
/// Whole-value only, deliberately. An address embedded in prose is not found by
/// this, and pretending otherwise would be the same overclaim as scrubbing prose
/// — the prose is flagged as free text instead (see the module docs).
fn is_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.contains('@') || local.len() > 64 {
        return false;
    }
    if !local
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c))
    {
        return false;
    }
    if domain.contains('/') || domain.contains(' ') {
        return false;
    }
    let Some((_, tld)) = domain.rsplit_once('.') else {
        return false;
    };
    // `checkout@1.0.0` is a version, not an address: the final label must be
    // alphabetic. This is the guard that keeps the fusion surface intact.
    tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_ipv4(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.chars().all(|c| c.is_ascii_digit())
                && part.parse::<u16>().is_ok_and(|n| n <= 255)
        })
}

/// An IPv6 or MAC literal.
///
/// The guard against clock times matters: `12:34:56` is two colons, all digits,
/// and would otherwise be indistinguishable from a short IPv6 group run. A real
/// IPv6 address either compresses (`::`) or spells all eight groups out.
fn is_ipv6_or_mac(value: &str) -> bool {
    let core = value.split('%').next().unwrap_or(value);
    if core.is_empty()
        || !core
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
    {
        return false;
    }
    let colons = core.matches(':').count();
    if core.contains("::") || colons == 7 {
        return true;
    }
    // Six two-hex groups: a MAC address.
    colons == 5
        && core
            .split(':')
            .all(|group| group.len() == 2 && group.chars().all(|c| c.is_ascii_hexdigit()))
}

fn luhn_valid(digits: &[u32]) -> bool {
    let mut sum = 0u32;
    for (index, digit) in digits.iter().rev().enumerate() {
        let mut value = *digit;
        if index % 2 == 1 {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
    }
    sum.is_multiple_of(10)
}

/// A Luhn-valid payment card number of 13–19 digits.
///
/// The checksum is what makes this usable: a bare digit run of that length would
/// redact timestamps, ids and build numbers.
fn is_credit_card(value: &str) -> bool {
    if !value
        .chars()
        .all(|c| c.is_ascii_digit() || c == ' ' || c == '-')
    {
        return false;
    }
    let digits: Vec<u32> = value.chars().filter_map(|c| c.to_digit(10)).collect();
    (13..=19).contains(&digits.len()) && luhn_valid(&digits)
}

/// The high-confidence shape rules: a JWT, an `Authorization` scheme, an AWS key
/// id or PEM material cannot plausibly be a structural identifier.
fn high_confidence_shape(value: &str) -> Option<RedactionKind> {
    if is_jwt(value) {
        return Some(RedactionKind::JsonWebToken);
    }
    if has_auth_scheme(value) {
        return Some(RedactionKind::AuthorizationHeader);
    }
    if has_aws_access_key_id(value) {
        return Some(RedactionKind::AwsAccessKeyId);
    }
    if has_pem_private_key(value) {
        return Some(RedactionKind::PrivateKeyMaterial);
    }
    None
}

/// The fuzzy shape rules, applied only to non-structural keys.
fn fuzzy_shape(value: &str) -> Option<RedactionKind> {
    if is_email(value) {
        return Some(RedactionKind::EmailAddress);
    }
    if is_ipv4(value) || is_ipv6_or_mac(value) {
        return Some(RedactionKind::NetworkAddress);
    }
    if is_credit_card(value) {
        return Some(RedactionKind::CreditCardNumber);
    }
    None
}

// ---------------------------------------------------------------------------
// URL surgery
// ---------------------------------------------------------------------------

/// What a URL strip removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlSurgery {
    pub cleaned: String,
    pub removed_query: bool,
    pub removed_fragment: bool,
    pub removed_userinfo: bool,
}

/// Removes the userinfo, query and fragment from a URL, keeping scheme, host and
/// path.
///
/// The path is kept **raw**, percent-encoding included, because it is an input to
/// route correlation and decoding it here would change what Step 41 matches on.
pub fn strip_url(raw: &str) -> UrlSurgery {
    let mut surgery = UrlSurgery::default();

    let (head, rest) = match raw.find("://") {
        Some(index) => raw.split_at(index + 3),
        None => ("", raw),
    };

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);

    // A password may itself contain `@`, so the split is on the *last* one.
    let authority = match authority.rfind('@') {
        Some(index) => {
            surgery.removed_userinfo = true;
            &authority[index + 1..]
        }
        None => authority,
    };

    let query_at = tail.find('?');
    let fragment_at = tail.find('#');
    let cut_at = match (query_at, fragment_at) {
        (Some(q), Some(h)) => Some(q.min(h)),
        (Some(q), None) => Some(q),
        (None, Some(h)) => Some(h),
        (None, None) => None,
    };
    let path = match cut_at {
        Some(index) => {
            if query_at == Some(index) {
                surgery.removed_query = true;
                if fragment_at.is_some() {
                    surgery.removed_fragment = true;
                }
            } else {
                surgery.removed_fragment = true;
            }
            &tail[..index]
        }
        None => tail,
    };

    surgery.cleaned = format!("{head}{authority}{path}");
    surgery
}

// ---------------------------------------------------------------------------
// The redactor
// ---------------------------------------------------------------------------

/// Applies the redaction policy, accumulating a count per reason.
///
/// Counts, never values. A per-value record would have to name the value.
#[derive(Debug, Default, Clone)]
pub struct Redactor {
    counts: BTreeMap<RedactionKind, usize>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn counts(&self) -> &BTreeMap<RedactionKind, usize> {
        &self.counts
    }

    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    fn record(&mut self, kind: RedactionKind) {
        *self.counts.entry(kind).or_insert(0) += 1;
    }

    /// Sanitizes a string value, applying URL surgery first when the key is a URL
    /// key and shape detection otherwise.
    fn string(&mut self, key: Option<&str>, value: &str) -> SanitizedValue {
        if let Some(key) = key
            && is_url_key(key)
        {
            let surgery = strip_url(value);
            if surgery.removed_query {
                self.record(RedactionKind::QueryString);
            }
            if surgery.removed_fragment {
                self.record(RedactionKind::UrlFragment);
            }
            if surgery.removed_userinfo {
                self.record(RedactionKind::UrlUserInfo);
            }
            // A URL with nothing to remove is returned as-is rather than through
            // the shape detectors: `https://api.example.com/v1/orders` is not an
            // e-mail address (no `@`), but a *relative* path key could be, and
            // the caller asked about a URL.
            return SanitizedValue::Text(surgery.cleaned);
        }

        let structural = key.is_some_and(is_structural);
        if let Some(kind) = high_confidence_shape(value) {
            self.record(kind);
            return SanitizedValue::Redacted(kind);
        }
        if !structural && let Some(kind) = fuzzy_shape(value) {
            self.record(kind);
            return SanitizedValue::Redacted(kind);
        }
        if key.is_some_and(is_free_text) {
            return SanitizedValue::FreeText(value.to_string());
        }
        SanitizedValue::Text(value.to_string())
    }

    /// Sanitizes any value, recursively.
    pub fn value(&mut self, key: Option<&str>, value: &AnyValue) -> SanitizedValue {
        // The key table is consulted **before** the value shape rules, and it
        // removes the whole subtree. Two reasons:
        //
        // * A key that names a credential is that credential, whatever its value
        //   happens to look like. `Authorization: Bearer abc` is an authorization
        //   header, not "a token", and reporting the vaguer reason would lose the
        //   one fact the reader can act on.
        // * An `arrayValue` or `kvlistValue` under a table rule must not have its
        //   elements walked and kept — the leak is inside them.
        if let Some(key) = key
            && let Some(kind) = key_rule(key)
        {
            self.record(kind);
            return SanitizedValue::Redacted(kind);
        }

        match value {
            AnyValue::Str(text) => self.string(key, text),
            AnyValue::Bool(flag) => SanitizedValue::Flag(*flag),
            AnyValue::Int(number) => SanitizedValue::Number(number.to_string()),
            AnyValue::Double(text) => SanitizedValue::Number(text.clone()),
            AnyValue::Bytes(_) => {
                // Not classified rather than unclassified: bytes cannot be read as
                // text, so no rule can see inside them.
                self.record(RedactionKind::OpaqueBinaryValue);
                SanitizedValue::Redacted(RedactionKind::OpaqueBinaryValue)
            }
            AnyValue::Array(items) => {
                SanitizedValue::List(items.iter().map(|item| self.value(None, item)).collect())
            }
            AnyValue::KvList(entries) => {
                let mut sanitized = self.attributes(entries);
                // Sorted by key: a `kvlistValue`'s entry order is formatting, and
                // allowing it to reach the digest would make the same span digest
                // differently depending on which exporter wrote it.
                sanitized.sort();
                SanitizedValue::Map(sanitized)
            }
            AnyValue::Unmodelled(variants) => SanitizedValue::Unmodelled(variants.clone()),
        }
    }

    /// Sanitizes an attribute list, sorted by key.
    pub fn attributes(&mut self, attributes: &[KeyValue]) -> Vec<SanitizedEntry> {
        let mut out: Vec<SanitizedEntry> = attributes
            .iter()
            .map(|attribute| SanitizedEntry {
                key: attribute.key.clone(),
                value: self.value(Some(&attribute.key), &attribute.value),
            })
            .collect();
        out.sort();
        out
    }

    /// Sanitizes a span name.
    ///
    /// Span names routinely carry the raw request target (`GET /users/2?token=x`)
    /// because the OpenTelemetry HTTP instrumentation defaults to it before a
    /// route is known. Running the URL machinery over the name is therefore not
    /// defensive; it is the common case.
    pub fn span_name(&mut self, name: &str) -> SanitizedValue {
        let surgery = strip_url(name);
        if surgery.removed_query {
            self.record(RedactionKind::QueryString);
        }
        if surgery.removed_fragment {
            self.record(RedactionKind::UrlFragment);
        }
        if surgery.removed_userinfo {
            self.record(RedactionKind::UrlUserInfo);
        }
        if let Some(kind) = high_confidence_shape(&surgery.cleaned) {
            self.record(kind);
            return SanitizedValue::Redacted(kind);
        }
        SanitizedValue::Text(surgery.cleaned)
    }

    /// The sanitized text of a structural key, for the correlation surface.
    ///
    /// Returns `None` when the value was removed: a structural key that matched a
    /// redaction rule must not be reported as a correlation target, or Step 41
    /// would correlate against a placeholder.
    pub fn structural_text(&mut self, key: &str, value: &AnyValue) -> Option<String> {
        match self.value(Some(key), value) {
            SanitizedValue::Text(text) | SanitizedValue::FreeText(text) => Some(text),
            SanitizedValue::Number(number) => Some(number),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any(text: &str) -> AnyValue {
        AnyValue::Str(text.to_string())
    }

    #[test]
    fn a_jwt_is_recognised_and_a_short_dotted_string_is_not() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.dBjftJeZ4CVP";
        assert!(is_jwt(jwt));
        assert!(!is_jwt("get.1.2"), "three segments alone are not a JWT");
        assert!(!is_jwt("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0"));
        assert!(!is_jwt("a.b.c"));
    }

    #[test]
    fn an_auth_scheme_needs_a_single_opaque_token() {
        assert!(has_auth_scheme("Bearer abc.def"));
        assert!(has_auth_scheme("basic YWJj"));
        assert!(!has_auth_scheme("bearer of good news"));
        assert!(!has_auth_scheme("Bearer"));
        assert!(!has_auth_scheme("application/json"));
    }

    #[test]
    fn an_aws_key_id_is_found_inside_a_longer_string() {
        assert!(has_aws_access_key_id("AKIAIOSFODNN7EXAMPLE"));
        assert!(has_aws_access_key_id("key=AKIAIOSFODNN7EXAMPLE;"));
        assert!(!has_aws_access_key_id("AKIAIOSFODNN7EXAMPL"));
        assert!(!has_aws_access_key_id("akiaiosfodnn7example"));
    }

    #[test]
    fn a_pem_header_is_recognised_but_a_public_key_is_not_treated_as_a_secret() {
        // Only *private* key material is redacted. A public key is not a secret,
        // and redacting it would remove evidence rather than protect anyone.
        assert!(has_pem_private_key("-----BEGIN RSA PRIVATE KEY-----\nMIIE"));
        assert!(!has_pem_private_key("-----BEGIN PUBLIC KEY-----\nMIIB"));
    }

    #[test]
    fn an_email_address_requires_an_alphabetic_final_label() {
        assert!(is_email("dana@example.com"));
        assert!(is_email("dana.scully+ci@example.co.uk"));
        // The guard that keeps the correlation surface intact.
        assert!(!is_email("checkout@1.0.0"));
        assert!(!is_email("payments@2.3.4"));
        assert!(!is_email("no-at-sign.example.com"));
        assert!(!is_email("a@b"));
        assert!(!is_email("@example.com"));
    }

    #[test]
    fn an_ipv6_literal_is_not_confused_with_a_clock_time() {
        assert!(is_ipv6_or_mac("2001:db8::1"));
        assert!(is_ipv6_or_mac("::1"));
        assert!(is_ipv6_or_mac("2001:0db8:85a3:0000:0000:8a2e:0370:7334"));
        assert!(is_ipv6_or_mac("fe80::1%eth0"));
        assert!(is_ipv6_or_mac("aa:bb:cc:dd:ee:ff"));
        assert!(
            !is_ipv6_or_mac("12:34:56"),
            "a clock time is not an address"
        );
        assert!(!is_ipv6_or_mac("12:34:56.789"));
        assert!(!is_ipv6_or_mac("build:1234"));
    }

    #[test]
    fn an_ipv4_literal_needs_four_in_range_octets() {
        assert!(is_ipv4("10.0.0.5"));
        assert!(is_ipv4("255.255.255.255"));
        assert!(!is_ipv4("256.0.0.1"));
        assert!(!is_ipv4("10.0.0"));
        assert!(!is_ipv4("10.0.0.5.6"));
        assert!(!is_ipv4("1.2.3.4.5"));
    }

    #[test]
    fn a_card_number_needs_a_valid_checksum_not_just_a_digit_run() {
        assert!(is_credit_card("4111111111111111"));
        assert!(is_credit_card("4111 1111 1111 1111"));
        assert!(
            !is_credit_card("4111111111111112"),
            "the same digits, one off"
        );
        assert!(!is_credit_card("1234567890123"));
        assert!(!is_credit_card("123456789012"), "too short");
        assert!(!is_credit_card("2026-10-16T12:34:56"), "not a digit run");
    }

    #[test]
    fn a_whole_key_rule_does_not_fire_on_a_suffix_rule() {
        assert_eq!(
            key_rule("http.request.header.authorization"),
            Some(RedactionKind::AuthorizationHeader)
        );
        assert_eq!(key_rule("url.query"), Some(RedactionKind::QueryString));
        assert_eq!(key_rule("db.statement"), Some(RedactionKind::Payload));
        assert_eq!(key_rule("enduser.id"), Some(RedactionKind::UserIdentifier));
        // `enduser.id` is a whole-key rule precisely so that other `.id` keys —
        // which Step 41 correlates on — are left alone.
        assert_eq!(key_rule("some.other.id"), None);
        assert_eq!(key_rule("code.function"), None);
        assert_eq!(key_rule("http.route"), None);
    }

    #[test]
    fn url_surgery_removes_userinfo_query_and_fragment() {
        let surgery = strip_url("https://alice:hunter2@api.example.com/v1/orders?token=abc#frag");
        assert_eq!(surgery.cleaned, "https://api.example.com/v1/orders");
        assert!(surgery.removed_query);
        assert!(surgery.removed_fragment);
        assert!(surgery.removed_userinfo);

        // A password containing `@`: the split is on the last one.
        let surgery = strip_url("https://user:p@ss@host/path");
        assert_eq!(surgery.cleaned, "https://host/path");

        // A relative target, which is what `http.target` carries.
        let surgery = strip_url("/users/2?token=abc");
        assert_eq!(surgery.cleaned, "/users/2");
        assert!(surgery.removed_query);

        // Nothing to remove.
        let surgery = strip_url("https://api.example.com/v1/orders");
        assert_eq!(surgery.cleaned, "https://api.example.com/v1/orders");
        assert!(!surgery.removed_query && !surgery.removed_fragment && !surgery.removed_userinfo);

        // `?` before `#`: the fragment goes with the query.
        let surgery = strip_url("/a?b=1#c");
        assert_eq!(surgery.cleaned, "/a");
        assert!(surgery.removed_query && surgery.removed_fragment);
    }

    #[test]
    fn a_query_string_embedded_in_a_value_key_is_removed() {
        let mut redactor = Redactor::new();
        let sanitized = redactor.value(
            Some("url.full"),
            &any("https://api.example.com/a?token=abc"),
        );
        assert_eq!(
            sanitized,
            SanitizedValue::Text("https://api.example.com/a".to_string())
        );
        assert_eq!(redactor.counts().get(&RedactionKind::QueryString), Some(&1));
    }

    #[test]
    fn a_table_rule_removes_the_whole_subtree_not_just_a_string() {
        // A `Cookie` header that is an array must not have its elements walked and
        // kept.
        let mut redactor = Redactor::new();
        let value = AnyValue::Array(vec![any("a=1"), any("b=2")]);
        assert_eq!(
            redactor.value(Some("http.request.header.cookie"), &value),
            SanitizedValue::Redacted(RedactionKind::CookieHeader)
        );
    }

    #[test]
    fn a_structural_key_is_immune_to_the_fuzzy_detectors() {
        let mut redactor = Redactor::new();
        // A service name that looks like a version, and a function name that looks
        // like an address. Redacting either would silently destroy the fusion.
        assert_eq!(
            redactor.value(Some("service.name"), &any("checkout@1.0.0")),
            SanitizedValue::Text("checkout@1.0.0".to_string())
        );
        assert_eq!(
            redactor.value(Some("code.function"), &any("10.0.0.5")),
            SanitizedValue::Text("10.0.0.5".to_string())
        );
        assert_eq!(redactor.total(), 0);

        // But a JWT in a structural key is still removed: it cannot plausibly be
        // a service name.
        assert_eq!(
            redactor.value(
                Some("service.name"),
                &any("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.dBjftJeZ4CVP")
            ),
            SanitizedValue::Redacted(RedactionKind::JsonWebToken)
        );
        assert_eq!(redactor.total(), 1);
    }

    #[test]
    fn a_kvlist_is_walked_recursively_and_sorted() {
        const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.dBjftJeZ4CVP";
        let mut redactor = Redactor::new();
        let value = AnyValue::KvList(vec![
            KeyValue {
                key: "z".to_string(),
                value: any("last"),
            },
            KeyValue {
                key: "authorization".to_string(),
                value: any("Bearer abc"),
            },
            // A key the table does not name, carrying a credential anyway. This is
            // the case the table alone cannot see.
            KeyValue {
                key: "note".to_string(),
                value: any(JWT),
            },
        ]);
        let sanitized = redactor.value(None, &value);
        let SanitizedValue::Map(entries) = sanitized else {
            panic!("expected a map");
        };
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key, "authorization", "entries are sorted by key");
        // The key table is consulted first, so the reason names the *key* rather
        // than whatever the value happened to look like.
        assert_eq!(
            entries[0].value,
            SanitizedValue::Redacted(RedactionKind::AuthorizationHeader)
        );
        assert_eq!(entries[1].key, "note");
        assert_eq!(
            entries[1].value,
            SanitizedValue::Redacted(RedactionKind::JsonWebToken)
        );
        assert_eq!(entries[2].value, SanitizedValue::Text("last".to_string()));
        assert_eq!(redactor.total(), 2);
    }

    #[test]
    fn opaque_bytes_are_removed_rather_than_kept() {
        let mut redactor = Redactor::new();
        assert_eq!(
            redactor.value(None, &AnyValue::Bytes("aGk=".to_string())),
            SanitizedValue::Redacted(RedactionKind::OpaqueBinaryValue)
        );
    }

    #[test]
    fn free_text_is_kept_and_labelled_rather_than_trusted_or_erased() {
        let mut redactor = Redactor::new();
        assert_eq!(
            redactor.value(
                Some("exception.message"),
                &any("card 4111 1111 1111 1111 rejected")
            ),
            SanitizedValue::FreeText("card 4111 1111 1111 1111 rejected".to_string()),
        );
        // No redaction was recorded: the value is still there. The disclosure is
        // what stops it being read as sanitized.
        assert_eq!(redactor.total(), 0);
    }

    #[test]
    fn a_span_name_is_stripped_of_its_query_string() {
        let mut redactor = Redactor::new();
        assert_eq!(
            redactor.span_name("GET /users/2?token=abc"),
            SanitizedValue::Text("GET /users/2".to_string())
        );
        assert_eq!(redactor.counts().get(&RedactionKind::QueryString), Some(&1));
    }

    #[test]
    fn a_double_is_canonicalised_so_its_spelling_cannot_move_a_digest() {
        let mut redactor = Redactor::new();
        assert_eq!(
            redactor.value(None, &AnyValue::Double("1.0".to_string())),
            SanitizedValue::Number("1.0".to_string())
        );
        // `serde_json` renders both spellings identically, which is asserted at
        // the reader rather than here.
    }
}
