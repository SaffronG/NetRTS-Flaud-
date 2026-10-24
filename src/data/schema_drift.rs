//! Lists the differences between the structs in `models.rs` and the server's
//! OpenAPI spec, so a server change shows up in `cargo test` instead of as a
//! decode error mid-match.
//!
//! Our side comes from `schemars`, which reads the serde attributes (`rename`,
//! `rename_all`, `default`, `Option`), so it describes the JSON the structs
//! actually accept. The spec side is `fixtures/openapi.json`.
//!
//! ```shell
//! cargo test schema_drift -- --nocapture                       # against the saved spec
//! cargo test schema_drift -- --ignored --nocapture             # against the live server
//! UPDATE_SPEC=1 cargo test schema_drift -- --ignored --nocapture  # ...and save it as the fixture
//! ```

use crate::data::models::*;
use schemars::{JsonSchema, schema_for};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const SPEC_URL: &str = "https://netrts.snowse.io/openapi/v1.json";
const FIXTURE: &str = include_str!("fixtures/openapi.json");
const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/data/fixtures/openapi.json"
);
/// An enum variant named this is taken to be a `#[serde(other)]` catch-all, so
/// spec values we don't know decode to it instead of failing.
const CATCH_ALL: &str = "Unknown";

/// Most severe first, so a sorted report leads with what breaks decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Severity {
    /// Decoding (or the server reading our request) will fail
    Breaking,
    /// Probably a mistake on our side, such as a type name the spec doesn't have
    Warn,
    /// We model something the spec no longer has
    Stale,
    /// The spec has something we ignore; serde skips it
    Info,
}

#[derive(Debug, PartialEq)]
struct Drift {
    severity: Severity,
    type_name: String,
    field: Option<String>,
    detail: String,
}

impl Drift {
    fn new(severity: Severity, type_name: &str, field: Option<&str>, detail: String) -> Self {
        Self {
            severity,
            type_name: type_name.to_string(),
            field: field.map(str::to_string),
            detail,
        }
    }
}

impl fmt::Display for Drift {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let tag = format!("{:?}", self.severity).to_lowercase();
        match &self.field {
            Some(field) => write!(f, "[{tag}] {}.{field}: {}", self.type_name, self.detail),
            None => write!(f, "[{tag}] {}: {}", self.type_name, self.detail),
        }
    }
}

/// Schema name → JSON Schema
type Schemas = BTreeMap<String, Value>;

/// Adds `T` and every type nested in it (its `$defs`) under their Rust names.
fn add<T: JsonSchema>(out: &mut Schemas) {
    let mut root = serde_json::to_value(schema_for!(T)).expect("schema is JSON");
    if let Some(Value::Object(defs)) = root.as_object_mut().and_then(|o| o.remove("$defs")) {
        out.extend(defs);
    }
    out.insert(T::schema_name().into_owned(), root);
}

/// Our models. Listing the top-level request and response types is enough;
/// the types inside them come along through `$defs`.
fn ours() -> Schemas {
    let mut s = Schemas::new();
    add::<ApiErrorDto>(&mut s);
    add::<RegisterPlayerRequest>(&mut s);
    add::<RegisterPlayerResponse>(&mut s);
    add::<HouseBotDto>(&mut s);
    add::<MatchId>(&mut s);
    add::<CreateMatchRequest>(&mut s);
    add::<MatchSummaryDto>(&mut s);
    add::<MatchResultDto>(&mut s);
    add::<RulesDto>(&mut s);
    add::<MapDto>(&mut s);
    add::<GameStateDto>(&mut s);
    add::<SubmitCommandsRequest>(&mut s);
    add::<SubmitCommandsResponse>(&mut s);
    s
}

fn spec_schemas(spec: &Value) -> Schemas {
    spec.pointer("/components/schemas")
        .and_then(Value::as_object)
        .map(|o| o.clone().into_iter().collect())
        .unwrap_or_default()
}

/// A rough type name that both sides agree on: `integer`, `string`, `array<T>`,
/// or the name of a referenced schema. Nullability is dropped, because
/// `Option` and `#[serde(default)]` already cover a missing value on our side.
fn kind(v: &Value) -> String {
    if let Some(r) = v.get("$ref").and_then(Value::as_str) {
        return r.rsplit('/').next().unwrap_or(r).to_string();
    }
    // `Option<T>` (schemars) and nullable refs (.NET) wrap T with a null arm
    for key in ["anyOf", "oneOf"] {
        if let Some(arms) = v.get(key).and_then(Value::as_array) {
            let real: Vec<&Value> = arms
                .iter()
                .filter(|a| a.get("type").and_then(Value::as_str) != Some("null"))
                .collect();
            return match real[..] {
                [only] => kind(only),
                _ => key.to_string(),
            };
        }
    }
    let types: Vec<&str> = match v.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    // .NET writes integers as ["integer", "string"] with a digits-only pattern
    let t = ["integer", "number"]
        .into_iter()
        .find(|t| types.contains(t))
        .or_else(|| types.into_iter().find(|t| *t != "null"))
        .unwrap_or("any");
    match t {
        "array" => format!("array<{}>", v.get("items").map_or("any".to_string(), kind)),
        _ => t.to_string(),
    }
}

/// The values of a string enum, from `enum: [...]` or `oneOf: [{const}, ...]`.
/// `None` when the schema isn't an enum.
fn enum_values(v: &Value) -> Option<BTreeSet<String>> {
    if let Some(vals) = v.get("enum").and_then(Value::as_array) {
        return Some(vals.iter().filter_map(Value::as_str).map(str::to_string).collect());
    }
    let arms = v.get("oneOf")?.as_array()?;
    let consts: Option<BTreeSet<String>> = arms
        .iter()
        .map(|a| a.get("const").and_then(Value::as_str).map(str::to_string))
        .collect();
    consts
}

fn properties(v: &Value) -> serde_json::Map<String, Value> {
    v.get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Differences between our schema for `name` and the spec's schema of the same name.
fn diff(name: &str, ours: &Value, theirs: &Value) -> Vec<Drift> {
    use Severity::*;
    let mut out = Vec::new();

    if let Some(our_vals) = enum_values(ours) {
        let their_vals = enum_values(theirs).unwrap_or_default();
        let catch_all = our_vals.contains(CATCH_ALL);
        for v in their_vals.difference(&our_vals) {
            out.push(if catch_all {
                Drift::new(Info, name, None, format!("spec value `{v}` decodes as `{CATCH_ALL}`"))
            } else {
                Drift::new(Breaking, name, None, format!("spec value `{v}` is missing"))
            });
        }
        for v in our_vals.difference(&their_vals).filter(|v| *v != CATCH_ALL) {
            out.push(Drift::new(Stale, name, None, format!("value `{v}` is not in the spec")));
        }
        return out;
    }

    let (ours_p, theirs_p) = (properties(ours), properties(theirs));
    let required: BTreeSet<&str> = ours
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    for (field, o) in &ours_p {
        match theirs_p.get(field) {
            None if required.contains(field.as_str()) => out.push(Drift::new(
                Breaking,
                name,
                Some(field),
                "required here but not in the spec".to_string(),
            )),
            None => out.push(Drift::new(
                Stale,
                name,
                Some(field),
                "optional here and not in the spec".to_string(),
            )),
            Some(t) => {
                let (ok, tk) = (kind(o), kind(t));
                if ok != tk {
                    out.push(Drift::new(
                        Breaking,
                        name,
                        Some(field),
                        format!("`{ok}` here but `{tk}` in the spec"),
                    ));
                }
            }
        }
    }
    for (field, t) in theirs_p.iter().filter(|(f, _)| !ours_p.contains_key(*f)) {
        out.push(Drift::new(
            Info,
            name,
            Some(field),
            format!("spec field (`{}`) is not modelled", kind(t)),
        ));
    }
    out
}

/// Every difference between our models and the spec, most severe first.
fn report(ours: &Schemas, spec: &Schemas) -> Vec<Drift> {
    let mut out: Vec<Drift> = ours
        .iter()
        .flat_map(|(name, o)| match spec.get(name) {
            Some(t) => diff(name, o, t),
            None => vec![Drift::new(
                Severity::Warn,
                name,
                None,
                "no schema with this name in the spec".to_string(),
            )],
        })
        .collect();
    out.sort_by(|a, b| {
        (a.severity, &a.type_name, &a.field).cmp(&(b.severity, &b.type_name, &b.field))
    });
    out
}

/// Prints the report and fails if anything would break decoding.
fn check(spec: &Value) {
    let drift = report(&ours(), &spec_schemas(spec));
    for d in &drift {
        println!("{d}");
    }
    let breaking: Vec<String> = drift
        .iter()
        .filter(|d| d.severity == Severity::Breaking)
        .map(Drift::to_string)
        .collect();
    assert!(
        breaking.is_empty(),
        "models no longer match the spec:\n{}",
        breaking.join("\n")
    );
}

#[test]
fn models_match_the_saved_spec() {
    check(&serde_json::from_str(FIXTURE).expect("fixture is JSON"));
}

#[tokio::test]
#[ignore = "needs the server; run with --ignored"]
async fn models_match_the_live_spec() {
    let body = reqwest::get(SPEC_URL)
        .await
        .and_then(|r| r.error_for_status())
        .expect("spec request failed")
        .text()
        .await
        .expect("spec body");
    let spec: Value = serde_json::from_str(&body).expect("spec is JSON");
    if std::env::var_os("UPDATE_SPEC").is_some() {
        // Pretty-printed so a spec change reads as a small git diff
        let pretty = serde_json::to_string_pretty(&spec).unwrap() + "\n";
        std::fs::write(FIXTURE_PATH, pretty).expect("write fixture");
        println!("saved {FIXTURE_PATH}");
    }
    check(&spec);
}

#[cfg(test)]
mod diff_tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    const DOTNET_INT: &str = r#"{"type": ["integer", "string"], "pattern": "^-?(?:0|[1-9]\\d*)$", "format": "int32"}"#;

    fn dotnet_int() -> Value {
        serde_json::from_str(DOTNET_INT).unwrap()
    }

    fn severities(d: &[Drift]) -> Vec<Severity> {
        d.iter().map(|d| d.severity).collect()
    }

    #[test]
    fn identical_schemas_have_no_drift() {
        let s = json!({"properties": {"x": {"type": "integer"}}, "required": ["x"]});
        assert_eq!(diff("P", &s, &s), vec![]);
    }

    #[test]
    fn spec_enum_value_we_lack_is_breaking() {
        // The SiegeWorkshop case that stopped the bot joining games
        let ours = json!({"type": "string", "enum": ["Barracks"]});
        let theirs = json!({"enum": ["Barracks", "SiegeWorkshop"]});
        let d = diff("BuildingType", &ours, &theirs);
        assert_eq!(severities(&d), [Severity::Breaking]);
        assert!(d[0].detail.contains("SiegeWorkshop"), "{}", d[0]);
    }

    #[test]
    fn enum_value_only_we_have_is_stale() {
        let ours = json!({"enum": ["Barracks", "Bunker"]});
        let theirs = json!({"enum": ["Barracks"]});
        assert_eq!(severities(&diff("B", &ours, &theirs)), [Severity::Stale]);
    }

    #[test]
    fn spec_enum_ignores_null_entries() {
        // .NET lists `null` among the values of a nullable enum
        let ours = json!({"enum": ["A"]});
        let theirs = json!({"enum": ["A", null]});
        assert_eq!(diff("E", &ours, &theirs), vec![]);
    }

    #[test]
    fn catch_all_variant_turns_missing_values_into_info() {
        // Uses real schemars output, to check it keeps the `serde(other)` variant
        #[derive(Deserialize, JsonSchema)]
        #[allow(dead_code)]
        enum Kind {
            A,
            #[serde(other)]
            Unknown,
        }
        let ours = serde_json::to_value(schema_for!(Kind)).unwrap();
        let theirs = json!({"enum": ["A", "B"]});
        assert_eq!(severities(&diff("Kind", &ours, &theirs)), [Severity::Info]);
    }

    #[test]
    fn spec_field_we_skip_is_info() {
        let ours = json!({"properties": {}});
        let theirs = json!({"properties": {"minRange": dotnet_int()}});
        let d = diff("UnitStatsDto", &ours, &theirs);
        assert_eq!(severities(&d), [Severity::Info]);
        assert_eq!(d[0].field.as_deref(), Some("minRange"));
    }

    #[test]
    fn our_required_field_missing_from_spec_is_breaking_but_optional_is_stale() {
        let ours = json!({
            "properties": {"gone": {"type": "integer"}, "maybe": {"type": ["integer", "null"]}},
            "required": ["gone"],
        });
        let theirs = json!({"properties": {}});
        let d = diff("P", &ours, &theirs);
        let by_field: BTreeMap<_, _> =
            d.iter().map(|d| (d.field.as_deref().unwrap(), d.severity)).collect();
        assert_eq!(by_field["gone"], Severity::Breaking);
        assert_eq!(by_field["maybe"], Severity::Stale);
    }

    #[test]
    fn type_mismatch_is_breaking() {
        let ours = json!({"properties": {"id": {"type": "integer"}}});
        let theirs = json!({"properties": {"id": {"type": "string", "format": "uuid"}}});
        let d = diff("P", &ours, &theirs);
        assert_eq!(severities(&d), [Severity::Breaking]);
        assert!(d[0].detail.contains("integer") && d[0].detail.contains("string"), "{}", d[0]);
    }

    #[test]
    fn dotnet_and_schemars_encodings_agree() {
        // Same Rust types, encoded the way each side writes them
        let pairs = [
            (json!({"type": "integer", "format": "int32"}), dotnet_int()),
            (json!({"type": ["integer", "null"]}), json!({"type": ["null", "integer", "string"]})),
            (
                json!({"anyOf": [{"$ref": "#/$defs/PositionDto"}, {"type": "null"}]}),
                json!({"oneOf": [{"type": "null"}, {"$ref": "#/components/schemas/PositionDto"}]}),
            ),
            (
                json!({"type": "array", "items": {"$ref": "#/$defs/UnitType"}}),
                json!({"type": "array", "items": {"$ref": "#/components/schemas/UnitType"}}),
            ),
            (json!({"type": "array", "items": {"type": "integer"}}), json!({"type": "array", "items": dotnet_int()})),
        ];
        for (ours, theirs) in &pairs {
            assert_eq!(kind(ours), kind(theirs), "{ours} vs {theirs}");
        }
    }

    #[test]
    fn our_type_missing_from_spec_is_a_warning() {
        let ours = Schemas::from([("Typo".to_string(), json!({"properties": {}}))]);
        let d = report(&ours, &Schemas::new());
        assert_eq!(severities(&d), [Severity::Warn]);
    }

    #[test]
    fn report_puts_breaking_first() {
        let ours = Schemas::from([
            ("A".to_string(), json!({"properties": {}})),
            ("Z".to_string(), json!({"enum": ["X"]})),
        ]);
        let spec = Schemas::from([
            ("A".to_string(), json!({"properties": {"new": {"type": "boolean"}}})),
            ("Z".to_string(), json!({"enum": ["X", "Y"]})),
        ]);
        assert_eq!(severities(&report(&ours, &spec)), [Severity::Breaking, Severity::Info]);
    }

    #[test]
    fn ours_collects_nested_types_under_their_rust_names() {
        // Names must match the spec's, or every type would only be a warning
        let s = ours();
        for name in ["RulesDto", "UnitStatsDto", "BuildingType", "GameStateDto", "PositionDto", "CommandRequest"] {
            assert!(s.contains_key(name), "{name} missing from {:?}", s.keys());
        }
    }
}
