use std::time::Duration;

use super::*;

pub struct KickBombPlugin;

#[derive(Reflect, Component, Clone, Debug)]
#[reflect(Component)]
pub struct IdleKickBomb {
    spawner: Entity,
}

impl Default for IdleKickBomb {
    fn default() -> Self {
        Self {
            spawner: crate::utils::invalid_entity(),
        }
    }
}

#[derive(Reflect, Component, Clone, Debug)]
#[reflect(Component, Default)]
pub struct LitKickBomb {
    spawner: Entity,
    fuse_sound: Handle<AudioInstance>,
    age: f32,
}

impl Default for LitKickBomb {
    fn default() -> Self {
        Self {
            spawner: crate::utils::invalid_entity(),
            fuse_sound: default(),
            age: 0.0,
        }
    }
}

impl Plugin for KickBombPlugin {
    fn build(&self, app: &mut App) {
        app.add_rollback_system(RollbackStage::PreUpdate, pre_update_in_game)
            .add_rollback_system(RollbackStage::Update, update_lit_kick_bombs)
            .add_rollback_system(RollbackStage::Update, update_idle_kick_bombs)
            .extend_rollback_plugin(|plugin| {
                plugin
                    .register_rollback_type::<IdleKickBomb>()
                    .register_rollback_type::<LitKickBomb>()
            });
    }
}

fn pre_update_in_game(
    mut commands: Commands,
    non_hydrated_map_elements: Query<
        (Entity, &Sort, &Handle<MapElementMeta>, &Transform),
        Without<MapElementHydrated>,
    >,
    mut ridp: ResMut<RollbackIdProvider>,
    element_assets: ResMut<Assets<MapElementMeta>>,
) {
    let mut elements = non_hydrated_map_elements.iter().collect::<Vec<_>>();
    elements.sort_by_key(|x| x.1);
    for (entity, _sort, map_element_handle, transform) in elements {
        let map_element = element_assets.get(map_element_handle).unwrap();
        if let BuiltinElementKind::KickBomb {
            body_size,
            body_offset,
            atlas_handle,
            can_rotate,
            bouncyness,
            ..
        } = &map_element.builtin
        {
            commands.entity(entity).insert(MapElementHydrated);

            commands
                .spawn()
                .insert(Rollback::new(ridp.next_id()))
                .insert(Item {
                    script: "core:kick_bomb".into(),
                })
                .insert(IdleKickBomb { spawner: entity })
                .insert(Name::new("Item: Kick Bomb"))
                .insert(AnimatedSprite {
                    start: 0,
                    end: 0,
                    atlas: atlas_handle.inner.clone(),
                    repeat: false,
                    ..default()
                })
                .insert(map_element_handle.clone_weak())
                .insert_bundle(VisibilityBundle::default())
                .insert(MapRespawnPoint(transform.translation))
                .insert_bundle(TransformBundle {
                    local: *transform,
                    ..default()
                })
                .insert(KinematicBody {
                    size: *body_size,
                    offset: *body_offset,
                    gravity: 1.0,
                    has_mass: true,
                    has_friction: true,
                    can_rotate: *can_rotate,
                    bouncyness: *bouncyness,
                    ..default()
                });
        }
    }
}

fn update_idle_kick_bombs(
    mut commands: Commands,
    players: Query<(&AnimatedSprite, &Transform, &KinematicBody), With<PlayerIdx>>,
    mut kick_bombs: Query<
        (
            &Rollback,
            Entity,
            &IdleKickBomb,
            &mut Transform,
            &mut AnimatedSprite,
            &mut KinematicBody,
            &Handle<MapElementMeta>,
            Option<&Parent>,
            Option<&ItemUsed>,
            Option<&ItemDropped>,
        ),
        Without<PlayerIdx>,
    >,
    mut ridp: ResMut<RollbackIdProvider>,
    element_assets: ResMut<Assets<MapElementMeta>>,
    effects: Res<AudioChannel<EffectsChannel>>,
) {
    let mut items = kick_bombs.iter_mut().collect::<Vec<_>>();
    items.sort_by_key(|x| x.0.id());
    for (
        _,
        item_ent,
        kick_bomb,
        mut transform,
        mut sprite,
        mut body,
        meta_handle,
        parent,
        used,
        dropped,
    ) in items
    {
        let meta = element_assets.get(meta_handle).unwrap();
        let BuiltinElementKind::KickBomb {
            grab_offset,
            fuse_sound_handle,
            angular_velocity,
            throw_velocity,
            atlas_handle,
            ..
        } = &meta.builtin else {
            unreachable!();
        };

        // If the item is dropped
        if let Some(dropped) = dropped {
            commands.entity(item_ent).remove::<ItemDropped>();
            let (player_sprite, player_transform, player_body) =
                players.get(dropped.player).expect("Parent is not a player");

            // Re-activate physics
            body.is_deactivated = false;

            sprite.start = 0;
            sprite.end = 0;
            body.velocity = player_body.velocity;
            body.is_spawning = true;

            let horizontal_flip_factor = if player_sprite.flip_x {
                Vec2::new(-1.0, 1.0)
            } else {
                Vec2::ONE
            };

            // Drop item at player position
            transform.translation =
                player_transform.translation + (*grab_offset * horizontal_flip_factor).extend(0.0);
        }

        // If the item is being held
        if let Some(parent) = parent {
            let (player_sprite, player_transform, player_body) =
                players.get(parent.get()).expect("Parent is not player");

            // Deactivate items while held
            body.is_deactivated = true;

            // Flip the sprite to match the player orientation
            let flip = player_sprite.flip_x;
            sprite.flip_x = flip;
            let flip_factor = if flip { -1.0 } else { 1.0 };
            let horizontal_flip_factor = Vec2::new(flip_factor, 1.0);
            transform.translation.x = grab_offset.x * flip_factor;
            transform.translation.y = grab_offset.y;
            transform.translation.z = 0.0;

            // If the item is being used
            if used.is_some() {
                // Despawn the item from the player's hand
                commands.entity(item_ent).despawn();

                body.angular_velocity = *angular_velocity;

                let pos = player_transform.translation
                    + (*grab_offset * horizontal_flip_factor).extend(0.0);
                commands
                    .spawn()
                    .insert(Rollback::new(ridp.next_id()))
                    .insert(Name::new("Lit Kick Bomb"))
                    .insert(MapRespawnPoint(pos))
                    .insert(Transform::from_translation(pos))
                    .insert(GlobalTransform::default())
                    .insert(Visibility::default())
                    .insert(ComputedVisibility::default())
                    .insert(AnimatedSprite {
                        atlas: atlas_handle.inner.clone(),
                        start: 3,
                        end: 5,
                        repeat: true,
                        fps: 8.0,
                        ..default()
                    })
                    .insert(meta_handle.clone_weak())
                    .insert(body.clone())
                    .insert(LitKickBomb {
                        spawner: kick_bomb.spawner,
                        fuse_sound: effects.play(fuse_sound_handle.clone_weak()).handle(),
                        ..default()
                    })
                    .insert(KinematicBody {
                        velocity: *throw_velocity * horizontal_flip_factor + player_body.velocity,
                        is_deactivated: false,
                        ..body.clone()
                    });
            }
        }
    }
}

fn update_lit_kick_bombs(
    mut commands: Commands,
    players: Query<(&AnimatedSprite, &Transform, &KinematicBody), With<PlayerIdx>>,
    mut kick_bombs: Query<
        (
            &Rollback,
            Entity,
            &mut LitKickBomb,
            &mut Transform,
            &GlobalTransform,
            &mut KinematicBody,
            &mut AnimatedSprite,
            &Handle<MapElementMeta>,
            Option<&Parent>,
            Option<&ItemDropped>,
        ),
        Without<PlayerIdx>,
    >,
    mut ridp: ResMut<RollbackIdProvider>,
    element_assets: ResMut<Assets<MapElementMeta>>,
    player_inputs: Res<PlayerInputs>,
    effects: Res<AudioChannel<EffectsChannel>>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    collision_world: CollisionWorld,
) {
    let mut items = kick_bombs.iter_mut().collect::<Vec<_>>();
    items.sort_by_key(|x| x.0.id());
    for (
        _,
        item_ent,
        mut kick_bomb,
        mut transform,
        global_transform,
        mut body,
        mut sprite,
        meta_handle,
        parent,
        _,
    ) in items
    {
        let meta = element_assets.get(meta_handle).unwrap();
        let BuiltinElementKind::KickBomb {
            fuse_time,
            damage_region_size,
            damage_region_lifetime,
            explosion_atlas_handle,
            explosion_lifetime,
            explosion_fps,
            explosion_frames,
            explosion_sound_handle,
            grab_offset,
            throw_velocity,
            arm_delay,
            ..
        } = &meta.builtin else {
            unreachable!();
        };

        kick_bomb.age += 1.0 / crate::FPS as f32;

        if let Some(parent) = parent {
            let (player_sprite, ..) = players.get(parent.get()).expect("Parent is not player");

            // Deactivate items while held
            body.is_deactivated = true;

            // Flip the sprite to match the player orientation
            let flip = player_sprite.flip_x;
            sprite.flip_x = flip;
            let flip_factor = if flip { -1.0 } else { 1.0 };
            transform.translation.x = grab_offset.x * flip_factor;
            transform.translation.y = grab_offset.y;
            transform.translation.z = 0.0;
        }

        let mut should_explode = false;
        if parent.is_none() {
            if let Some(entity) = collision_world
                .actor_collisions(item_ent)
                .into_iter()
                .find(|&x| players.contains(x))
            {
                let (player_sprite, player_tansform, _) = players.get(entity).unwrap();
                let player_standing_left = player_tansform.translation.x <= transform.translation.x;
                if body.velocity.x == 0.0 {
                    body.velocity = *throw_velocity;
                    if player_sprite.flip_x {
                        body.velocity.x *= -1.0;
                    }
                } else if player_standing_left && !player_sprite.flip_x {
                    body.velocity.x = throw_velocity.x;
                    body.velocity.y = throw_velocity.y;
                } else if !player_standing_left && player_sprite.flip_x {
                    body.velocity.x = -throw_velocity.x;
                    body.velocity.y = throw_velocity.y;
                } else if kick_bomb.age >= *arm_delay {
                    should_explode = true;
                }
            }
        }

        if kick_bomb.age >= *fuse_time || should_explode {
            if player_inputs.is_confirmed {
                effects.play(explosion_sound_handle.clone_weak());
                audio_instances
                    .get_mut(&kick_bomb.fuse_sound)
                    .map(|x| x.stop(AudioTween::linear(Duration::from_secs_f32(0.1))));
            }

            commands.entity(item_ent).despawn();
            // Cause the item to re-spawn by re-triggering spawner hydration
            commands
                .entity(kick_bomb.spawner)
                .remove::<MapElementHydrated>();

            // Spawn the damage region entity
            let mut spawn_transform = global_transform.compute_transform();
            spawn_transform.rotation = Quat::IDENTITY;

            commands
                .spawn()
                .insert(Rollback::new(ridp.next_id()))
                .insert(spawn_transform)
                .insert(GlobalTransform::default())
                .insert(Visibility::default())
                .insert(ComputedVisibility::default())
                .insert(DamageRegion {
                    size: *damage_region_size,
                })
                .insert(Lifetime::new(*damage_region_lifetime));
            // Spawn the explosion sprite entity
            commands
                .spawn()
                .insert(Rollback::new(ridp.next_id()))
                .insert(spawn_transform)
                .insert(GlobalTransform::default())
                .insert(Visibility::default())
                .insert(ComputedVisibility::default())
                .insert(AnimatedSprite {
                    start: 0,
                    end: *explosion_frames,
                    atlas: explosion_atlas_handle.inner.clone(),
                    repeat: false,
                    fps: *explosion_fps,
                    ..default()
                })
                .insert(Lifetime::new(*explosion_lifetime));
        }
    }
}