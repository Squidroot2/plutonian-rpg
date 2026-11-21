use crate::controls::{Control, Keybinds};
use crate::{GameState, player::Player, quest::Dialogue, quest_ui::QuestLog};
use avian3d::prelude::*;
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    post_process::bloom::Bloom,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

pub struct FollowCameraPlugin;
#[derive(Resource)]
pub struct Orbit {
    pub yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) distance: f32,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.22,
            distance: 5.5,
        }
    }
}
#[derive(Component)]
pub struct FollowCamera;
impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Orbit>()
            .add_systems(Startup, setup)
            .add_systems(
                PreUpdate,
                look.after(crate::ui::pause)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, cursor)
            .add_systems(
                PostUpdate,
                follow
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
fn setup(mut commands: Commands) {
    commands.spawn((
        FollowCamera,
        Camera3d::default(),
        Transform::from_xyz(0.0, 2.5, 10.5).looking_at(Vec3::ZERO, Vec3::Y),
        Bloom {
            intensity: 0.12,
            ..Bloom::NATURAL
        },
        DistanceFog {
            color: Color::srgb(0.025, 0.055, 0.085),
            falloff: FogFalloff::Linear {
                start: 35.0,
                end: 160.0,
            },
            ..default()
        },
    ));
}
fn look(
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    dialogue: Res<Dialogue>,
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keybinds>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut orbit: ResMut<Orbit>,
    log: Option<Res<QuestLog>>,
) {
    if dialogue.text.is_some()
        || log.is_some_and(|log| log.is_open())
        || bindings.just_pressed(Control::Inventory, &keys)
        || keys.just_pressed(KeyCode::Escape)
        || !windows.iter().any(|w| w.focused)
    {
        return;
    }
    orbit.yaw -= motion.delta.x * 0.003;
    orbit.pitch = (orbit.pitch + motion.delta.y * 0.003).clamp(-0.15, 1.15);
    orbit.distance = (orbit.distance - scroll.delta.y * 0.4).clamp(2.0, 8.0);
}
fn cursor(
    state: Res<State<GameState>>,
    dialogue: Res<Dialogue>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    log: Option<Res<QuestLog>>,
) {
    for (window, mut options) in &mut windows {
        let grab = *state.get() == GameState::Playing
            && dialogue.text.is_none()
            && window.focused
            && !log.as_ref().is_some_and(|log| log.is_open());
        let mode = if grab {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
        if options.grab_mode != mode {
            options.grab_mode = mode;
        }
        options.visible = !grab;
    }
}
pub(crate) fn follow(
    orbit: Res<Orbit>,
    spatial: SpatialQuery,
    player: Query<(Entity, &Transform), With<Player>>,
    mut camera: Query<&mut Transform, (With<FollowCamera>, Without<Player>)>,
) {
    let Ok((entity, player)) = player.single() else {
        return;
    };
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    let pivot = player.translation + Vec3::Y * 0.65;
    let offset = Vec3::new(
        orbit.yaw.sin() * orbit.pitch.cos(),
        orbit.pitch.sin(),
        orbit.yaw.cos() * orbit.pitch.cos(),
    );
    let direction = Dir3::new(offset)
        .expect("finite orbit yaw and pitch must produce a nonzero camera direction");
    let hit = spatial.cast_shape(
        &Collider::sphere(0.18),
        pivot,
        Quat::IDENTITY,
        direction,
        &ShapeCastConfig::from_max_distance(orbit.distance),
        &SpatialQueryFilter::from_excluded_entities([entity]),
    );
    let distance = hit.map_or(orbit.distance, |h| {
        (h.distance - 0.1).clamp(0.25, orbit.distance)
    });
    *camera = Transform::from_translation(pivot + offset * distance).looking_at(pivot, Vec3::Y);
}
