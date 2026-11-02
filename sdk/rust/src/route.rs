//! Route metadata, the access model, and the [`Endpoint`] trait.

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// HTTP method a contract route uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `PATCH`
    Patch,
    /// `DELETE`
    Delete,
}

impl Method {
    /// The uppercase method name used on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    /// True for methods that carry a request body.
    pub fn allows_body(self) -> bool {
        !matches!(self, Self::Get | Self::Delete)
    }
}

/// The contract's authentication model for one route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Auth {
    /// No credential required.
    Public,
    /// An operator JWT.
    Member,
    /// An operator JWT or an API key with the route's scope.
    MemberOrApiKey,
    /// An API key with the route's scope.
    ApiKey,
    /// A host relationship credential.
    Host,
    /// A provider signature, verified by the pool.
    Signature,
    /// An auth model this SDK does not know yet.
    Unknown,
}

impl Auth {
    /// The contract string for this model.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Member => "member",
            Self::MemberOrApiKey => "member_or_api_key",
            Self::ApiKey => "api_key",
            Self::Host => "host",
            Self::Signature => "signature",
            Self::Unknown => "unknown",
        }
    }
}

/// Which of the pool's two surfaces a route belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Surface {
    /// Operational routes (`/healthz`, `/metrics`, the OpenAPI documents).
    System,
    /// The pool and hub operator surface.
    Hub,
    /// The Talo-compatible game surface.
    Game,
    /// A surface this SDK does not know yet.
    Unknown,
}

impl Surface {
    /// The contract string for this surface.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Hub => "hub",
            Self::Game => "game",
            Self::Unknown => "unknown",
        }
    }
}

/// One route's access model and wire shape, straight from the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteInfo {
    /// Stable operation id (`get_v1_hosts`).
    pub operation_id: &'static str,
    /// HTTP method.
    pub method: Method,
    /// Path template with `{param}` placeholders.
    pub path: &'static str,
    /// Path parameter names, in template order.
    pub path_params: &'static [&'static str],
    /// Query parameter names the SDK types (`docs/API_CONTRACT.md` §5.1).
    pub query_params: &'static [&'static str],
    /// True when the route accepts a JSON request body.
    pub has_body: bool,
    /// The success body type, for humans and generic dispatch.
    pub response: &'static str,
    /// Success statuses the contract declares.
    pub success: &'static [u16],
    /// Error statuses the contract declares, excluding 429.
    pub errors: &'static [u16],
    /// True when the route declares a 429 response.
    pub rate_limited: bool,
    /// Authentication model.
    pub auth: Auth,
    /// API-key scope that grants the route, when one narrows it.
    pub scope: Option<&'static str>,
    /// Member role the route requires, when one narrows it.
    pub role: Option<&'static str>,
    /// Pool surface.
    pub surface: Surface,
    /// True when the contract marks the route deprecated.
    pub deprecated: bool,
    /// Human-readable summary.
    pub summary: &'static str,
}

/// A principal asking whether it may call an operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Access<'a> {
    /// The member role, when the principal is a signed-in member.
    pub role: Option<&'a str>,
    /// API-key scopes, when the principal is a key.
    pub scopes: &'a [&'a str],
}

impl<'a> Access<'a> {
    /// Build an access description.
    pub fn new(role: Option<&'a str>, scopes: &'a [&'a str]) -> Self {
        Self { role, scopes }
    }

    /// A member with one role.
    pub fn member(role: &'a str) -> Self {
        Self {
            role: Some(role),
            scopes: &[],
        }
    }

    /// An API key holding exactly these scopes.
    pub fn key(scopes: &'a [&'a str]) -> Self {
        Self { role: None, scopes }
    }
}

impl RouteInfo {
    /// Whether this principal may call the route, before any request is sent.
    ///
    /// Mirrors `docs/API_CONTRACT.md` §4 and the TypeScript SDK's `allows()`:
    /// a public route is open; a scope satisfies a key; a member route needs a
    /// role and refuses writes from a viewer; an owner-only route needs the
    /// owner role (no owner-only route declares a scope).
    pub fn allows(&self, access: &Access<'_>) -> bool {
        if let Some(required) = self.role {
            if access.role != Some(required) {
                return false;
            }
        }
        if self.auth == Auth::Public {
            return true;
        }
        if let Some(scope) = self.scope {
            if access.scopes.contains(&scope) {
                return true;
            }
        }
        if self.auth == Auth::ApiKey {
            return self
                .scope
                .is_some_and(|scope| access.scopes.contains(&scope));
        }
        let Some(role) = access.role else {
            return false;
        };
        if self.method != Method::Get && role == "viewer" {
            return false;
        }
        true
    }
}

/// One typed contract request.
///
/// Generated types live in [`crate::generated::endpoints`]; a caller can also
/// implement this trait for a scripted endpoint in tests. The client uses
/// [`Endpoint::ROUTE`] for metadata, [`Endpoint::path`] for the URL,
/// [`Endpoint::query`] and [`Endpoint::body`] for the wire, and
/// [`Endpoint::decode_response`] to turn the success body into
/// [`Endpoint::Response`].
pub trait Endpoint {
    /// The typed success body.
    type Response;

    /// The contract route this request calls.
    const ROUTE: &'static RouteInfo;

    /// The encoded path, with every `{param}` substituted.
    fn path(&self) -> String;

    /// Query parameters, already rendered as strings.
    fn query(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }

    /// The JSON request body, when the route takes one.
    fn body(&self) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    /// Parse the success body into [`Endpoint::Response`].
    ///
    /// An empty body decodes as `null`, which is what the contract's `204`
    /// routes produce. The metrics route overrides this because its body is
    /// Prometheus text, not JSON.
    fn decode_response(body: &[u8]) -> Result<Self::Response>
    where
        Self::Response: DeserializeOwned,
    {
        let payload: &[u8] = if body.is_empty() { b"null" } else { body };
        serde_json::from_slice(payload).map_err(|source| Error::Decode {
            operation_id: Self::ROUTE.operation_id,
            source,
        })
    }
}

/// Substitute percent-encoded path parameters into a contract path template.
///
/// Every parameter must appear in the template; a repeated parameter is
/// replaced everywhere it occurs.
pub fn path_with(template: &str, params: &[(&str, &str)]) -> String {
    let mut path = template.to_owned();
    for (name, value) in params {
        let placeholder = format!("{{{name}}}");
        path = path.replace(&placeholder, &encode_component(value));
    }
    path
}

/// Percent-encode one URL component, leaving RFC 3986 unreserved characters.
///
/// Used for path segments and query values, so a slug or id containing `/`,
/// space or `?` cannot change the route the request hits.
pub fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                encoded.push('%');
                encoded.push(char::from(HEX[(byte >> 4) as usize]));
                encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
            }
        }
    }
    encoded
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_and_auth_strings_match_the_contract() {
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Patch.as_str(), "PATCH");
        assert!(Method::Post.allows_body());
        assert!(!Method::Get.allows_body());
        assert_eq!(Auth::MemberOrApiKey.as_str(), "member_or_api_key");
        assert_eq!(Surface::Hub.as_str(), "hub");
    }

    #[test]
    fn path_parameters_are_encoded() {
        assert_eq!(
            path_with("/v1/hosts/{id}", &[("id", "a b/c?d")]),
            "/v1/hosts/a%20b%2Fc%3Fd"
        );
        assert_eq!(
            path_with(
                "/v1/projects/{id}/x/{other}",
                &[("id", "a"), ("other", "b")]
            ),
            "/v1/projects/a/x/b"
        );
    }

    #[test]
    fn encoding_leaves_unreserved_characters_alone() {
        assert_eq!(encode_component("Az09-._~"), "Az09-._~");
        assert_eq!(encode_component("é"), "%C3%A9");
    }

    fn route(
        method: Method,
        auth: Auth,
        scope: Option<&'static str>,
        role: Option<&'static str>,
    ) -> RouteInfo {
        RouteInfo {
            operation_id: "test",
            method,
            path: "/test",
            path_params: &[],
            query_params: &[],
            has_body: method.allows_body(),
            response: "()",
            success: &[200],
            errors: &[401],
            rate_limited: true,
            auth,
            scope,
            role,
            surface: Surface::Hub,
            deprecated: false,
            summary: "test route",
        }
    }

    #[test]
    fn public_routes_are_open() {
        let info = route(Method::Get, Auth::Public, None, None);
        assert!(info.allows(&Access::default()));
    }

    #[test]
    fn scope_satisfies_a_member_or_api_key_route() {
        let info = route(
            Method::Post,
            Auth::MemberOrApiKey,
            Some("runs:dispatch"),
            None,
        );
        assert!(info.allows(&Access::key(&["runs:dispatch"])));
        assert!(!info.allows(&Access::key(&["projects:read"])));
        assert!(info.allows(&Access::member("member")));
    }

    #[test]
    fn api_key_routes_need_the_scope() {
        let info = route(Method::Get, Auth::ApiKey, Some("projects:read"), None);
        assert!(info.allows(&Access::key(&["projects:read"])));
        assert!(!info.allows(&Access::member("owner")));
    }

    #[test]
    fn viewers_cannot_write_but_can_read() {
        let read = route(Method::Get, Auth::Member, None, None);
        let write = route(Method::Post, Auth::Member, None, None);
        assert!(read.allows(&Access::member("viewer")));
        assert!(!write.allows(&Access::member("viewer")));
        assert!(write.allows(&Access::member("member")));
    }

    #[test]
    fn owner_only_routes_need_the_owner_role() {
        let info = route(Method::Delete, Auth::Member, None, Some("owner"));
        assert!(!info.allows(&Access::member("admin")));
        assert!(info.allows(&Access::member("owner")));
        assert!(!info.allows(&Access::key(&["configuration:install"])));
    }

    #[test]
    fn an_unauthenticated_principal_cannot_use_member_routes() {
        let info = route(Method::Get, Auth::Member, None, None);
        assert!(!info.allows(&Access::default()));
    }
}
