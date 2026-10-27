use super::schema;
use serde_json::{Value, json};

pub(super) fn tools(unit: &Value, controller: &Value) -> Vec<Value> {
    let pos = json!({"type":"array","items":{"type":"integer"},"minItems":2,"maxItems":2,
        "description":"fixed [x,z] position in millimetres"});
    let name = json!({"type":"string","maxLength":200,"description":"one-line landmark name"});
    let note = json!({"type":"string","maxLength":8192,"description":"at most 8192 UTF-8 bytes"});
    vec![
        json!({
            "name":"map_list", "title":"List your map",
            "description":"Your fixed landmarks, by id, with names, positions, kinds, notes and updated stamps. You decide which places are worth remembering. Available after registration, including before taking control.",
            "inputSchema":schema(json!({"unit":unit,"controller":controller}), &["unit"]),
        }),
        json!({
            "name":"map_add", "title":"Add a landmark",
            "description":"Mark a place to return to: place is 'here', {thing: ID} for a currently sensed or remembered thing, or {pos: [x,z]} in millimetres. A landmark stays at that position even if its thing moves. To return, submit a Move order with landmark: its id.",
            "inputSchema":schema(json!({
                "unit":unit,"controller":controller,"name":name,
                "place":{"oneOf":[
                    {"type":"string","enum":["here"]},
                    {"type":"object","properties":{"thing":{"type":"integer"}},"required":["thing"],"additionalProperties":false},
                    {"type":"object","properties":{"pos":pos},"required":["pos"],"additionalProperties":false}
                ]},
                "kind":{"type":"string","description":"optional category, for example gate or well"},
                "note":note
            }), &["unit","name","place"]),
        }),
        json!({
            "name":"map_update", "title":"Change a landmark",
            "description":"Change name, kind, position or note while preserving id and thing. Pass the exact updated stamp returned by map_list/add/update. A changed landmark rejects as stale; list it again. Omitted fields stay unchanged; kind: null clears the category.",
            "inputSchema":schema(json!({
                "unit":unit,"controller":controller,"id":{"type":"integer"},
                "expected":{"type":"object","properties":{
                    "tick":{"type":"integer"},"date":{"type":"string","description":"exact updated.date, including any revision suffix"}
                },"required":["tick","date"]},
                "name":name,"kind":{"type":["string","null"]},"pos":pos,"note":note
            }), &["unit","id","expected"]),
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schemas_expose_all_placements_and_nullable_kind_changes() {
        let all = tools(&json!({"type":"integer"}), &json!({"type":"string"}));
        let add = &all[1]["inputSchema"];
        assert_eq!(
            add["properties"]["place"]["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(add["required"], json!(["unit", "name", "place"]));
        let update = &all[2]["inputSchema"]["properties"];
        for key in ["id", "expected", "name", "kind", "pos", "note"] {
            assert!(update.get(key).is_some(), "{key}");
        }
        assert_eq!(update["kind"]["type"], json!(["string", "null"]));
    }

    #[test]
    fn map_calls_keep_explicit_null_and_registered_schemas_hide_identity() {
        let (method, params) = crate::mcp::prepare_call(
            Some("pilot"),
            "map_update",
            json!({"unit":1,"id":2,"expected":{"tick":3,"date":"D#r4"},"kind":null}),
        )
        .unwrap();
        assert_eq!(method, "agent/map/update");
        assert_eq!(params["controller"], "pilot");
        assert!(params.get("kind").is_some_and(Value::is_null));
        let definitions = crate::mcp::registered_tools();
        for name in ["map_list", "map_add", "map_update"] {
            let tool = definitions
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == name)
                .unwrap();
            let props = &tool["inputSchema"]["properties"];
            assert!(props.get("unit").is_none());
            assert!(props.get("controller").is_none());
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
    }
}
