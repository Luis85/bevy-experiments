//! Exact-principal operator controls. This is a trusted host API, never a wire
//! message or a public identity service. Bearer capabilities stay private.
use super::*;

const MAX_OPERATOR_AUDIT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAction {
    Provision,
    Suspend,
    Allow,
    RotateCapability,
}

/// A candidate registration has no external effect until its owner installs it
/// or commits it together with private host control state.
pub(super) struct PreparedPrincipal {
    pub(super) sandbox: SandboxState,
    pub(super) player_id: u64,
    pub(super) next_actor_id: u64,
    pub(super) spawn: GeoPosition,
    pub(super) token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorAuditEntry {
    pub revision: u64,
    pub player_id: u64,
    pub action: OperatorAction,
    pub unix_seconds: f64,
}

/// Safe inspection data for a trusted administrative interface. This type is
/// deliberately separate from private session capabilities and game inventories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalStatus {
    pub player_id: u64,
    pub connected: bool,
    pub dormant: bool,
    pub suspended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorReceipt {
    pub player_id: u64,
    pub revision: u64,
    /// None means this is an in-memory host: its owner must explicitly save.
    pub committed_generation: Option<u64>,
    pub disconnected_peers: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperatorState {
    pub(super) suspended: BTreeSet<u64>,
    revision: u64,
    audit: VecDeque<OperatorAuditEntry>,
}

impl OperatorState {
    pub(super) fn validate(&self, world: &SandboxState) -> Result<()> {
        if self.suspended.len() > terra_sandbox::MAX_PLAYERS
            || self.audit.len() > MAX_OPERATOR_AUDIT
            || self
                .suspended
                .iter()
                .any(|id| !world.players.contains_key(id) || !world.is_player_dormant(*id))
        {
            return Err(NetError::Protocol(
                "invalid operator principal access state".into(),
            ));
        }
        let mut previous = 0;
        for entry in &self.audit {
            if entry.revision == 0
                || entry.revision <= previous
                || entry.revision > self.revision
                || !world.players.contains_key(&entry.player_id)
                || !entry.unix_seconds.is_finite()
                || entry.unix_seconds.abs() > 1e12
            {
                return Err(NetError::Protocol("invalid operator audit record".into()));
            }
            previous = entry.revision;
        }
        if self
            .audit
            .back()
            .map_or(self.revision != 0, |entry| entry.revision != self.revision)
        {
            return Err(NetError::Protocol("operator audit revision differs".into()));
        }
        Ok(())
    }

    fn record(&mut self, player_id: u64, action: OperatorAction, unix_seconds: f64) -> Result<()> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| NetError::Protocol("operator revision exhausted".into()))?;
        self.audit.push_back(OperatorAuditEntry {
            revision: self.revision,
            player_id,
            action,
            unix_seconds,
        });
        if self.audit.len() > MAX_OPERATOR_AUDIT {
            self.audit.pop_front();
        }
        Ok(())
    }
}

impl Server {
    /// Provision one dormant principal through the trusted embedding host. This
    /// works with `allow_new_players = false`; no guest listener is opened. The
    /// game's external account/enrollment service must authorize this call and
    /// privately deliver the returned capability to its intended account.
    ///
    /// With regional storage, principal, starter policy, audit and capability
    /// commit atomically before return. Otherwise the receipt has no generation
    /// and the embedding host must save before delivering the identity.
    pub fn provision_principal(&mut self) -> Result<(OperatorReceipt, crate::SessionIdentity)> {
        self.provision_principal_for(self.local_addr()?)
    }

    /// Explicit client-facing endpoint for a wildcard listener or mapped port.
    /// This maintenance API may block on one bounded storage transaction; call
    /// it outside fixed updates and only after outstanding storage completes.
    pub fn provision_principal_for(
        &mut self,
        server: SocketAddr,
    ) -> Result<(OperatorReceipt, crate::SessionIdentity)> {
        self.require_region_idle()?;
        validate_identity_endpoint(server)?;
        let prepared = self.prepare_principal(self.sandbox.players.len() as u64)?;
        let mut candidate = prepared.sandbox;
        candidate.set_player_dormant(prepared.player_id, true)?;
        let mut control = self.saved_control();
        control
            .sessions
            .insert(prepared.token.clone(), prepared.player_id);
        control.operator.record(
            prepared.player_id,
            OperatorAction::Provision,
            self.clock.unix_seconds,
        )?;
        control.operator.validate(&candidate)?;
        candidate.validate_with_catalog(&self.catalog)?;
        let generation = if let Some(store) = &mut self.region_store {
            let metadata = serde_json::to_value(&control)?;
            let requested = self
                .source_regions
                .values()
                .flat_map(|keys| keys.iter().copied())
                .collect();
            Some(
                store
                    .with_store(|store| store.checkpoint(&mut candidate, metadata, &requested))
                    .map_err(persistence_error)?
                    .generation,
            )
        } else {
            None
        };
        // No IDs, policy, peer state or inventories have changed before this
        // point. A failed commit leaves the same authority and cached partition.
        self.sandbox = candidate;
        self.sessions = control.sessions;
        self.operator = control.operator;
        self.next_actor_id = prepared.next_actor_id;
        if generation.is_some() {
            self.region_checkpoint_pending = false;
            self.region_hydrated_for_input = false;
            self.region_source_activation = false;
            self.region_hydration_failed = false;
            self.region_last_error = None;
        }
        Ok((
            OperatorReceipt {
                player_id: prepared.player_id,
                revision: self.operator.revision,
                committed_generation: generation,
                disconnected_peers: 0,
            },
            crate::SessionIdentity {
                server,
                resume_token: prepared.token,
            },
        ))
    }

    pub(super) fn prepare_principal(&self, spawn_ordinal: u64) -> Result<PreparedPrincipal> {
        // Placement observes dormant bodies and structures as well as resident
        // ones, without mutating live residency before the atomic commit.
        let complete;
        let base = if self.sandbox.is_partial() {
            complete = self
                .region_store
                .as_ref()
                .ok_or_else(|| {
                    NetError::Configuration("partial world has no regional store".into())
                })?
                .export(&self.sandbox)
                .map_err(persistence_error)?;
            &complete
        } else {
            &self.sandbox
        };
        if base.players.len() >= terra_sandbox::MAX_PLAYERS
            || (base.living.enabled
                && base
                    .living_unit_count()
                    .saturating_add(terra_sandbox::STARTER_UNIT_COUNT)
                    > terra_sandbox::MAX_LIVING_UNITS)
        {
            return Err(NetError::Configuration(
                "saved principal or living-body capacity reached".into(),
            ));
        }
        let mut player_id = self.next_actor_id;
        while self.actors.contains_key(&player_id) || base.players.contains_key(&player_id) {
            player_id = player_id
                .checked_add(1)
                .ok_or_else(|| NetError::Protocol("actor IDs exhausted".into()))?;
        }
        let next_actor_id = player_id
            .checked_add(1)
            .ok_or_else(|| NetError::Protocol("actor IDs exhausted".into()))?;
        // Search complete crew placements transactionally. An available avatar
        // point alone cannot authorize an unsupported or overlapping rover.
        let frame = LocalFrame::new(self.config.spawn);
        let offset_angle = (spawn_ordinal % 12) as f64 * std::f64::consts::TAU / 12.;
        let obstacles: Vec<_> = base
            .structures
            .values()
            .map(|s| (s.position, s.radius_m))
            .chain(base.living.facility_footprints())
            .collect();
        for radius in [0., 18., 36., 60., 90., 125., 165., 210.] {
            for n in 0..if radius == 0. { 1 } else { 16 } {
                let angle = offset_angle + n as f64 * std::f64::consts::TAU / 16.;
                let raw = if radius == 0. {
                    self.config.spawn
                } else {
                    frame
                        .to_geo([radius * angle.cos(), 0., radius * angle.sin()])
                        .map_err(|e| NetError::Configuration(e.to_string()))?
                };
                let Some(spawn) = self
                    .environment
                    .resolve_movement(ActorKind::Human, raw, raw)
                    .filter(|p| validate_position(*p) && self.environment.can_enter(*p))
                else {
                    continue;
                };
                if base
                    .players
                    .values()
                    .any(|p| p.position.surface_distance_m(spawn) < 1.2)
                    || !self.environment.allows_placement(spawn, 0.4, true)
                {
                    continue;
                }
                let mut sandbox = base.clone();
                sandbox.add_player_with_inventory(
                    player_id,
                    spawn,
                    self.starting_inventory.clone(),
                    &self.catalog,
                )?;
                sandbox.ensure_living_player(player_id)?;
                sandbox.ensure_transport_demo(&self.catalog)?;
                let crew: Vec<_> = sandbox
                    .living
                    .units
                    .values()
                    .filter(|u| u.owner == Some(player_id))
                    .map(|u| u.id)
                    .collect();
                let mut supported = true;
                for id in &crew {
                    let unit = &sandbox.living.units[id];
                    let grounded = if unit.kind == ActorKind::Vehicle {
                        let Some(vehicle) = sandbox.transport.vehicles.get(id) else {
                            supported = false;
                            break;
                        };
                        let config =
                            sandbox
                                .transport
                                .vehicle_dynamics_config(*id)
                                .ok_or_else(|| {
                                    NetError::Configuration("invalid starter vehicle".into())
                                })?;
                        if terra_core::sample_vehicle_ground(
                            unit.position,
                            f64::from(unit.heading_deg),
                            config.wheelbase_m,
                            vehicle.model.dimensions_m()[0] * 0.86,
                            |p| self.environment.vehicle_ground_sample(p),
                        )
                        .is_err()
                        {
                            supported = false;
                            break;
                        }
                        resolve_vehicle_motion(
                            self.environment.as_ref(),
                            unit.position,
                            unit.position,
                            unit.heading_deg,
                            unit.heading_deg,
                            vehicle.model.dimensions_m(),
                            &obstacles,
                        )
                    } else {
                        self.environment
                            .resolve_movement(unit.kind, unit.position, unit.position)
                            .filter(|p| validate_position(*p) && self.environment.can_enter(*p))
                            .filter(|p| {
                                is_airborne(&unit.kind)
                                    || self.environment.allows_placement(*p, 0.4, true)
                            })
                            .filter(|p| {
                                terra_sandbox::facility_path_clear(unit.kind, *p, *p, &obstacles)
                            })
                    };
                    let Some(grounded) = grounded else {
                        supported = false;
                        break;
                    };
                    let body = sandbox.living.units.get_mut(id).unwrap();
                    body.position = grounded;
                    body.home = grounded;
                }
                if !supported {
                    continue;
                }
                if crew.iter().any(|id| {
                    sandbox
                        .spawn_obstruction(&sandbox.living.units[id])
                        .map_or(true, |other| other.is_some())
                }) {
                    continue;
                }
                let spawn = sandbox
                    .living
                    .active_unit(player_id)
                    .map_or(spawn, |u| u.position);
                sandbox.players.get_mut(&player_id).unwrap().position = spawn;
                sandbox.validate_with_catalog(&self.catalog)?;
                if self.sandbox.is_partial() {
                    // Keep the store's exact prior residency cursor. The full
                    // candidate above was inspection evidence, not permission
                    // to replace a partial working set with a complete export.
                    let mut resident = self.sandbox.clone();
                    resident.add_player_with_inventory(
                        player_id,
                        spawn,
                        self.starting_inventory.clone(),
                        &self.catalog,
                    )?;
                    resident.ensure_living_player(player_id)?;
                    for id in &crew {
                        resident
                            .living
                            .units
                            .insert(*id, sandbox.living.units[id].clone());
                    }
                    resident.transport = sandbox.transport;
                    resident.validate_with_catalog(&self.catalog)?;
                    sandbox = resident;
                }
                return Ok(PreparedPrincipal {
                    sandbox,
                    player_id,
                    next_actor_id,
                    spawn,
                    token: self.unused_resume_capability()?,
                });
            }
        }
        Err(NetError::Configuration("no fully supported, unoccupied crew placement is available near spawn; load terrain or clear the arrival area".into()))
    }

    fn unused_resume_capability(&self) -> Result<String> {
        (0..4)
            .map(|_| fresh_token())
            .find_map(|result| match result {
                Ok(token) if !self.sessions.contains_key(&token) => Some(Ok(token)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .ok_or_else(|| NetError::Protocol("capability collision budget exhausted".into()))?
    }

    pub fn principal_status(&self, player_id: u64) -> Option<PrincipalStatus> {
        self.sandbox
            .players
            .contains_key(&player_id)
            .then(|| PrincipalStatus {
                player_id,
                connected: self
                    .peers
                    .values()
                    .any(|peer| peer.actor_id == Some(player_id)),
                dormant: self.sandbox.is_player_dormant(player_id),
                suspended: self.operator.suspended.contains(&player_id),
            })
    }

    /// The most recent 128 operator actions, excluding all secret capabilities.
    /// Authentication/authorization of the caller belongs to the embedding host.
    pub fn operator_audit(&self) -> impl Iterator<Item = &OperatorAuditEntry> {
        self.operator.audit.iter()
    }

    /// Suspend or allow an exact saved principal. Suspension disconnects current
    /// sessions only after durable commit, freezes ownership and blocks Hello
    /// before cold hydration. Allow keeps the player dormant until valid resume.
    /// Failed storage leaves policy, peers, state and generation unchanged.
    ///
    /// This explicit maintenance call can block on one bounded checkpoint; do
    /// not invoke it from a fixed update. It rejects an already-busy worker.
    pub fn set_principal_suspended(
        &mut self,
        player_id: u64,
        suspended: bool,
    ) -> Result<OperatorReceipt> {
        self.apply_operator_action(
            player_id,
            if suspended {
                OperatorAction::Suspend
            } else {
                OperatorAction::Allow
            },
            None,
        )
    }

    /// Replace one saved principal's bearer capability and disconnect its active
    /// session after commit. The old capability never becomes valid again. This
    /// does not establish who a real person is: perform identity recovery in the
    /// game's account service before calling this trusted API.
    ///
    /// The returned identity has no Debug implementation. Deliver/store it using
    /// the host's authorized private recovery channel; never put it in telemetry.
    pub fn rotate_resume_capability(
        &mut self,
        player_id: u64,
    ) -> Result<(OperatorReceipt, crate::SessionIdentity)> {
        self.rotate_resume_capability_for(player_id, self.local_addr()?)
    }

    /// Explicit client-facing address for a wildcard listener or an externally
    /// mapped port. The trusted host owns this endpoint choice; a listen address
    /// such as 0.0.0.0 cannot be a usable pinned recovery identity.
    pub fn rotate_resume_capability_for(
        &mut self,
        player_id: u64,
        server: SocketAddr,
    ) -> Result<(OperatorReceipt, crate::SessionIdentity)> {
        self.require_region_idle()?;
        validate_identity_endpoint(server)?;
        let token = self.unused_resume_capability()?;
        let receipt = self.apply_operator_action(
            player_id,
            OperatorAction::RotateCapability,
            Some(token.clone()),
        )?;
        Ok((
            receipt,
            crate::SessionIdentity {
                server,
                resume_token: token,
            },
        ))
    }

    fn apply_operator_action(
        &mut self,
        player_id: u64,
        action: OperatorAction,
        replacement: Option<String>,
    ) -> Result<OperatorReceipt> {
        self.require_region_idle()?;
        if !self.sandbox.players.contains_key(&player_id) {
            return Err(NetError::Configuration(
                "operator target principal does not exist".into(),
            ));
        }
        let disconnect = action != OperatorAction::Allow;
        let mut candidate = self.sandbox.clone();
        if disconnect {
            candidate.set_player_dormant(player_id, true)?;
        }
        let mut control = self.saved_control();
        match action {
            OperatorAction::Provision => {
                return Err(NetError::Configuration(
                    "use provision_principal for a new principal".into(),
                ));
            }
            OperatorAction::Suspend => {
                control.operator.suspended.insert(player_id);
            }
            OperatorAction::Allow => {
                control.operator.suspended.remove(&player_id);
            }
            OperatorAction::RotateCapability => {
                let token = replacement
                    .ok_or_else(|| NetError::Protocol("replacement capability absent".into()))?;
                control.sessions.retain(|_, owner| *owner != player_id);
                control.sessions.insert(token, player_id);
            }
        }
        control
            .operator
            .record(player_id, action, self.clock.unix_seconds)?;
        control.operator.validate(&candidate)?;
        candidate.validate_with_catalog(&self.catalog)?;
        let generation = if let Some(store) = &mut self.region_store {
            let metadata = serde_json::to_value(&control)?;
            let requested = self
                .source_regions
                .values()
                .flat_map(|keys| keys.iter().copied())
                .collect();
            Some(
                store
                    .with_store(|store| store.checkpoint(&mut candidate, metadata, &requested))
                    .map_err(persistence_error)?
                    .generation,
            )
        } else {
            None
        };
        self.sandbox = candidate;
        self.operator = control.operator;
        self.sessions = control.sessions;
        let revision = self.operator.revision;
        let mut disconnected = 0;
        if disconnect {
            self.peers.retain(|_, peer| {
                let retain = peer.actor_id != Some(player_id);
                disconnected += usize::from(!retain);
                retain
            });
            self.closing_peers
                .retain(|peer| peer.actor_id != Some(player_id));
            self.actors.remove(&player_id);
        }
        if generation.is_some() {
            self.region_checkpoint_pending = false;
            self.region_hydrated_for_input = false;
            self.region_source_activation = false;
            self.region_hydration_failed = false;
            self.region_last_error = None;
        }
        Ok(OperatorReceipt {
            player_id,
            revision,
            committed_generation: generation,
            disconnected_peers: disconnected,
        })
    }
}

fn validate_identity_endpoint(server: SocketAddr) -> Result<()> {
    if server.port() == 0 || server.ip().is_unspecified() || server.ip().is_multicast() {
        return Err(NetError::Configuration(
            "principal capability requires an explicit client-facing unicast server address; wildcard listeners must use the _for method".into(),
        ));
    }
    Ok(())
}
