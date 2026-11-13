//! Living economy recipe DAG and real registered-client production contracts.
use island_content::village::catalog;
use npc_framework::crafting::RecipeRegistry;
use std::collections::{BTreeMap, BTreeSet};

fn roots(
    kind: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
) -> BTreeSet<String> {
    assert!(
        visiting.insert(kind.into()),
        "production graph contains a cycle at {kind}"
    );
    let mut result = BTreeSet::new();
    match graph.get(kind) {
        Some(inputs) => {
            for input in inputs {
                result.extend(roots(input, graph, visiting));
            }
        }
        None => {
            result.insert(kind.into());
        }
    }
    visiting.remove(kind);
    result
}

#[test]
fn finished_goods_join_real_hunting_farming_water_and_forestry_inputs_without_cycles() {
    let app = island_content::new_sim_app(7, "living_world");
    let recipes = &app.world().resource::<RecipeRegistry>().recipes;
    let mut graph = BTreeMap::new();
    for verb in catalog::RECIPE_VERBS
        .iter()
        .chain(catalog::LIVING_RECIPE_VERBS)
    {
        let recipe = &recipes[verb.strip_prefix("craft:").unwrap()];
        for output in &recipe.outputs {
            graph.insert(
                output.kind.clone(),
                recipe
                    .inputs
                    .iter()
                    .map(|input| input.key.clone())
                    .collect(),
            );
        }
    }
    for output in graph.keys() {
        roots(output, &graph, &mut BTreeSet::new());
    }
    assert_eq!(
        roots(catalog::WARM_CLOTHES, &graph, &mut BTreeSet::new()),
        BTreeSet::from([
            catalog::HIDE.into(),
            catalog::FIBER.into(),
            catalog::WATER.into(),
            "log".into()
        ])
    );
    assert_eq!(
        roots(catalog::COOKED_MEAL, &graph, &mut BTreeSet::new()),
        BTreeSet::from([
            catalog::RAW_MEAT.into(),
            catalog::WATER.into(),
            "log".into()
        ])
    );
    assert_eq!(
        roots(catalog::SPEAR, &graph, &mut BTreeSet::new()),
        BTreeSet::from(["ore".into(), "log".into()])
    );
}
