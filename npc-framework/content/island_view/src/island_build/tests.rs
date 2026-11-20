use super::model::{self, Card, CardBody, Ghost, Group, PaletteItem, Shelf, SiteRow, Stage, Tint};
use super::*;
use island_content::IslandIds;
use island_content::village::frontier::building_spec::{BuildingDef, BuildingKind};
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::{record_effect, set_journal};
use npc_framework::prelude::run_ticks;
use npc_framework::view_helpers::Selected;
use serde_json::json;

fn defs() -> Vec<BuildingDef> {
    serde_json::from_value(json!([
        {"id": "cottage", "display": "Cottage",
         "cost": [{"good": "plank", "count": 8}, {"good": "log", "count": 4}],
         "work_ticks": 600, "footprint": {"width": 4, "depth": 5},
         "kind": {"type": "home", "beds": 2}},
        {"id": "smithy", "display": "Smithy",
         "cost": [{"good": "plank", "count": 10}, {"good": "log", "count": 6}],
         "work_ticks": 800, "footprint": {"width": 5, "depth": 5},
         "kind": {"type": "workplace", "profession": "smith", "station": "village_smelt",
                  "capacity": 2}},
        {"id": "well", "display": "Well", "work_ticks": 100,
         "footprint": {"width": 3, "depth": 3},
         "kind": {"type": "workplace", "profession": "water_keeper",
                  "station": "village_water", "capacity": 1}, "solid": false},
        {"id": "stockpile", "display": "Stockpile", "cost": [{"good": "log", "count": 4}],
         "work_ticks": 200, "footprint": {"width": 4, "depth": 4},
         "kind": {"type": "storage", "capacity": 120}, "solid": false},
        {"id": "market_stall", "display": "Market stall",
         "cost": [{"good": "plank", "count": 6}, {"good": "log", "count": 2}],
         "work_ticks": 300, "footprint": {"width": 3, "depth": 2},
         "kind": {"type": "stall", "capacity": 60}, "solid": false, "civic": false}
    ]))
    .expect("building definitions")
}

fn named(kind: &str) -> String {
    match kind {
        "plank" => "planks".into(),
        "log" => "logs".into(),
        other => other.into(),
    }
}

fn items() -> Vec<PaletteItem> {
    model::palette(&defs(), &named)
}

#[test]
fn the_palette_groups_every_building_with_its_cost_and_purpose() {
    let items = items();
    assert_eq!(items.len(), 5, "every building of the catalogue is offered");
    let groups = model::grouped(&items);
    let shape: Vec<(Group, Vec<&str>)> = groups
        .iter()
        .map(|(group, items)| (*group, items.iter().map(|item| item.id.as_str()).collect()))
        .collect();
    assert_eq!(
        shape,
        [
            (Group::Homes, vec!["cottage"]),
            (Group::Workplaces, vec!["smithy", "well"]),
            (Group::Storage, vec!["stockpile"]),
            (Group::Market, vec!["market_stall"]),
        ],
        "grouped by kind, in authored order inside a group"
    );
    assert_eq!(
        Group::ALL.map(Group::label),
        ["Homes", "Workplaces", "Storage", "Market", "Farms"]
    );
    assert_eq!(Group::Workplaces.dock_label(), "Work");
    // The cost is kept by catalogue key too, for icons and the village stock.
    assert!(items.iter().all(|item| !item.id.is_empty()));
    assert_eq!(
        items[0]
            .materials
            .iter()
            .map(|(_, count)| *count)
            .sum::<u32>()
            > 0,
        items[0].cost != "no materials"
    );
    let cottage = &items[0];
    assert_eq!(cottage.cost, "8 planks, 4 logs");
    assert_eq!(cottage.purpose, "A home with 2 beds");
    assert_eq!(cottage.size, (4, 5));
    // The card shows the cost as icon and number pairs, by catalogue key.
    assert_eq!(cottage.materials.len(), 2);
    assert_eq!(
        cottage
            .materials
            .iter()
            .map(|(_, count)| *count)
            .collect::<Vec<_>>(),
        [8, 4]
    );
    assert_eq!(cottage.icon, crate::game_ui::icon::Icon::Cottage);
    assert_eq!(
        model::capitalised_cost(&cottage.cost),
        "Costs 8 planks, 4 logs."
    );
    assert_eq!(items[1].purpose, "Work for 2 blacksmiths");
    assert_eq!(items[2].purpose, "Work for 1 well keeper");
    assert_eq!(items[2].cost, "no materials", "a free building says so");
    assert_eq!(items[3].purpose, "Stores 120 units; residents buy from it");
    assert_eq!(items[4].purpose, "Sells 60 units to anyone, visitors too");
    // The shown group falls back to one that has buildings.
    assert_eq!(
        model::shown_group(&items, Group::Storage),
        Some(Group::Storage)
    );
    let homes_only = &items[..1];
    assert_eq!(
        model::shown_group(homes_only, Group::Market),
        Some(Group::Homes)
    );
    assert_eq!(model::shown_group(&[], Group::Homes), None);
}

fn plot() -> civic::Plot {
    civic::plot_for(&defs()[0], 10, -4, 0)
}

#[test]
fn the_ghost_is_green_or_red_from_the_placement_check_and_says_why() {
    let valid = Ghost::new("cottage", (10, -4), 0, Some(plot()), Ok(()));
    assert_eq!(valid.tint(), Tint::Valid);
    // The tag beside the cursor: a verdict in words, the cost and footprint.
    let cottage = &items()[0];
    let tag = model::tag(cottage, valid.blocked.as_deref());
    assert!(tag.valid);
    assert_eq!(tag.verdict, "You can build here");
    assert_eq!(tag.reason, None);
    assert_eq!(tag.facts, "Cottage · 8 planks · 4 logs · 4 × 5 m");
    let (body, entrance) = valid.cells();
    assert_eq!(body.len(), 20, "a 4 x 5 body");
    assert_eq!(entrance, (10, -4));
    assert!(
        !body.contains(&entrance),
        "the entrance is in front of the body"
    );
    assert!(body.contains(&(9, -3)) && body.contains(&(12, 1)));

    let blocked = Ghost::new(
        "cottage",
        (10, -4),
        1,
        Some(plot()),
        Err("the ground is too steep here".into()),
    );
    assert_eq!(blocked.tint(), Tint::Blocked);
    let tag = model::tag(cottage, blocked.blocked.as_deref());
    assert!(!tag.valid);
    assert_eq!(tag.verdict, "Cannot build here");
    assert_eq!(
        tag.reason.as_deref(),
        Some("The ground is too steep here."),
        "the simulation's reason as a sentence of its own"
    );
    // An unknown building has no plot and can never be placed.
    let unknown = Ghost::new("castle", (0, 0), 0, None, Ok(()));
    assert_eq!(unknown.tint(), Tint::Blocked);
    assert!(unknown.cells().0.is_empty());

    // R walks the four turns and comes back.
    let turns: Vec<u8> = (0..5)
        .scan(0_u8, |rotation, _| {
            let now = *rotation;
            *rotation = model::rotated(now);
            Some(now)
        })
        .collect();
    assert_eq!(turns, [0, 1, 2, 3, 0]);
    assert_eq!(Ghost::new("cottage", (0, 0), 7, None, Ok(())).rotation, 3);
}

fn site(delivered: (u32, u32), work_done: u32) -> SiteRow {
    SiteRow {
        id: 812,
        label: "Harbour cottage 3".into(),
        materials: vec![
            ("planks".into(), delivered.0, 8),
            ("logs".into(), delivered.1, 4),
        ],
        work_done,
        work_ticks: 600,
        civic: true,
        reason: "2 resident(s) have had no bed for 3 reviews".into(),
        builder: None,
        waiting_days: 0,
        unfunded: false,
    }
}

#[test]
fn a_project_row_shows_materials_stage_and_progress() {
    let empty = site((0, 0), 0);
    assert_eq!(empty.stage(), Stage::Materials);
    assert_eq!(empty.progress_permille(), 0);
    assert_eq!(
        empty.line(),
        "Harbour cottage 3 · 0%\nplanks 0/8 · logs 0/4 · waiting for materials"
    );
    let half = site((3, 3), 0);
    assert_eq!(half.progress_permille(), 250, "materials are half the job");
    assert_eq!(half.materials_line(), "planks 3/8 · logs 3/4");
    let stalled = SiteRow {
        waiting_days: 2,
        ..site((3, 3), 0)
    };
    assert_eq!(stalled.status(), "waiting for materials for 2 days");
    // A fund with nothing to spend is why nobody brings anything.
    let unfunded = SiteRow {
        unfunded: true,
        ..site((0, 0), 0)
    };
    assert_eq!(
        unfunded.status(),
        "waiting for materials; the fund cannot pay for them yet"
    );
    let long_unfunded = SiteRow {
        unfunded: true,
        waiting_days: 3,
        ..site((0, 0), 0)
    };
    assert_eq!(
        long_unfunded.status(),
        "waiting for materials for 3 days; the fund cannot pay for them yet"
    );
    let ready = site((8, 4), 0);
    assert_eq!(ready.stage(), Stage::Ready);
    assert_eq!(ready.progress_permille(), 500);
    assert_eq!(ready.status(), "materials in · waiting for a builder");
    let walking = SiteRow {
        builder: Some("Ada".into()),
        ..site((8, 4), 0)
    };
    assert_eq!(walking.status(), "materials in · Ada is on the way");
    let building = SiteRow {
        builder: Some("Ada".into()),
        ..site((8, 4), 200)
    };
    assert_eq!(building.stage(), Stage::Building);
    assert_eq!(building.progress_permille(), 666);
    assert_eq!(building.status(), "work 200/600 · Ada is building");
    assert_eq!(site((9, 4), 600).progress_permille(), 1000);
    // A building that costs nothing is half done the moment it is placed.
    let free = SiteRow {
        materials: Vec::new(),
        ..site((0, 0), 0)
    };
    assert_eq!(free.stage(), Stage::Ready);
    assert_eq!(free.materials_line(), "no materials needed");
    assert_eq!(free.progress_permille(), 500);

    assert_eq!(
        model::fund_line(670, 420, 96, 2),
        "Civic fund: 670 coins. 420 coins can be spent on materials; 250 stay in reserve for grants.\n96 held back from the dividend for 2 open sites."
    );
    assert_eq!(
        model::fund_line(226, 0, 75, 1),
        "Civic fund: 226 coins, all of it the reserve for grants. Nothing can be spent on materials yet.\n75 held back from the dividend for 1 open site."
    );
    assert_eq!(
        model::fund_line(0, 0, 0, 0),
        "Civic fund: empty. It fills on levy day and from what the village's stores sell."
    );
    // Waiting is counted in whole days since the last delivery.
    assert_eq!(model::waiting_days(30_000, 10_000, 24_000, false), 0);
    assert_eq!(model::waiting_days(34_000, 10_000, 24_000, false), 1);
    assert_eq!(model::waiting_days(90_000, 10_000, 24_000, false), 3);
    assert_eq!(model::waiting_days(90_000, 10_000, 24_000, true), 0);
    assert_eq!(model::waiting_days(5, 10, 24_000, false), 0);
}

#[test]
fn planner_decisions_read_as_sentences_with_their_reasons() {
    let commissioned = json!({"settlement": 3, "decision": "commissioned",
        "building": "cottage", "site": 812,
        "reason": "2 resident(s) have had no bed for 3 reviews",
        "estimated_cost": 64, "fund": 410});
    assert_eq!(
        model::decision_text("Harbour", Some("Cottage"), &commissioned),
        "Harbour ordered a cottage: 2 resident(s) have had no bed for 3 reviews (about 64 coins; fund 410)"
    );
    let deferred = json!({"settlement": 3,
        "decision": "deferred: the civic fund cannot pay for the materials yet",
        "building": "inn", "reason": "9 unit(s) of innkeeper goods are wanted and every seat is taken",
        "estimated_cost": 120, "fund": 260});
    assert_eq!(
        model::decision_text("Alderwood", Some("Inn"), &deferred),
        "Alderwood deferred an inn: the civic fund cannot pay for the materials yet (about 120 coins; fund 260). It wants an inn: 9 unit(s) of innkeeper goods are wanted and every seat is taken."
    );
    let completed = json!({"decision": "completed", "building": "stockpile", "paid": 18});
    assert_eq!(
        model::decision_text("Stone Ridge", Some("Stockpile"), &completed),
        "Stone Ridge finished a stockpile; the fund paid 18 coins"
    );
    let cancelled = json!({"decision": "cancelled", "reason": "not finished in time"});
    assert_eq!(
        model::decision_text("Harbour", None, &cancelled),
        "Harbour gave up a building: not finished in time"
    );
    let log: Vec<model::Decision> = (0..7)
        .map(|tick| model::Decision {
            tick,
            site: None,
            text: format!("decision {tick}"),
        })
        .collect();
    let recent: Vec<u64> = model::recent(&log, 3).iter().map(|d| d.tick).collect();
    assert_eq!(recent, [6, 5, 4], "newest first, bounded");
    assert_eq!(
        model::planner_line("Harbour", 2, 1, 46, "commissioned cottage"),
        "Harbour: 2 without a bed · 1 free bed · 46 goods with no room in store\nLast decision: commissioned cottage"
    );
    assert!(model::planner_line("Harbour", 0, 3, 1, "").ends_with("No decision yet."));
    // The planner's own memory names a building by its key.
    let display = |key: &str| (key == "tailor_shop").then(|| "Tailor's shop".to_owned());
    assert_eq!(
        model::last_decision("commissioned tailor_shop", &display),
        "ordered a tailor's shop"
    );
    assert_eq!(
        model::last_decision("commissioned grain_store", &display),
        "ordered a grain store"
    );
    assert_eq!(
        model::last_decision("deferred: no buildable site near the centre", &display),
        "deferred: no buildable site near the centre"
    );
}

#[test]
fn a_building_card_shows_what_its_kind_needs() {
    let site = Card {
        title: "Harbour cottage 3".into(),
        settlement: Some("Harbour".into()),
        upkeep: None,
        body: CardBody::Site(site((8, 4), 300)),
    };
    assert_eq!(
        site.kind_line(),
        "Building site · ordered by the village · Harbour"
    );
    assert_eq!(site.bar_permille(), Some(750));
    assert_eq!(
        site.lines(),
        [
            "Progress 75%",
            "Materials: planks 8/8 · logs 4/4",
            "Work 300/600",
            "Why: 2 resident(s) have had no bed for 3 reviews",
        ]
    );
    let own = Card {
        body: CardBody::Site(SiteRow {
            civic: false,
            reason: "placed by the overseer".into(),
            ..self::site((0, 0), 0)
        }),
        ..site.clone()
    };
    assert_eq!(own.kind_line(), "Building site · placed by you · Harbour");
    assert_eq!(own.lines().len(), 3, "your own site needs no reason");

    let home = Card {
        title: "Harbour cottage 3".into(),
        settlement: None,
        upkeep: None,
        body: CardBody::Home {
            beds: 2,
            residents: vec!["Ada".into()],
            heated: true,
        },
    };
    assert_eq!(home.kind_line(), "Home");
    assert_eq!(home.bar_permille(), None);
    assert_eq!(
        home.lines(),
        ["Beds 1/2", "Lives here: Ada", "The hearth is lit."]
    );
    let work = Card {
        title: "Harbour smithy 1".into(),
        settlement: Some("Harbour".into()),
        upkeep: None,
        body: CardBody::Workplace {
            trade: "Blacksmith".into(),
            seats: Some(2),
            workers: Vec::new(),
            inside: None,
            guest_beds: Vec::new(),
        },
    };
    assert_eq!(work.kind_line(), "Workplace · Harbour");
    assert_eq!(
        work.lines(),
        ["Blacksmith · 0/2 places taken", "Works here: nobody yet"]
    );
    // An inn also says who is inside now.
    let inn = Card {
        title: "Harbour inn".into(),
        settlement: Some("Harbour".into()),
        upkeep: None,
        body: CardBody::Workplace {
            trade: "Innkeeper".into(),
            seats: None,
            workers: vec!["Mara".into()],
            inside: Some(vec!["Mara".into(), "Odo".into()]),
            guest_beds: vec![Some(("Odo".into(), "06:30".into())), None, None, None],
        },
    };
    assert_eq!(inn.kind_line(), "Inn · Harbour");
    assert_eq!(
        inn.lines(),
        [
            "Innkeeper",
            "Works here: Mara",
            "Guest beds 1/4 taken",
            "Bed 1 · Odo, until 06:30",
            "Bed 2 · free",
            "Bed 3 · free",
            "Bed 4 · free",
            "Inside now: Mara, Odo"
        ]
    );
    // On the tabs: the count on Overview, the beds with the people.
    assert_eq!(
        inn.tab_lines(model::CardTab::Overview),
        [
            "Innkeeper",
            "Guest beds 1/4 taken",
            "2 people are inside now."
        ]
    );
    assert_eq!(
        inn.tab_lines(model::CardTab::People),
        [
            "Mara",
            "Bed 1 · Odo, until 06:30",
            "Bed 2 · free",
            "Bed 3 · free",
            "Bed 4 · free",
            "Inside now: Mara, Odo"
        ]
    );
    let empty_inn = Card {
        body: CardBody::Workplace {
            trade: "Innkeeper".into(),
            seats: None,
            workers: Vec::new(),
            inside: Some(Vec::new()),
            guest_beds: Vec::new(),
        },
        ..inn.clone()
    };
    assert_eq!(empty_inn.lines()[2], "Inside now: nobody");
    let stall = Card {
        title: "Harbour market stall 1".into(),
        settlement: Some("Harbour".into()),
        upkeep: None,
        body: CardBody::Depot {
            stall: true,
            shelf: Shelf {
                used: 15,
                capacity: 60,
                stock: vec![("bread".into(), 5), ("planks".into(), 10)],
                sold_units: 7,
                turnover: 31,
            },
        },
    };
    assert_eq!(stall.kind_line(), "Market stall · Harbour");
    assert_eq!(stall.bar_permille(), Some(250));
    assert_eq!(
        stall.lines(),
        [
            "Stock 15/60 units",
            "10 planks, 5 bread",
            "Sold 7 units for 31 coins",
            "Sells to anyone, visitors included.",
        ]
    );
    let empty = Shelf {
        used: 0,
        capacity: 120,
        stock: Vec::new(),
        sold_units: 0,
        turnover: 0,
    };
    assert_eq!(
        empty.lines()[1],
        "The shelves are empty: its makers restock it as they work."
    );

    // Tabs by kind: a site has one view; a home lists who lives there; a
    // workplace who works there; a store what is on its shelves.
    use model::CardTab;
    assert_eq!(site.tabs(), [CardTab::Overview]);
    assert!(
        !site
            .tab_lines(CardTab::Overview)
            .iter()
            .any(|line| line.starts_with("Progress ")),
        "the bar says how far a site is"
    );
    assert_eq!(home.tabs(), [CardTab::Overview, CardTab::People]);
    assert_eq!(home.tab_label(CardTab::People), "Residents");
    assert_eq!(
        home.tab_lines(CardTab::Overview),
        ["Beds 1/2", "The hearth is lit."]
    );
    assert_eq!(home.tab_lines(CardTab::People), ["Ada"]);
    assert_eq!(work.tab_label(CardTab::People), "Workers");
    assert_eq!(
        work.tab_lines(CardTab::People),
        ["Nobody works here yet: a resident's Home and job tab can request a place here."]
    );
    assert_eq!(inn.tab_label(CardTab::People), "Workers and guests");
    assert_eq!(
        inn.tab_lines(CardTab::Overview),
        [
            "Innkeeper",
            "Guest beds 1/4 taken",
            "2 people are inside now."
        ]
    );
    assert_eq!(inn.tab_lines(CardTab::People).first().unwrap(), "Mara");
    assert_eq!(
        inn.tab_lines(CardTab::People).last().unwrap(),
        "Inside now: Mara, Odo"
    );
    assert_eq!(stall.tabs(), [CardTab::Overview, CardTab::Stock]);
    assert_eq!(
        stall.tab_lines(CardTab::Overview),
        [
            "Stock 15/60 units",
            "Sold 7 units for 31 coins",
            "Sells to anyone, visitors included."
        ]
    );
    assert_eq!(stall.tab_lines(CardTab::Stock), ["10 planks", "5 bread"]);
    assert_eq!(
        empty.stock_lines(),
        ["The shelves are empty: its makers restock it as they work."]
    );
    assert_eq!(
        home.tab_lines(CardTab::Stock),
        home.tab_lines(CardTab::Overview),
        "a tab the card lacks shows its Overview"
    );
    // The header: a picture of the subject and, for a site, its state as a
    // shape, a colour and a word.
    use super::card::look;
    use crate::game_ui::icon::Icon;
    use crate::game_ui::tokens::Status;
    assert_eq!(
        look(&site),
        (
            Icon::BuildingSite,
            Some((Status::Info, "Under way".to_owned()))
        )
    );
    let waiting = Card {
        body: CardBody::Site(SiteRow {
            waiting_days: 2,
            ..self::site((0, 0), 0)
        }),
        ..site.clone()
    };
    assert_eq!(look(&waiting).1, Some((Status::Warn, "Waiting".to_owned())));
    let fresh = Card {
        body: CardBody::Site(self::site((0, 0), 0)),
        ..site.clone()
    };
    assert_eq!(
        look(&fresh).1,
        Some((Status::Info, "Collecting materials".to_owned()))
    );
    let ready = Card {
        body: CardBody::Site(self::site((8, 4), 0)),
        ..site.clone()
    };
    assert_eq!(
        look(&ready).1,
        Some((Status::Info, "Ready to build".to_owned()))
    );
    let unfunded = Card {
        body: CardBody::Site(SiteRow {
            unfunded: true,
            ..self::site((0, 0), 0)
        }),
        ..site.clone()
    };
    assert_eq!(
        look(&unfunded).1,
        Some((Status::Warn, "Waiting".to_owned()))
    );
    let done = Card {
        body: CardBody::Site(self::site((8, 4), 600)),
        ..site.clone()
    };
    assert_eq!(look(&done).1, Some((Status::Good, "Finished".to_owned())));
    assert_eq!(look(&home), (Icon::Homes, None));
    assert_eq!(look(&stall), (Icon::Market, None));
    assert_eq!(look(&work).1, None, "only a site has a state to report");
}

#[test]
fn the_wheel_moves_the_row_of_cards_forward_when_turned_down_or_right() {
    use super::palette::wheel_step;
    use bevy::input::mouse::MouseScrollUnit::{Line, Pixel};
    assert_eq!(wheel_step(Line, 0.0, -1.0), 64.0, "down is forward");
    assert_eq!(wheel_step(Line, 0.0, 1.0), -64.0);
    assert_eq!(wheel_step(Line, 1.0, 0.0), 64.0, "a sideways wheel too");
    assert_eq!(wheel_step(Pixel, 0.0, -30.0), 30.0);
}

#[test]
fn the_tray_is_centred_and_never_wider_than_the_window() {
    use super::palette::tray_rect;
    assert_eq!(tray_rect(1280.0), (232.0, 816.0));
    assert_eq!(tray_rect(1024.0), (104.0, 816.0), "the spec's x at 1024");
    let (left, width) = tray_rect(700.0);
    assert_eq!(left + width + left, 700.0);
    assert!(left >= crate::game_ui::tokens::SCREEN_MARGIN);
    assert_eq!(
        tray_rect(200.0).1,
        320.0,
        "never narrower than a card and the stock"
    );
}

#[test]
fn every_building_has_a_picture_and_a_workplace_takes_its_trades() {
    use crate::game_ui::icon::Icon;
    for def in defs() {
        let icon = model::building_icon(&def.kind);
        match &def.kind {
            BuildingKind::Home { beds } => {
                assert_eq!(
                    icon,
                    if *beds > 2 {
                        Icon::House
                    } else {
                        Icon::Cottage
                    }
                )
            }
            BuildingKind::Workplace { profession, .. } => assert_eq!(
                icon,
                Icon::of_trade(profession.name()).unwrap_or(Icon::Work),
                "{}",
                def.id
            ),
            BuildingKind::Storage { .. } => assert_eq!(icon, Icon::Stockpile),
            BuildingKind::Stall { .. } => assert_eq!(icon, Icon::MarketStall),
        }
    }
    // Every trade the game names has a picture, the smith by either name.
    for trade in [
        "farmer",
        "miller",
        "baker",
        "well_keeper",
        "woodcutter",
        "carpenter",
        "miner",
        "smith",
        "blacksmith",
        "innkeeper",
        "hunter",
        "tanner",
        "tailor",
        "fisher",
    ] {
        assert!(Icon::of_trade(trade).is_some(), "{trade}");
    }
    // A card that cannot be chosen says why where its purpose would be.
    let cottage = &items()[0];
    assert_eq!(
        cottage.unavailable, None,
        "the catalogue has no such rule today"
    );
    assert_eq!(cottage.card_note(), cottage.purpose);
    let locked = PaletteItem {
        unavailable: Some("Needs a carpenter in the village.".into()),
        ..cottage.clone()
    };
    assert_eq!(locked.card_note(), "Needs a carpenter in the village.");
    // The tag turns the simulation's words into one sentence, once.
    let cottage = &items()[0];
    let tag = model::tag(cottage, Some("too close to the shore."));
    assert_eq!(tag.reason.as_deref(), Some("Too close to the shore."));
}

#[test]
fn errands_name_the_site_the_store_and_the_stall() {
    assert_eq!(
        model::errand_label("build", "", Some("Harbour cottage 3")).as_deref(),
        Some("building Harbour cottage 3")
    );
    assert_eq!(
        model::errand_label("haul", "planks", Some("Harbour smithy 2")).as_deref(),
        Some("carrying planks to Harbour smithy 2")
    );
    assert_eq!(
        model::errand_label("fetch", "logs", Some("Harbour stockpile 1")).as_deref(),
        Some("fetching logs from Harbour stockpile 1")
    );
    assert_eq!(
        model::errand_label("store", "grain", Some("Harbour barn 1")).as_deref(),
        Some("storing grain at Harbour barn 1")
    );
    assert_eq!(
        model::errand_label("restock", "bread", Some("Harbour market stall 1")).as_deref(),
        Some("restocking Harbour market stall 1 with bread")
    );
    assert_eq!(
        model::errand_label("build_cottage", "cottage", None).as_deref(),
        Some("building a cottage"),
        "a resident's own cottage keeps its words"
    );
    assert_eq!(
        model::errand_label("haul", "planks", None).as_deref(),
        Some("carrying planks to a building site")
    );
    assert_eq!(model::errand_label("gather", "grain", None), None);
    assert_eq!(
        model::shelf_trade_label("sell", "bread", "Harbour market stall 1").as_deref(),
        Some("selling bread at Harbour market stall 1")
    );
    assert_eq!(
        model::shelf_trade_label("buy", "planks", "Harbour stockpile 1").as_deref(),
        Some("buying planks at Harbour stockpile 1")
    );
    assert_eq!(model::shelf_trade_label("craft", "bread", "x"), None);
    // A job stated for an earlier leaf never names what this one is doing.
    assert!(model::errand_verbs("building").contains(&"build"));
    assert!(model::errand_verbs("carrying building materials").contains(&"fetch"));
    assert!(!model::errand_verbs("delivering goods").contains(&"haul"));
    assert!(model::errand_verbs("sleeping at home").is_empty());
}

#[test]
fn stock_and_material_props_follow_the_numbers() {
    assert_eq!(model::filled_slots(0, 120, 9), 0);
    assert_eq!(model::filled_slots(1, 120, 9), 1, "anything at all shows");
    assert_eq!(model::filled_slots(60, 120, 9), 5);
    assert_eq!(model::filled_slots(120, 120, 9), 9);
    assert_eq!(model::filled_slots(400, 120, 9), 9);
    assert_eq!(model::filled_slots(5, 0, 9), 0);
    // The pile beside a site is used up by the work.
    assert_eq!(model::pile_units(12, 0, 600), 12);
    assert_eq!(model::pile_units(12, 300, 600), 6);
    assert_eq!(model::pile_units(12, 600, 600), 0);
    assert_eq!(model::pile_units(12, 599, 600), 1);
    // Props are shared out by what lies there; a small lot still shows.
    assert_eq!(model::spread(&[('a', 10)], 3), ['a', 'a', 'a']);
    assert_eq!(
        model::spread(&[('a', 5), ('b', 5)], 4),
        ['a', 'a', 'b', 'b']
    );
    assert_eq!(model::spread(&[('a', 30), ('b', 1)], 3), ['a', 'a', 'b']);
    assert_eq!(model::spread(&[('a', 30), ('b', 1)], 1), ['a']);
    assert!(model::spread::<char>(&[], 4).is_empty());
    assert!(model::spread(&[('a', 0)], 4).is_empty());
}

#[test]
fn a_click_finds_the_building_its_ray_meets_first() {
    let (lo, hi) = ([0.0, 0.0, 0.0], [4.0, 3.0, 5.0]);
    // Straight down into the roof.
    assert_eq!(
        model::ray_box([2.0, 10.0, 2.0], [0.0, -1.0, 0.0], lo, hi),
        Some(7.0)
    );
    // Past the corner.
    assert_eq!(
        model::ray_box([6.0, 10.0, 2.0], [0.0, -1.0, 0.0], lo, hi),
        None
    );
    // Behind the camera.
    assert_eq!(
        model::ray_box([2.0, 10.0, 2.0], [0.0, 1.0, 0.0], lo, hi),
        None
    );
    // An oblique ray through the front wall.
    let hit = model::ray_box([2.0, 6.0, -6.0], [0.0, -0.6, 0.8], lo, hi).unwrap();
    assert!((hit - 7.5).abs() < 0.01, "{hit}");
    // The same ray from lower down passes under the floor in front of it.
    assert_eq!(
        model::ray_box([2.0, 4.0, -6.0], [0.0, -0.6, 0.8], lo, hi),
        None
    );
}

#[test]
fn a_plot_frame_puts_models_on_the_body_for_every_turn() {
    let def = &defs()[0];
    for rotation in 0..4 {
        let plot = civic::plot_for(def, 10, -4, rotation);
        let frame = scene::frame(&plot);
        assert_eq!((frame.width, frame.depth), (4.0, 5.0), "turn {rotation}");
        // The body's centre is three metres behind the entrance's centre.
        let behind = (frame.centre - frame.door).dot(frame.dir);
        assert!((behind - 3.0).abs() < 1e-4, "turn {rotation}: {behind}");
        // A home's model stands on the centre, at the founding homes' offset.
        let home = frame.home_root();
        let model = home.transform_point(Vec3::new(0.0, 0.0, HOME_OFFSET_MM as f32 / 1000.0));
        assert!(model.distance(frame.centre) < 1e-3, "turn {rotation}");
        let work = frame.workplace_root();
        let model = work.transform_point(Vec3::new(0.0, 0.0, WORKPLACE_OFFSET_MM as f32 / 1000.0));
        assert!(model.distance(frame.centre) < 1e-3, "turn {rotation}");
        // Both roots stand outside the body, on the entrance's side.
        assert!((home.translation - frame.centre).dot(frame.dir) < -2.9);
    }
    let turned = scene::frame(&civic::plot_for(def, 0, 0, 1));
    assert_eq!(turned.dir, Vec3::X);
    assert!((scene::yaw_to(Vec3::X) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
}

#[test]
fn the_cursor_note_keeps_clear_of_the_ledger() {
    use super::palette::{NoteSide, note_side};
    assert_eq!(note_side(200.0, 1280.0), NoteSide::Right);
    assert_eq!(note_side(640.0, 1280.0), NoteSide::Left);
    assert_eq!(note_side(512.0, 1024.0), NoteSide::Left);
    assert_eq!(note_side(320.0, 1024.0), NoteSide::Right);
}

fn fixture() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    set_journal(app.world_mut(), true, false);
    app.init_resource::<BuildTool>()
        .init_resource::<CivicLog>()
        .init_resource::<Selected>()
        .init_resource::<SimControl>()
        .insert_resource(IslandMode::Inspector);
    app
}

/// A spot the simulation accepts for `building` near the Harbour.
fn spot(app: &App, building: &str) -> (i32, i32, u8) {
    let world = app.world();
    let settlement = world.resource::<IslandIds>().id("settlement_harbour");
    let def = civic::definition(world, building).expect("definition");
    civic::find_site(world, def, settlement, 36).expect("a buildable spot")
}

#[test]
fn the_shipped_catalogue_fills_all_four_groups() {
    let app = fixture();
    let items = super::items(app.world());
    assert_eq!(
        items.len(),
        civic::catalog(app.world()).len(),
        "every building of the world's data is offered"
    );
    let groups = model::grouped(&items);
    let offered: Vec<Group> = groups.iter().map(|(group, _)| *group).collect();
    // The four groups, and Farms only where the data has a building with
    // crop plots: then the farmers' workplace is offered with it.
    assert_eq!(offered, Group::ALL[..offered.len()]);
    assert!(offered.len() >= 4);
    let farms = civic::catalog(app.world())
        .iter()
        .any(|def| def.field.is_some());
    assert_eq!(offered.contains(&Group::Farms), farms);
    let ids = |wanted: Group| -> Vec<&str> {
        groups
            .iter()
            .find(|(group, _)| *group == wanted)
            .map(|(_, items)| items.iter().map(|item| item.id.as_str()).collect())
            .unwrap_or_default()
    };
    assert!(ids(Group::Homes).contains(&"cottage"));
    assert!(
        ids(Group::Workplaces).len() + ids(Group::Farms).len() >= 13,
        "one per profession"
    );
    if farms {
        assert!(ids(Group::Farms).contains(&"field"));
        assert!(
            !ids(Group::Storage).contains(&"field"),
            "a field is offered once, with the farms"
        );
        let field = items.iter().find(|item| item.id == "field").unwrap();
        assert!(field.purpose.contains("plots of"), "{}", field.purpose);
        assert_eq!(field.icon, crate::game_ui::icon::Icon::Field);
    }
    // Without a farm building in the data there is no Farms group at all:
    // the farmers' workplace stays with the other workplaces.
    let without: Vec<BuildingDef> = civic::catalog(app.world())
        .iter()
        .filter(|def| def.field.is_none())
        .cloned()
        .collect();
    let plain = model::palette(&without, &|kind| kind.to_owned());
    assert!(plain.iter().all(|item| item.group != Group::Farms));
    assert_eq!(model::grouped(&plain).len(), 4);
    assert!(ids(Group::Workplaces).contains(&"smithy"));
    assert!(ids(Group::Storage).contains(&"stockpile") && ids(Group::Storage).contains(&"barn"));
    assert!(ids(Group::Market).contains(&"market_stall"));
    let cottage = items.iter().find(|item| item.id == "cottage").unwrap();
    assert_eq!(cottage.cost, "8 planks, 4 logs", "goods by display name");
}

#[test]
fn placing_through_the_tool_records_one_command_and_reading_changes_nothing() {
    let mut app = fixture();
    let (x, z, rotation) = spot(&app, "stockpile");
    {
        let mut tool = app.world_mut().resource_mut::<BuildTool>();
        tool.requests.push(Action::Toggle);
        tool.requests.push(Action::Pick("stockpile".into()));
    }
    process(app.world_mut());
    {
        let mut tool = app.world_mut().resource_mut::<BuildTool>();
        assert!(tool.open && tool.selected.as_deref() == Some("stockpile"));
        tool.hover = Some((x, z));
        tool.rotation = rotation;
    }
    // The ghost is the simulation's own answer; asking changes nothing.
    let before = state_hash(app.world_mut());
    process(app.world_mut());
    let ghost = app.world().resource::<BuildTool>().ghost.clone().unwrap();
    assert_eq!(ghost.tint(), Tint::Valid, "{:?}", ghost.blocked);
    assert_eq!(ghost.plot.map(|plot| plot.cells().count()), Some(16));
    assert!(site_rows(app.world()).is_empty());
    assert_eq!(before, state_hash(app.world_mut()), "the ghost only reads");

    app.world_mut()
        .resource_mut::<BuildTool>()
        .requests
        .push(Action::Place);
    process(app.world_mut());
    assert_eq!(app.world().resource::<BuildTool>().queued(), 1);
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    let tool = app.world().resource::<BuildTool>();
    assert_eq!(tool.queued(), 0);
    assert!(tool.status.contains("site opened"), "{}", tool.status);
    assert!(
        tool.selected.is_some(),
        "the building stays in hand for the next one"
    );
    let rows = site_rows(app.world());
    assert_eq!(rows.len(), 1);
    let (row, door) = &rows[0];
    assert!(row.label.contains("stockpile"), "{}", row.label);
    assert!(!row.civic);
    assert_eq!(row.materials, [("logs".to_owned(), 0, 4)]);
    assert_eq!((door.x, door.z), (x * 1000 + 500, z * 1000 + 500));
    // The same cell is now taken: the next check is red and says why.
    process(app.world_mut());
    let ghost = app.world().resource::<BuildTool>().ghost.clone().unwrap();
    assert_eq!(ghost.tint(), Tint::Blocked);
    assert!(
        app.world()
            .resource::<BuildTool>()
            .cursor_reason()
            .is_some()
    );
    // A click on a red ghost records nothing.
    app.world_mut()
        .resource_mut::<BuildTool>()
        .requests
        .push(Action::Place);
    process(app.world_mut());
    let tool = app.world().resource::<BuildTool>();
    assert_eq!(tool.queued(), 0);
    assert!(tool.status.contains("not placed"), "{}", tool.status);

    // Reading the panel's data and the card never changes the simulation.
    let id = SimId(row.id);
    let before = state_hash(app.world_mut());
    let card = card(app.world(), id).expect("site card");
    assert!(matches!(card.body, CardBody::Site(_)));
    assert_eq!(card.settlement.as_deref(), Some("Harbour"));
    assert!(fund_line(app.world()).contains("1 open site"));
    assert_eq!(planner_lines(app.world()).len(), 3);
    assert_eq!(before, state_hash(app.world_mut()));

    // Esc puts the building down, then closes the palette.
    for (action, open, holding) in [
        (Action::CancelTool, true, false),
        (Action::Close, false, false),
    ] {
        app.world_mut()
            .resource_mut::<BuildTool>()
            .requests
            .push(action);
        process(app.world_mut());
        let tool = app.world().resource::<BuildTool>();
        assert_eq!((tool.open, tool.selected.is_some()), (open, holding));
        assert_eq!(tool.ghost.is_some(), holding);
    }
}

#[test]
fn a_refused_placement_is_reported_with_the_simulations_reason() {
    let mut app = fixture();
    let (x, z, rotation) = spot(&app, "stockpile");
    {
        let mut tool = app.world_mut().resource_mut::<BuildTool>();
        tool.requests.push(Action::Toggle);
        tool.requests.push(Action::Pick("stockpile".into()));
    }
    process(app.world_mut());
    {
        let mut tool = app.world_mut().resource_mut::<BuildTool>();
        tool.hover = Some((x, z));
        tool.rotation = rotation;
        // Two clicks before a tick: both pass the check, the second is refused.
        tool.requests.push(Action::Place);
        tool.requests.push(Action::Place);
    }
    process(app.world_mut());
    assert_eq!(app.world().resource::<BuildTool>().queued(), 2);
    run_ticks(app.world_mut(), 2);
    process(app.world_mut());
    let tool = app.world().resource::<BuildTool>();
    assert_eq!(tool.queued(), 0);
    assert_eq!(site_rows(app.world()).len(), 1, "one site, not two");
    assert!(
        tool.status.contains("not placed"),
        "the refusal is shown: {}",
        tool.status
    );
    assert!(tool.cursor_reason().is_some());
}

#[test]
fn the_tool_belongs_to_oversee_and_yields_the_column_to_a_resident() {
    let mut app = fixture();
    app.init_resource::<crate::resident_inspector::Inspector>();
    {
        let mut tool = app.world_mut().resource_mut::<BuildTool>();
        tool.requests.push(Action::Toggle);
        tool.requests.push(Action::Pick("cottage".into()));
    }
    process(app.world_mut());
    assert!(palette_open(app.world()) && column_taken(app.world()));
    // Choosing a resident hands the column to their card.
    let resident = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, entity)| app.world().get::<Villager>(**entity).is_some())
        .map(|(id, _)| *id)
        .unwrap();
    app.world_mut().resource_mut::<Selected>().0 = Some(resident);
    process(app.world_mut());
    let tool = app.world().resource::<BuildTool>();
    assert!(!tool.open && tool.selected.is_none());
    assert!(!column_taken(app.world()));
    // Opening the palette again closes the resident's card.
    app.world_mut()
        .resource_mut::<BuildTool>()
        .requests
        .push(Action::Toggle);
    process(app.world_mut());
    assert!(palette_open(app.world()));
    assert_eq!(app.world().resource::<Selected>().0, None);
    // A building that cannot be chosen is not taken in hand: the reason is
    // the status. (No shipped building is unavailable; this one is made so.)
    {
        let mut tool = app.world_mut().resource_mut::<BuildTool>();
        let item = tool
            .items
            .iter_mut()
            .find(|item| item.id == "cottage")
            .unwrap();
        item.unavailable = Some("Needs a carpenter in the village.".into());
        tool.requests.push(Action::Pick("cottage".into()));
    }
    process(app.world_mut());
    {
        let mut tool = app.world_mut().resource_mut::<BuildTool>();
        assert_eq!(tool.selected, None);
        assert_eq!(tool.status, "Needs a carpenter in the village.");
        let item = tool
            .items
            .iter_mut()
            .find(|item| item.id == "cottage")
            .unwrap();
        item.unavailable = None;
    }
    // Leaving Oversee puts the building down and hides the palette.
    app.world_mut()
        .resource_mut::<BuildTool>()
        .requests
        .push(Action::Pick("cottage".into()));
    process(app.world_mut());
    assert!(app.world().resource::<BuildTool>().selected.is_some());
    app.insert_resource(IslandMode::Editor);
    process(app.world_mut());
    assert!(app.world().resource::<BuildTool>().selected.is_none());
    assert!(!palette_open(app.world()) && !column_taken(app.world()));
}

#[test]
fn clicking_a_building_opens_its_card_and_a_founding_home_has_one_too() {
    let mut app = fixture();
    let (home_id, door) = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find_map(|(id, entity)| {
            let home = app.world().get::<Household>(*entity)?;
            Some((*id, home.door))
        })
        .expect("a founding home");
    // Straight down onto the roof, behind the door.
    let above = Vec3::new(
        door.x as f32 / 1000.0,
        40.0,
        (door.z + HOME_OFFSET_MM) as f32 / 1000.0,
    );
    let ray = Ray3d::new(above, Dir3::NEG_Y);
    assert_eq!(pick(app.world(), ray), Some(home_id));
    // The box is the house as it is drawn: every cell of the body the
    // simulation gives it, from wall to wall, and as high as its chimney.
    let home = village::view::home(app.world(), home_id).expect("the home as a place");
    let boxes = targets(app.world());
    let (_, lo, hi) = boxes
        .iter()
        .find(|(id, ..)| *id == home_id)
        .expect("a box for the home");
    assert_eq!(
        (lo.x, lo.z, hi.x, hi.z),
        (
            home.body_lo.0 as f32,
            home.body_lo.1 as f32,
            home.body_hi.0 as f32 + 1.0,
            home.body_hi.1 as f32 + 1.0
        )
    );
    assert!(hi.y - lo.y > 5.0, "roof and chimney included");
    // Down onto each corner cell of the roof, and level through an outer
    // wall above head height: the house, not the ground behind it.
    for (x, z) in [
        (home.body_lo.0, home.body_lo.1),
        (home.body_hi.0, home.body_lo.1),
        (home.body_lo.0, home.body_hi.1),
        (home.body_hi.0, home.body_hi.1),
    ] {
        let down = Ray3d::new(Vec3::new(x as f32 + 0.5, 40.0, z as f32 + 0.5), Dir3::NEG_Y);
        assert_eq!(pick(app.world(), down), Some(home_id), "roof over {x}, {z}");
    }
    let wall = Ray3d::new(
        Vec3::new(lo.x - 6.0, lo.y + 3.5, (lo.z + hi.z) * 0.5),
        Dir3::X,
    );
    assert_eq!(pick(app.world(), wall), Some(home_id), "an outer wall");
    // The first pass adopts the session; a click belongs to a running one.
    process(app.world_mut());
    app.world_mut().resource_mut::<BuildTool>().click = Some(ray);
    process(app.world_mut());
    assert_eq!(card_subject(app.world()), Some(home_id));
    let card = card(app.world(), home_id).expect("home card");
    assert!(matches!(card.body, CardBody::Home { .. }));
    // Open ground closes it.
    let nowhere = Ray3d::new(Vec3::new(4000.0, 40.0, 4000.0), Dir3::NEG_Y);
    app.world_mut().resource_mut::<BuildTool>().click = Some(nowhere);
    process(app.world_mut());
    assert_eq!(card_subject(app.world()), None);
}

#[test]
fn planner_decisions_are_logged_from_the_journal_with_names() {
    let mut app = fixture();
    let settlement = app.world().resource::<IslandIds>().id("settlement_harbour");
    record_effect(
        app.world_mut(),
        None,
        "VillageCivicProject",
        &json!({"settlement": settlement, "decision": "commissioned", "building": "cottage",
            "site": 4000, "reason": "1 resident(s) have had no bed for 3 reviews",
            "estimated_cost": 40, "fund": 300}),
    );
    record_effect(
        app.world_mut(),
        None,
        "VillageTrade",
        &json!({"kind": "bread", "quantity": 1, "coins": 3}),
    );
    track(app.world_mut());
    let decisions = recent_decisions(app.world(), 5);
    assert_eq!(decisions.len(), 1, "only planner decisions are logged");
    assert_eq!(decisions[0].site, Some(4000));
    assert_eq!(
        decisions[0].text,
        "Harbour ordered a cottage: 1 resident(s) have had no bed for 3 reviews (about 40 coins; fund 300)"
    );
    // Nothing new in the journal: nothing is added twice.
    track(app.world_mut());
    assert_eq!(recent_decisions(app.world(), 5).len(), 1);
}

#[test]
fn a_workplace_card_is_titled_by_its_building() {
    use super::model::workplace_title;
    assert_eq!(
        workplace_title("Harbour innkeeper", "innkeeper", None),
        "Harbour inn"
    );
    assert_eq!(
        workplace_title("Alderwood baker", "baker", None),
        "Alderwood bakery"
    );
    assert_eq!(workplace_title("innkeeper", "innkeeper", None), "Inn");
    assert_eq!(
        workplace_title("The old mill", "miller", None),
        "The old mill"
    );
    assert_eq!(
        workplace_title("Harbour innkeeper", "innkeeper", Some("Harbour inn")),
        "Harbour inn"
    );
}

#[test]
fn a_building_card_shows_its_condition_and_whether_it_needs_repair() {
    let upkeep = model::CardUpkeep {
        condition: 420,
        needs_repair: true,
        worn: false,
        unusable: false,
        burning: false,
        repairing: false,
    };
    let home = Card {
        title: "Harbour cottage 3".into(),
        settlement: None,
        upkeep: Some(upkeep),
        body: CardBody::Home {
            beds: 2,
            residents: vec!["Ada".into()],
            heated: false,
        },
    };
    assert_eq!(
        home.tab_lines(model::CardTab::Overview)[..2],
        [
            "Condition 42% \u{b7} needs repair".to_owned(),
            "The village repairs it when it has the builders and the coins.".to_owned()
        ]
    );
    assert_eq!(home.lines()[0], "Condition 42% \u{b7} needs repair");
    let burning = Card {
        upkeep: Some(model::CardUpkeep {
            condition: 0,
            unusable: true,
            worn: true,
            burning: true,
            repairing: true,
            ..upkeep
        }),
        ..home.clone()
    };
    assert_eq!(
        burning.lines()[..3],
        [
            "On fire: residents nearby are fighting it.".to_owned(),
            "Condition 0% \u{b7} unusable until repaired".to_owned(),
            "Repairs are under way.".to_owned()
        ]
    );
    // A sound building just says so.
    let sound = Card {
        upkeep: Some(model::CardUpkeep {
            condition: 980,
            needs_repair: false,
            ..upkeep
        }),
        ..home
    };
    assert_eq!(sound.lines()[0], "Condition 98%");
    assert_eq!(sound.tab_lines(model::CardTab::People), ["Ada"]);
}

/// The card of a running world reads the simulation's own condition and
/// records nothing.
#[test]
fn the_card_of_a_founding_home_reads_its_condition() {
    let mut app = island_content::try_new_sim_app(7, "living_world", 1).expect("island");
    run_ticks(app.world_mut(), 2);
    let home = app.world().resource::<IslandIds>().id("harbour_home_00");
    let hash = state_hash(app.world_mut());
    let card = card(app.world(), home).expect("a card");
    let upkeep = card.upkeep.expect("a home wears");
    assert!((600..=1000).contains(&upkeep.condition));
    assert!(card.lines()[0].starts_with("Condition "));
    assert_eq!(state_hash(app.world_mut()), hash, "a card records nothing");
}
