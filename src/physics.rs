use bevy::{math::vec2, time::FixedTimestep};

use crate::{metadata::GameMeta, prelude::*};

use self::collisions::{Actor, Collider, CollisionWorld, TileCollision};

pub mod collisions;
// mod debug;

pub struct PhysicsPlugin;

#[derive(StageLabel)]
pub enum GamePhysicsStages {
    Hydrate,
    UpdatePhysics,
}

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<KinematicBody>()
            .add_stage_after(
                CoreStage::PostUpdate,
                GamePhysicsStages::Hydrate,
                SystemStage::parallel().with_system(
                    hydrate_physics_bodies
                        .run_in_state(GameState::InGame)
                        .run_not_in_state(InGameState::Paused),
                ),
            )
            .add_stage_after(
                GamePhysicsStages::Hydrate,
                GamePhysicsStages::UpdatePhysics,
                SystemStage::parallel()
                    .with_run_criteria(FixedTimestep::step(0.016))
                    .with_system(
                        update_kinematic_bodies
                            .run_in_state(GameState::InGame)
                            .run_not_in_state(InGameState::Paused),
                    ),
            );
    }
}

/// A kinematic physics body
///
/// Used primarily for players and things that need to walk around, detect what kind of platform
/// they are standing on, etc.
///
/// For now, all kinematic bodies have axis-aligned, rectangular colliders. This may or may not change in the future.
#[derive(Reflect, Component, Default, Debug, Clone)]
#[reflect(Component, Default)]
pub struct KinematicBody {
    pub velocity: Vec2,
    pub offset: Vec2,
    pub size: Vec2,
    /// Angular velocity in degrees per second
    pub angular_velocity: f32,
    pub is_on_ground: bool,
    pub was_on_ground: bool,
    /// Will be `true` if the body is currently on top of a platform/jumpthrough tile
    pub is_on_platform: bool,
    /// If this is `true` the body will be affected by gravity
    pub has_mass: bool,
    pub has_friction: bool,
    pub can_rotate: bool,
    pub bouncyness: f32,
    pub is_deactivated: bool,
    pub gravity: f32,
}

#[derive(Component)]
struct KinematicBodyCollider;

fn hydrate_physics_bodies(
    mut commands: Commands,
    bodies: Query<(Entity, &Transform, &KinematicBody), Without<Collider>>,
) {
    for (entity, transform, body) in &bodies {
        if body.size.x.round() as i32 % 2 != 0 || body.size.y.round() as i32 % 2 != 0 {
            warn!(
                "TODO: Non-even widths and heights for colliders may currently \
                behave incorrectly, getting stuck in walls."
            );
        }
        commands
            .entity(entity)
            .insert(Collider {
                pos: transform.translation.truncate() + body.offset,
                width: body.size.x,
                height: body.size.y,
                ..default()
            })
            .insert(Actor);
    }
}

fn update_kinematic_bodies(
    game: Res<GameMeta>,
    mut collision_world: CollisionWorld,
    mut bodies: Query<(Entity, &mut KinematicBody, &mut Transform)>,
    time: Res<Time>,
) {
    let dt = time.delta_seconds();
    for (actor, mut body, mut transform) in &mut bodies {
        collision_world.set_actor_position(actor, transform.translation.truncate() + body.offset);

        if !body.is_deactivated {
            let position = collision_world.actor_pos(actor);

            {
                let position = position + vec2(0.0, -1.0);

                body.was_on_ground = body.is_on_ground;

                body.is_on_ground = collision_world.collide_check(actor, position);

                // FIXME: Using this to set `is_on_ground` caused weird glitching behavior when
                // jumping up through platforms
                let tile = collision_world.collide_solids(position, body.size.x, body.size.y);

                body.is_on_platform = tile == TileCollision::JumpThrough;
            }

            if !collision_world.move_h(actor, body.velocity.x) {
                body.velocity.x *= -body.bouncyness;
            }

            if !collision_world.move_v(actor, body.velocity.y) {
                body.velocity.y *= -body.bouncyness;
            }

            if !body.is_on_ground && body.has_mass {
                body.velocity.y -= body.gravity;

                if body.velocity.y < -game.physics.terminal_velocity {
                    body.velocity.y = -game.physics.terminal_velocity;
                }
            }

            if body.can_rotate {
                apply_rotation(
                    &mut transform,
                    body.velocity,
                    body.angular_velocity,
                    body.is_on_ground,
                    dt,
                );
            }

            if body.is_on_ground && body.has_friction {
                body.velocity.x *= game.physics.friction_lerp;
                if body.velocity.x.abs() <= game.physics.stop_threshold {
                    body.velocity.x = 0.0;
                }
            }

            transform.translation =
                (collision_world.actor_pos(actor) - body.offset).extend(transform.translation.z);
        }
    }
}

fn apply_rotation(
    transform: &mut Transform,
    velocity: Vec2,
    angular_velocity: f32,
    is_on_ground: bool,
    dt: f32,
) {
    let mut angle = transform.rotation.to_euler(EulerRot::XYZ).2;
    if angular_velocity != 0.0 {
        angle += (angular_velocity * dt).to_radians();
    } else if !is_on_ground {
        angle += velocity.x.abs() * 0.00045 + velocity.y.abs() * 0.00015;
    } else {
        angle %= std::f32::consts::PI * 2.0;

        let goal = std::f32::consts::PI * 2.0;

        let rest = goal - angle;
        if rest.abs() >= 0.1 {
            angle += (rest * 0.1).max(0.1);
        }
    }

    transform.rotation = Quat::from_rotation_z(angle);
}
