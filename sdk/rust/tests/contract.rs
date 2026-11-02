//! The generated route table must match the committed contract exactly.
//!
//! This test parses `docs/api/openapi-v1.json` directly, so a contract change
//! without `node scripts/generate.mjs` fails here even where Node is not
//! installed.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use tidepool_client::generated::routes::{CONTRACT_SHA256, CONTRACT_VERSION, ROUTES, SCOPES};
use tidepool_client::generated::types::problem_code;

const CONTRACT: &str = include_str!("../../../docs/api/openapi-v1.json");

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("contract is valid JSON")
}

fn operations(contract: &Value) -> BTreeMap<String, (String, String, Vec<String>, bool)> {
    let mut routes = BTreeMap::new();
    for (path, item) in contract["paths"].as_object().expect("paths object") {
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let id = operation["operationId"]
                .as_str()
                .expect("operationId")
                .to_owned();
            let path_params = operation["parameters"]
                .as_array()
                .map(|parameters| {
                    parameters
                        .iter()
                        .filter(|parameter| parameter["in"] == "path")
                        .map(|parameter| parameter["name"].as_str().expect("path name").to_owned())
                        .collect()
                })
                .unwrap_or_default();
            let has_body = operation.get("requestBody").is_some()
                || matches!(method, "post" | "put" | "patch");
            routes.insert(
                id,
                (method.to_uppercase(), path.clone(), path_params, has_body),
            );
        }
    }
    routes
}

#[test]
fn every_contract_operation_is_in_the_route_table() {
    let contract = contract();
    let expected = operations(&contract);
    let actual: BTreeSet<&str> = ROUTES.iter().map(|route| route.operation_id).collect();
    assert_eq!(actual.len(), ROUTES.len(), "operation ids are unique");
    assert_eq!(
        actual,
        expected.keys().map(String::as_str).collect(),
        "generated routes and contract operations must be the same set"
    );
    assert_eq!(ROUTES.len(), expected.len());
}

#[test]
fn every_route_matches_its_contract_entry() {
    for (id, (method, path, path_params, has_body)) in operations(&contract()) {
        let route = tidepool_client::generated::routes::route(&id)
            .unwrap_or_else(|| panic!("route {id} is missing"));
        assert_eq!(route.method.as_str(), method, "{id} method");
        assert_eq!(route.path, path, "{id} path");
        assert_eq!(
            route.path_params,
            path_params.as_slice(),
            "{id} path params"
        );
        assert_eq!(route.has_body, has_body, "{id} body");
        assert!(!route.summary.is_empty(), "{id} summary");
        assert!(!route.deprecated, "{id} is not marked deprecated");
    }
}

#[test]
fn access_metadata_is_present_and_known() {
    for route in ROUTES {
        assert!(
            [
                "public",
                "member",
                "member_or_api_key",
                "api_key",
                "host",
                "signature"
            ]
            .contains(&route.auth.as_str()),
            "{} has an unknown auth model",
            route.operation_id
        );
        assert!(
            ["system", "hub", "game"].contains(&route.surface.as_str()),
            "{} has an unknown surface",
            route.operation_id
        );
        if let Some(scope) = route.scope {
            assert!(
                SCOPES.contains(&scope),
                "{} declares an unknown scope",
                route.operation_id
            );
        }
        if let Some(role) = route.role {
            assert_eq!(
                role, "owner",
                "{} declares an undeclared role rule",
                route.operation_id
            );
        }
    }
}

#[test]
fn the_contract_version_is_documented() {
    let contract = contract();
    assert_eq!(contract["info"]["version"], CONTRACT_VERSION);
    assert_eq!(CONTRACT_VERSION, "1.0.0");
    assert_eq!(CONTRACT_SHA256.len(), 64);
    assert!(
        CONTRACT_SHA256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
}

#[test]
fn problem_codes_cover_the_contract() {
    let contract = contract();
    let codes = contract["components"]["schemas"]["Problem"]["properties"]["code"]["enum"]
        .as_array()
        .expect("problem code enum");
    for code in codes {
        let code = code.as_str().expect("code string");
        let constant = match code {
            "unauthorized" => problem_code::UNAUTHORIZED,
            "forbidden" => problem_code::FORBIDDEN,
            "missing_scope" => problem_code::MISSING_SCOPE,
            "invalid_request" => problem_code::INVALID_REQUEST,
            "validation_failed" => problem_code::VALIDATION_FAILED,
            "not_found" => problem_code::NOT_FOUND,
            "conflict" => problem_code::CONFLICT,
            "gone" => problem_code::GONE,
            "method_not_allowed" => problem_code::METHOD_NOT_ALLOWED,
            "rate_limited" => problem_code::RATE_LIMITED,
            "daemon_not_connected" => problem_code::DAEMON_NOT_CONNECTED,
            "configuration_invalid" => problem_code::CONFIGURATION_INVALID,
            "service_unavailable" => problem_code::SERVICE_UNAVAILABLE,
            "internal_error" => problem_code::INTERNAL_ERROR,
            other => panic!("problem code {other} has no generated constant"),
        };
        assert_eq!(constant, code);
    }
}

#[test]
fn every_route_is_describable_and_unknown_ids_fail() {
    for route in ROUTES {
        let described = tidepool_client::describe(route.operation_id).expect("described");
        assert_eq!(described.operation_id, route.operation_id);
    }
    assert!(tidepool_client::describe("get_the_thing").is_err());
}

#[test]
fn the_crate_names_the_api_version_it_targets() {
    assert_eq!(tidepool_client::API_VERSION, "v1");
    assert!(tidepool_client::CONTRACT_VERSION.starts_with("1."));
    let documentation = include_str!("../README.md");
    assert!(documentation.contains("API v1"));
    assert!(documentation.contains(CONTRACT_VERSION));
}
