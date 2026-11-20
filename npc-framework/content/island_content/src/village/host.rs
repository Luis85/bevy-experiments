//! Host-side diary publication. Never scheduled in NpcSimTick; files never enter sim decisions.
use super::{PublishDiary, model::*};
use bevy_ecs::prelude::*;
use npc_framework::agent_io::notes::{Book, NotesDirs, NotesStores};
use npc_framework::kernel::commands::CommandQueue;
use npc_framework::prelude::*;
use std::collections::BTreeSet;

#[derive(Resource, Default)]
pub struct DiaryBridge {
    pub error: Option<String>,
}
/// Called by the windowed host each frame and before saving.
pub fn frame(world: &mut World) {
    if world.get_resource::<VillageState>().is_none() {
        return;
    }
    let excluded = world
        .query::<(&SimId, &Vitality)>()
        .iter(world)
        .filter(|(_, v)| v.died_at.is_some())
        .map(|(id, _)| (*id, "actor is permanently dead".into()))
        .collect();
    world.insert_resource(npc_framework::agent_io::scheduler::SchedulerExclusions(
        excluded,
    ));
    let queued: BTreeSet<_> = world
        .get_resource::<CommandQueue>()
        .into_iter()
        .flat_map(|q| q.pending.values())
        .filter_map(|c| match c {
            SimCommand::Content { kind, payload } if kind == "VillagePublishDiary" => payload
                .0
                .iter()
                .find(|(name, _)| name == "diary")
                .and_then(|(_, v)| v.as_u64())
                .and_then(|id| u32::try_from(id).ok())
                .map(SimId),
            _ => None,
        })
        .collect();
    let mut jobs: Vec<_> = world
        .query::<(&SimId, &DroppedDiary)>()
        .iter(world)
        .filter(|(id, d)| !d.sealed && !queued.contains(id))
        .map(|(id, d)| (*id, d.clone()))
        .collect();
    jobs.sort_by_key(|(id, _)| *id);
    for (id, book) in jobs {
        let mut pages = book.pages.clone();
        let dir = world
            .get_resource::<NotesDirs>()
            .and_then(|d| d.0.get(&book.author).cloned());
        if let Some(dir) = dir {
            let read = world
                .get_resource_or_init::<NotesStores>()
                .store_for(&dir)
                .and_then(|store| {
                    let notes = store.list(Book::Diary)?;
                    Ok((notes, store.unreadable(Book::Diary)))
                });
            match read {
                Ok((notes, unreadable)) => {
                    if !unreadable.is_empty() {
                        world.get_resource_or_init::<DiaryBridge>().error=Some("Diary publication is waiting for unreadable Diary files to be fixed in F7. Original files are preserved.".into());
                        continue;
                    }
                    for note in notes.into_iter().filter(|n| {
                        n.created.tick <= book.died_at && n.updated.tick <= book.died_at
                    }) {
                        let page = DiaryPage {
                            tick: note.created.tick,
                            title: note.title,
                            body: note.body,
                        };
                        if !pages.contains(&page) {
                            pages.push(page);
                        }
                    }
                }
                Err(e) => {
                    world.get_resource_or_init::<DiaryBridge>().error =
                        Some(format!("Could not publish the dropped diary: {e}"));
                    continue;
                }
            }
        }
        pages.sort_by(|a, b| {
            a.tick
                .cmp(&b.tick)
                .then(a.title.cmp(&b.title))
                .then(a.body.cmp(&b.body))
        });
        let command = PublishDiary {
            unit: book.author.0,
            diary: id,
            pages,
        };
        match submit(world, None, SimCommand::content(&command)) {
            Ok(_) => {
                let mut b = world.get_resource_or_init::<DiaryBridge>();
                b.error = None;
            }
            Err(e) => {
                world.get_resource_or_init::<DiaryBridge>().error =
                    Some(format!("Could not publish the dropped diary: {e}"));
            }
        }
    }
}
