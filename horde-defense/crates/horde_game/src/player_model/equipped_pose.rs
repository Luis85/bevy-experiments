//! Deterministic source-coordinate FK and contact constraints. There is no
//! image fitting, weapon shrinking, mutable Game or presentation clock here.
use super::attachments::{ActivationKind, HoldDefinition, PoseActionKind, PoseTrack};
use super::bind::Affine2;
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: [f32; 2], f: f32) -> [f32; 2] {
    [a[0] * f, a[1] * f]
}
fn length(a: [f32; 2]) -> f32 {
    a[0].hypot(a[1])
}
fn angle(a: [f32; 2]) -> f32 {
    a[1].atan2(a[0])
}
fn finite(p: [f32; 2]) -> bool {
    p.iter().all(|v| v.is_finite() && v.abs() < 1_000_000_000.)
}
fn check_sample(s: PlayerPoseSample) -> Result<(), String> {
    let phase = match s.action {
        PlayerAction::Bind | PlayerAction::Hold => 0.,
        PlayerAction::Reload { phase }
        | PlayerAction::Melee { phase }
        | PlayerAction::Throw { phase } => phase,
    };
    if !finite(s.aim)
        || [s.recoil, s.brace, s.hit, phase]
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
    {
        return Err("Invalid immutable player pose sample".into());
    }
    Ok(())
}
fn selected_actions(s: PlayerPoseSample) -> Vec<PoseActionKind> {
    let mut out = match s.action {
        PlayerAction::Bind => vec![],
        PlayerAction::Hold => vec![PoseActionKind::Aim],
        PlayerAction::Reload { .. } => vec![PoseActionKind::Reload],
        PlayerAction::Melee { .. } => vec![PoseActionKind::Melee],
        PlayerAction::Throw { .. } => vec![PoseActionKind::Throw],
    };
    if s.recoil > 0. {
        out.push(PoseActionKind::Recoil);
    }
    if s.brace > 0. {
        out.push(PoseActionKind::Brace);
    }
    if s.hit > 0. {
        out.push(PoseActionKind::Hit);
    }
    out
}
fn attachment_active(
    a: &attachments::AttachmentDefinition,
    originals: &BTreeSet<&str>,
    actions: &[PoseActionKind],
) -> bool {
    if !a
        .activation
        .required_parts
        .iter()
        .all(|id| originals.contains(id.as_str()))
    {
        return false;
    }
    let action = a
        .activation
        .actions
        .iter()
        .any(|kind| actions.contains(kind));
    match a.activation.kind {
        ActivationKind::SkinSelected => a.activation.actions.is_empty() || action,
        ActivationKind::Revealed => {
            a.activation
                .missing_parts
                .iter()
                .any(|id| !originals.contains(id.as_str()))
                || action
        }
    }
}
fn publish_socket(
    aliases: &mut BTreeMap<String, [f32; 2]>,
    id: &str,
    point: [f32; 2],
) -> Result<(), String> {
    if let Some(previous) = aliases.get(id) {
        if length(sub(*previous, point)) > 0.01 {
            return Err(format!(
                "Active attachments disagree on rendered socket {id}"
            ));
        }
    } else {
        aliases.insert(id.into(), point);
    }
    Ok(())
}
fn track_delta(track: &PoseTrack, phase: f32) -> BTreeMap<String, ([f32; 2], f32)> {
    let phase = phase.clamp(0., 1.);
    let mut left = &track.keys[0];
    let mut right = left;
    for key in &track.keys {
        if key.phase <= phase {
            left = key;
        }
        if key.phase >= phase {
            right = key;
            break;
        }
        right = key;
    }
    let t = if right.phase > left.phase {
        (phase - left.phase) / (right.phase - left.phase)
    } else {
        0.
    };
    left.bones
        .iter()
        .map(|a| {
            let b = right.bones.iter().find(|b| b.bone == a.bone).unwrap_or(a);
            (
                a.bone.clone(),
                (
                    [
                        a.translation[0] + (b.translation[0] - a.translation[0]) * t,
                        a.translation[1] + (b.translation[1] - a.translation[1]) * t,
                    ],
                    a.rotation + (b.rotation - a.rotation) * t,
                ),
            )
        })
        .collect()
}
fn apply_track(
    c: &CompiledCatalogue,
    id: &Option<String>,
    phase: f32,
    deltas: &mut BTreeMap<String, ([f32; 2], f32)>,
) -> Result<(), String> {
    let Some(id) = id else {
        return Ok(());
    };
    let track = c
        .definition
        .pose_tracks
        .iter()
        .find(|t| t.id == *id)
        .ok_or("Missing validated pose track")?;
    for (bone, (translation, rotation)) in track_delta(track, phase) {
        let entry = deltas.entry(bone).or_insert(([0.; 2], 0.));
        entry.0 = add(entry.0, translation);
        entry.1 += rotation;
    }
    Ok(())
}
fn source_fk(model: &CompiledModel, deltas: &BTreeMap<String, ([f32; 2], f32)>) -> Vec<Affine2> {
    let mut out = Vec::with_capacity(model.bind.bones.len());
    for bone in &model.bind.bones {
        let parent = bone.parent.map_or(Affine2::IDENTITY, |i| out[i]);
        let local = if let Some((translation, rotation)) = deltas.get(&bone.id) {
            Affine2::translation(*translation).compose(Affine2::around(bone.origin, *rotation))
        } else {
            Affine2::IDENTITY
        };
        out.push(parent.compose(local));
    }
    out
}
fn model_socket(model: &CompiledModel, id: &str) -> Result<([f32; 2], usize), String> {
    let s = model
        .definition
        .sockets
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("Missing model socket {id}"))?;
    Ok((
        s.bind_position,
        *model
            .bind
            .indices
            .get(&s.bone)
            .ok_or("Missing socket bone")?,
    ))
}
fn weapon_bind_sockets(
    c: &CompiledCatalogue,
    model: &CompiledModel,
    hold: &HoldDefinition,
) -> Result<BTreeMap<String, [f32; 2]>, String> {
    if let Some(id) = &hold.weapon_attachment {
        let a = c.attachments.get(id).ok_or("Missing held attachment")?;
        Ok(a.definition
            .sockets
            .iter()
            .map(|s| (s.id.clone(), a.binding.point(s.source_position)))
            .collect())
    } else {
        Ok(model
            .definition
            .sockets
            .iter()
            .map(|s| (s.id.clone(), s.bind_position))
            .collect())
    }
}
fn segment_delta(a: [f32; 2], b: [f32; 2], start: [f32; 2], end: [f32; 2]) -> Affine2 {
    Affine2::translation(sub(start, a)).compose(Affine2::around(
        a,
        angle(sub(end, start)) - angle(sub(b, a)),
    ))
}
fn solve_chain(
    model: &CompiledModel,
    hand_bone: &str,
    hand_delta: Affine2,
    world: &mut [Affine2],
) -> Result<(), String> {
    let hand = *model
        .bind
        .indices
        .get(hand_bone)
        .ok_or("Unknown held hand bone")?;
    let fore = model.bind.bones[hand]
        .parent
        .ok_or("Hand lacks a forearm")?;
    let upper = model.bind.bones[fore]
        .parent
        .ok_or("Forearm lacks an upper arm")?;
    let u = &model.bind.bones[upper];
    let f = &model.bind.bones[fore];
    let ul = u
        .length
        .filter(|v| *v > 0.)
        .ok_or("Upper arm has no source segment")?;
    let fl = f
        .length
        .filter(|v| *v > 0.)
        .ok_or("Forearm has no source segment")?;
    let shoulder = world[upper].point(u.origin);
    let wrist = hand_delta.point(model.bind.bones[hand].origin);
    let delta = sub(wrist, shoulder);
    let distance = length(delta);
    if distance > ul + fl + 0.01 || distance < (ul - fl).abs() - 0.01 {
        return Err(format!(
            "Unsupported source hold reach for {hand_bone}: distance {distance:.2}, source bones {ul:.2}+{fl:.2}; author a hold/aim cel or track"
        ));
    }
    let d = distance.max(0.001);
    let axis = mul(delta, 1. / d);
    let along = (ul * ul + d * d - fl * fl) / (2. * d);
    let h = (ul * ul - along * along).max(0.).sqrt();
    let center = add(shoulder, mul(axis, along));
    let perpendicular = mul([-axis[1], axis[0]], h);
    let a = add(center, perpendicular);
    let b = sub(center, perpendicular);
    let elbow = if a[1] >= b[1] { a } else { b };
    world[upper] = segment_delta(u.origin, u.tip.unwrap(), shoulder, elbow);
    world[fore] = segment_delta(f.origin, f.tip.unwrap(), elbow, wrist);
    world[hand] = hand_delta;
    Ok(())
}
fn support_position(track: &attachments::SupportTrack, phase: f32) -> [f32; 2] {
    let phase = phase.clamp(0., 1.);
    let mut left = &track.keys[0];
    let mut right = left;
    for key in &track.keys {
        if key.phase <= phase {
            left = key;
        }
        if key.phase >= phase {
            right = key;
            break;
        }
        right = key;
    }
    let t = if right.phase > left.phase {
        (phase - left.phase) / (right.phase - left.phase)
    } else {
        0.
    };
    add(
        left.source_position,
        mul(sub(right.source_position, left.source_position), t),
    )
}
fn aim_phase(
    model: &CompiledModel,
    hold: &HoldDefinition,
    target: [f32; 2],
) -> Result<f32, String> {
    let heading = angle(sub(target, model_socket(model, "shoulder.near")?.0)).to_degrees();
    Ok(((heading - hold.aim_range[0]) / (hold.aim_range[1] - hold.aim_range[0])).clamp(0., 1.))
}
fn held_bind(
    c: &CompiledCatalogue,
    model: &CompiledModel,
    hold: &HoldDefinition,
    target: [f32; 2],
) -> Result<BTreeMap<String, [f32; 2]>, String> {
    let mut bind = weapon_bind_sockets(c, model, hold)?;
    if let Some(name) = &hold.weapon_sockets.support {
        let original = *bind.get(name).ok_or("Missing support bind")?;
        bind.insert("support.bind".into(), original);
    }
    if let Some(id) = &hold.support_track {
        let track = c
            .definition
            .support_tracks
            .iter()
            .find(|t| t.id == *id)
            .ok_or("Missing support track")?;
        let attachment = c
            .attachments
            .get(&track.weapon_attachment)
            .ok_or("Missing rail weapon")?;
        let source = support_position(track, aim_phase(model, hold, target)?);
        bind.insert(
            hold.weapon_sockets
                .support
                .as_ref()
                .ok_or("Rail hold has no support")?
                .clone(),
            attachment.binding.point(source),
        );
    }
    Ok(bind)
}
fn resolved_weapon_sockets(
    model: &CompiledModel,
    hold: &HoldDefinition,
    bind: &BTreeMap<String, [f32; 2]>,
    driver: Affine2,
    world: &[Affine2],
) -> Result<BTreeMap<String, [f32; 2]>, String> {
    let names = &hold.weapon_sockets;
    let mut sockets = BTreeMap::new();
    for (name, id) in [
        ("grip", Some(names.grip.as_str())),
        ("support", names.support.as_deref()),
        ("stock", names.stock.as_deref()),
        ("muzzle", Some(names.muzzle.as_str())),
        ("barrel_back", Some(names.barrel_back.as_str())),
        ("magazine", names.magazine.as_deref()),
    ] {
        if let Some(id) = id {
            sockets.insert(
                name.into(),
                driver.point(*bind.get(id).ok_or("Missing weapon socket")?),
            );
        }
    }
    if let Some(point) = bind.get("support.bind") {
        sockets.insert("support.bind".into(), driver.point(*point));
    }
    for (id, contact) in [
        ("contact.near", &hold.hand_contacts.near),
        ("contact.far", &hold.hand_contacts.far),
    ] {
        sockets.insert(
            id.into(),
            world[model.bind.indices[&contact.bone]].point(contact.bind_position),
        );
    }
    Ok(sockets)
}
fn hand_at_contact(
    contact: &attachments::ContactDefinition,
    target: [f32; 2],
    rotation: f32,
) -> Affine2 {
    Affine2::translation(sub(target, contact.bind_position))
        .compose(Affine2::around(contact.bind_position, rotation))
}
fn weapon_index(
    c: &CompiledCatalogue,
    model: &CompiledModel,
    hold: &HoldDefinition,
) -> Result<usize, String> {
    let id = if let Some(id) = &hold.weapon_attachment {
        c.attachments
            .get(id)
            .ok_or("Missing weapon attachment")?
            .definition
            .bone
            .as_str()
    } else {
        model
            .definition
            .sockets
            .iter()
            .find(|s| s.id == hold.weapon_sockets.muzzle)
            .ok_or("Missing original muzzle")?
            .bone
            .as_str()
    };
    model
        .bind
        .indices
        .get(id)
        .copied()
        .ok_or_else(|| format!("Unknown weapon bone {id}"))
}
fn hold_pose(
    c: &CompiledCatalogue,
    model: &CompiledModel,
    hold: &HoldDefinition,
    sample: PlayerPoseSample,
    target: [f32; 2],
    world: &mut [Affine2],
    deltas: &BTreeMap<String, ([f32; 2], f32)>,
) -> Result<(), String> {
    let weapon = weapon_index(c, model, hold)?;
    let bind = held_bind(c, model, hold, target)?;
    let names = &hold.weapon_sockets;
    let get = |id: &str| {
        bind.get(id)
            .copied()
            .ok_or_else(|| format!("Missing weapon socket {id}"))
    };
    let (shoulder, shoulder_bone) = model_socket(model, "shoulder.near")?;
    let body_driver = world[shoulder_bone];
    let mut driver = body_driver;
    if let Some((translation, rotation)) = deltas.get(&model.bind.bones[weapon].id) {
        let pivot = match hold.pivot_socket.as_str() {
            "shoulder.near" => shoulder,
            "stock" => get(names.stock.as_deref().ok_or("Stock hold has no stock")?)?,
            _ => get(&names.grip)?,
        };
        driver = driver
            .compose(Affine2::translation(*translation).compose(Affine2::around(pivot, *rotation)));
    }
    if matches!(sample.action, PlayerAction::Hold) {
        let pivot = match hold.pivot_socket.as_str() {
            "shoulder.near" => body_driver.point(shoulder),
            "stock" => driver.point(get(names.stock.as_deref().ok_or("Missing hold stock")?)?),
            _ => driver.point(get(&names.grip)?),
        };
        let axis = sub(
            driver.point(get(&names.muzzle)?),
            driver.point(get(&names.barrel_back)?),
        );
        let axis = mul(axis, 1. / length(axis).max(0.001));
        let relative = sub(driver.point(get(&names.muzzle)?), pivot);
        let lateral = axis[0] * relative[1] - axis[1] * relative[0];
        let distance = length(sub(target, pivot));
        if distance <= lateral.abs() + 0.001 {
            return Err("Aim target is inside the held barrel lateral offset".into());
        }
        let target_bearing = angle(sub(target, pivot));
        let desired = target_bearing - (lateral / distance).asin();
        // Authoring ranges and normalized aim/support tracks describe the
        // target bearing. Off-axis boresight compensation may legitimately
        // rotate the barrel slightly beyond that range to reach the target.
        let degrees = target_bearing.to_degrees();
        if degrees < hold.aim_range[0] - 0.1 || degrees > hold.aim_range[1] + 0.1 {
            return Err(format!(
                "Unsupported {} aim {degrees:.1}° outside authored range",
                hold.weapon_class
            ));
        }
        let current = angle(sub(
            driver.point(get(&names.muzzle)?),
            driver.point(get(&names.barrel_back)?),
        ));
        driver = Affine2::around(pivot, desired - current).compose(driver);
    }
    let muzzle = driver.point(get(&names.muzzle)?);
    let barrel = sub(muzzle, driver.point(get(&names.barrel_back)?));
    let to_target = sub(target, muzzle);
    if barrel[0] * to_target[0] + barrel[1] * to_target[1] <= 0. {
        return Err("Aim target lies behind the held muzzle".into());
    }
    world[weapon] = driver;
    let current_axis = angle(sub(
        driver.point(get(&names.muzzle)?),
        driver.point(get(&names.barrel_back)?),
    ));
    let original_axis = angle(sub(
        model_socket(model, "muzzle")?.0,
        model_socket(model, "barrel_back")?.0,
    ));
    let hand_angle = current_axis - original_axis;
    let near_target = driver.point(get(&names.grip)?);
    let far_target = names
        .support
        .as_ref()
        .map(|id| get(id).map(|p| driver.point(p)))
        .transpose()?;
    let contact_delta = |contact: &attachments::ContactDefinition, target: [f32; 2]| {
        let extra = deltas.get(&contact.bone).map_or(0., |(_, angle)| *angle);
        hand_at_contact(contact, target, hand_angle + extra)
    };
    let near = contact_delta(&hold.hand_contacts.near, near_target);
    solve_chain(model, &hold.hand_contacts.near.bone, near, world)?;
    if let Some(target) = far_target {
        let far = contact_delta(&hold.hand_contacts.far, target);
        solve_chain(model, &hold.hand_contacts.far.bone, far, world)?;
    }
    Ok(())
}
fn layer_action(
    model: &CompiledModel,
    base: &[Affine2],
    deltas: &BTreeMap<String, ([f32; 2], f32)>,
    weapon: usize,
    pivot: [f32; 2],
) -> Result<Vec<Affine2>, String> {
    let mut world = Vec::with_capacity(base.len());
    let mut offsets = Vec::with_capacity(base.len());
    for (index, bone) in model.bind.bones.iter().enumerate() {
        let inherited = bone.parent.map_or(Affine2::IDENTITY, |i| offsets[i]);
        let (translation, rotation) = deltas.get(&bone.id).copied().unwrap_or(([0.; 2], 0.));
        if translation == [0.; 2] && rotation == 0. && inherited == Affine2::IDENTITY {
            world.push(base[index]);
            offsets.push(Affine2::IDENTITY);
            continue;
        }
        let origin = if index == weapon { pivot } else { bone.origin };
        let local = Affine2::translation(translation).compose(Affine2::around(origin, rotation));
        let posed = inherited.compose(base[index]).compose(local);
        offsets.push(posed.compose(base[index].inverse()?));
        world.push(posed);
    }
    Ok(world)
}
fn action_pose(
    c: &CompiledCatalogue,
    model: &CompiledModel,
    hold: &HoldDefinition,
    sample: PlayerPoseSample,
    target: [f32; 2],
    base: &[Affine2],
) -> Result<Vec<Affine2>, String> {
    let (track, phase, name) = match sample.action {
        PlayerAction::Reload { phase } => (&hold.reload_track, phase, "reload"),
        PlayerAction::Melee { phase } => (&hold.melee_track, phase, "melee"),
        PlayerAction::Throw { phase } => (&hold.throw_track, phase, "throw"),
        _ => return Ok(base.to_vec()),
    };
    if track.is_none() {
        return Err(format!(
            "Unsupported {name} pose: author an aimed-baseline offset track"
        ));
    }
    let mut deltas = BTreeMap::new();
    apply_track(c, track, phase, &mut deltas)?;
    if deltas
        .values()
        .all(|(translation, rotation)| *translation == [0.; 2] && *rotation == 0.)
    {
        return Ok(base.to_vec());
    }
    let bind = held_bind(c, model, hold, target)?;
    let pivot = match hold.pivot_socket.as_str() {
        "shoulder.near" => model_socket(model, "shoulder.near")?.0,
        "stock" => bind[hold.weapon_sockets.stock.as_ref().ok_or("Missing stock")?],
        _ => bind[&hold.weapon_sockets.grip],
    };
    let weapon = weapon_index(c, model, hold)?;
    let mut world = layer_action(model, base, &deltas, weapon, pivot)?;
    if matches!(sample.action, PlayerAction::Reload { .. }) {
        let driver = world[weapon];
        let current_axis = angle(sub(
            driver.point(bind[&hold.weapon_sockets.muzzle]),
            driver.point(bind[&hold.weapon_sockets.barrel_back]),
        ));
        let original_axis = angle(sub(
            model_socket(model, "muzzle")?.0,
            model_socket(model, "barrel_back")?.0,
        ));
        let extra = deltas
            .get(&hold.hand_contacts.near.bone)
            .map_or(0., |(_, r)| *r);
        let near = hand_at_contact(
            &hold.hand_contacts.near,
            driver.point(bind[&hold.weapon_sockets.grip]),
            current_axis - original_axis + extra,
        );
        solve_chain(model, &hold.hand_contacts.near.bone, near, &mut world)?;
        let far = world[model.bind.indices[&hold.hand_contacts.far.bone]];
        solve_chain(model, &hold.hand_contacts.far.bone, far, &mut world)?;
        world[weapon] = driver;
    } else {
        let near = world[model.bind.indices[&hold.hand_contacts.near.bone]];
        let far = world[model.bind.indices[&hold.hand_contacts.far.bone]];
        solve_chain(model, &hold.hand_contacts.near.bone, near, &mut world)?;
        solve_chain(model, &hold.hand_contacts.far.bone, far, &mut world)?;
    }
    Ok(world)
}

pub fn resolve_pose(
    c: &CompiledCatalogue,
    appearance: &ResolvedAppearance,
    sample: PlayerPoseSample,
    placement: ModelPlacement,
) -> Result<ResolvedPlayerPose, String> {
    check_sample(sample)?;
    // A completed recovery is the held state, including its original hand cel
    // and firearm. Normalize after validation so out-of-range samples still
    // fail rather than being silently clamped into a legal action.
    let sample = match sample.action {
        PlayerAction::Reload { phase: 1. }
        | PlayerAction::Melee { phase: 1. }
        | PlayerAction::Throw { phase: 1. } => PlayerPoseSample {
            action: PlayerAction::Hold,
            ..sample
        },
        _ => sample,
    };
    let model = c
        .model(&appearance.model_id)
        .ok_or("Unknown appearance model")?;
    let transform = placement.at_ground(model.definition.ground)?;
    let source_target = transform.inverse_point(sample.aim);
    let facing =
        sample.action != PlayerAction::Bind && source_target[0] < model.definition.ground[0];
    let mirror = if facing {
        Affine2::mirror_x(model.definition.ground[0])
    } else {
        Affine2::IDENTITY
    };
    let target = mirror.point(source_target);
    let hold = appearance
        .hold_id
        .as_ref()
        .and_then(|id| c.definition.holds.iter().find(|h| h.id == *id));
    let mut deltas = BTreeMap::new();
    if sample.action != PlayerAction::Bind {
        if let Some(hold) = hold {
            let phase = aim_phase(model, hold, target)?;
            apply_track(c, &hold.aim_track, phase, &mut deltas)?;
            if !sample.reduced_effects {
                apply_track(c, &hold.recoil_track, sample.recoil, &mut deltas)?;
                apply_track(c, &hold.hit_track, sample.hit, &mut deltas)?;
                if sample.recoil > 0. && hold.recoil_track.is_none() {
                    let (shoulder, bone) = model_socket(model, "shoulder.near")?;
                    let direction = sub(target, shoulder);
                    let shift = mul(
                        direction,
                        -hold.recoil_pixels * sample.recoil / length(direction).max(0.001),
                    );
                    let entry = deltas
                        .entry(model.bind.bones[bone].id.clone())
                        .or_insert(([0.; 2], 0.));
                    entry.0 = add(entry.0, shift);
                }
            }
            if sample.brace > 0. {
                if hold.brace_track.is_none() {
                    return Err("Unsupported brace pose: author a source track".into());
                }
                apply_track(c, &hold.brace_track, sample.brace, &mut deltas)?;
            }
        } else if !matches!(sample.action, PlayerAction::Hold) {
            return Err("Unarmed action has no authored source track".into());
        }
    }
    let mut world = source_fk(model, &deltas);
    let mut weapon_sockets = BTreeMap::new();
    if sample.action != PlayerAction::Bind
        && let Some(hold) = hold
    {
        let base_sample = PlayerPoseSample {
            action: PlayerAction::Hold,
            ..sample
        };
        hold_pose(c, model, hold, base_sample, target, &mut world, &deltas)?;
        world = action_pose(c, model, hold, sample, target, &world)?;
        let bind = held_bind(c, model, hold, target)?;
        let weapon = weapon_index(c, model, hold)?;
        weapon_sockets = resolved_weapon_sockets(model, hold, &bind, world[weapon], &world)?;
    }
    let mut source_sockets = model
        .definition
        .sockets
        .iter()
        .map(|s| {
            (
                s.id.clone(),
                world[model.bind.indices[&s.bone]].point(s.bind_position),
            )
        })
        .collect::<BTreeMap<_, _>>();
    source_sockets.extend(weapon_sockets);
    if sample.action == PlayerAction::Bind
        && let Some(hold) = hold
    {
        source_sockets.insert("contact.near".into(), hold.hand_contacts.near.bind_position);
        source_sockets.insert("contact.far".into(), hold.hand_contacts.far.bind_position);
    }
    let actions = selected_actions(sample);
    let hide_gun = matches!(
        sample.action,
        PlayerAction::Melee { .. } | PlayerAction::Throw { .. }
    );
    let selected_originals = appearance
        .part_indices
        .iter()
        .filter_map(|i| {
            let p = &model.parts[*i].definition;
            (!(hide_gun && p.slot == "ranged")).then_some(p.id.as_str())
        })
        .collect::<BTreeSet<_>>();
    // Cel activation sees the selected source owners. Its own required owner
    // may be the one it replaces, so do not recursively reevaluate it after
    // suppression. Repairs then see the actual effective owner set once.
    let mut active_replacements = BTreeSet::new();
    let mut suppressed = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    for id in &appearance.attachment_ids {
        let a = c
            .attachments
            .get(id)
            .ok_or("Missing selected compiled attachment")?;
        let d = &a.definition;
        if d.channel == SourceChannel::SkinReplacement
            && !(hide_gun && d.slot == "ranged")
            && attachment_active(d, &selected_originals, &actions)
        {
            active_replacements.insert(id.as_str());
            suppressed.extend(d.replace_parts.iter().map(String::as_str));
        }
    }
    let originals = selected_originals
        .into_iter()
        .filter(|id| !suppressed.contains(id))
        .collect::<BTreeSet<_>>();
    let mut packets = Vec::new();
    for index in &appearance.part_indices {
        let part = &model.parts[*index];
        let d = &part.definition;
        if !originals.contains(d.id.as_str()) {
            continue;
        }
        packets.push(DrawPacket {
            part_id: d.id.clone(),
            source_id: model.definition.source.id.clone(),
            channel: d.channel,
            slot: d.slot.clone(),
            material: d.material.clone(),
            draw_order: d.draw_order,
            rectangles: part.geometry.clone(),
            transform,
            local_mesh: part.local_mesh.clone(),
            source_to_bind: Affine2::IDENTITY,
            bone_transform: mirror.compose(world[model.bind.indices[&d.bone]]),
        });
    }
    for id in &appearance.attachment_ids {
        let a = c
            .attachments
            .get(id)
            .ok_or("Missing selected compiled attachment")?;
        let d = &a.definition;
        let active = if d.channel == SourceChannel::SkinReplacement {
            active_replacements.contains(id.as_str())
        } else {
            !(hide_gun && d.slot == "ranged") && attachment_active(d, &originals, &actions)
        };
        if !active {
            continue;
        }
        let bone = world[model.bind.indices[&d.bone]];
        packets.push(DrawPacket {
            part_id: d.id.clone(),
            source_id: d.source_id.clone(),
            channel: d.channel,
            slot: d.slot.clone(),
            material: d.material.clone(),
            draw_order: d.draw_order,
            rectangles: a.geometry.clone(),
            transform,
            local_mesh: a.local_mesh.clone(),
            source_to_bind: a.binding,
            bone_transform: mirror.compose(bone),
        });
        for socket in &d.sockets {
            let point = bone.point(a.binding.point(socket.source_position));
            source_sockets.insert(format!("{}::{}", d.id, socket.id), point);
            for alias in &socket.publish_as {
                publish_socket(&mut aliases, alias, point)?;
            }
            if d.slot == "melee"
                && matches!(sample.action, PlayerAction::Melee { .. })
                && socket.id == "melee.grip"
            {
                publish_socket(&mut aliases, "melee.grip", point)?;
                publish_socket(&mut aliases, "contact.near", point)?;
            }
        }
    }
    source_sockets.extend(aliases);
    if matches!(sample.action, PlayerAction::Melee { .. })
        && !source_sockets.contains_key("melee.grip")
    {
        return Err("Unsupported active melee appearance: no real melee grip attachment".into());
    }
    packets.sort_by(|a, b| {
        a.draw_order
            .cmp(&b.draw_order)
            .then(a.part_id.cmp(&b.part_id))
    });
    for point in source_sockets.values_mut() {
        *point = mirror.point(*point);
    }
    let sockets = source_sockets
        .iter()
        .map(|(id, p)| (id.clone(), transform.point(*p)))
        .collect::<BTreeMap<_, _>>();
    Ok(ResolvedPlayerPose {
        model_id: appearance.model_id.clone(),
        skin_id: appearance.skin_id.clone(),
        muzzle: *sockets.get("muzzle").ok_or("Missing resolved muzzle")?,
        throw_origin: *sockets
            .get("throw_origin")
            .ok_or("Missing resolved throw palm")?,
        packets,
        sockets,
        source_sockets,
    })
}
/// The pose of `model_id` in `equipment`: [`resolve_appearance`], then
/// [`resolve_equipped_pose`]. Play resolves its appearance through the
/// presentation cache instead (`visual_assets::resolve_player_model`).
#[cfg(any(test, feature = "review"))]
pub fn resolve_equipped_model(
    c: &CompiledCatalogue,
    model_id: &str,
    equipment: &horde_core::survivors::EquipmentVisual,
    sample: PlayerPoseSample,
    placement: ModelPlacement,
) -> Result<ResolvedPlayerPose, String> {
    let appearance = resolve_appearance(c, model_id, equipment)?;
    resolve_equipped_pose(c, model_id, &appearance, sample, placement)
}

/// The pose of `model_id` in an `appearance` resolved already: the second
/// half of `resolve_equipped_model`.
pub fn resolve_equipped_pose(
    c: &CompiledCatalogue,
    model_id: &str,
    appearance: &ResolvedAppearance,
    sample: PlayerPoseSample,
    placement: ModelPlacement,
) -> Result<ResolvedPlayerPose, String> {
    resolve_pose(c, appearance, sample, placement).map_err(|e| {
        format!(
            "{model_id} {:?} {:?} aim {:?}: {e}",
            appearance
                .frozen_equipment
                .as_ref()
                .and_then(|equipment| equipment.weapon_class),
            sample.action,
            sample.aim
        )
    })
}
