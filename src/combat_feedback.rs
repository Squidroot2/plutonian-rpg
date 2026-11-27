use crate::{
    GameState,
    camera::FollowCamera,
    combat::{CombatSystems, ConfirmedHit, Dead},
    player::Player,
};
use bevy::{
    camera::visibility::NoFrustumCulling,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
    ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialPlugin},
};

const IMPACT_SECONDS: f32 = 0.18;
const PULSE_SECONDS: f32 = 0.24;

#[derive(Asset, AsBindGroup, TypePath, Clone)]
struct ImpactMaterial {
    /// Normalized lifetime of the impact effect, from `0.0` when spawned to `1.0` when expired.
    ///
    /// The impact shader uses this value to animate the effect; `age_effects` advances it
    /// according to `IMPACT_SECONDS` and despawns the impact when it reaches `1.0`.
    #[uniform(0)]
    age: f32,
}

impl Material for ImpactMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/attack_impact.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_shadows() -> bool {
        false
    }

    fn enable_prepass() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone)]
struct BiteMaterial {
    /// Normalized lifetime of the bite pulse, from `0.0` when triggered to `1.0` when finished.
    ///
    /// The pulse shader uses this value to animate the overlay; `age_effects` advances it
    /// according to `PULSE_SECONDS` and hides the overlay when it reaches `1.0`.
    #[uniform(0)]
    age: f32,
}

impl UiMaterial for BiteMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/bite_pulse.wesl".into()
    }
}

#[derive(Component)]
pub(crate) struct AttackImpact;

#[derive(Component)]
pub(crate) struct BitePulse;

#[derive(Component)]
pub(crate) struct DeathAnimation {
    pub(crate) players: Vec<Entity>,
    pub(crate) clip: AnimationNodeIndex,
}

#[derive(Resource)]
struct ImpactMesh(Handle<Mesh>);

pub(crate) struct CombatFeedbackPlugin;

impl Plugin for CombatFeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<ImpactMaterial>::default(),
            UiMaterialPlugin::<BiteMaterial>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                age_effects.run_if(in_state(GameState::Playing)),
                show_feedback,
            )
                .chain()
                .after(CombatSystems::Resolve),
        )
        .add_systems(
            PostUpdate,
            face_camera
                .after(crate::camera::follow)
                .before(TransformSystems::Propagate)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(
            Update,
            play_death_animations
                .after(CombatSystems::Resolve)
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(OnExit(GameState::Playing), clear_effects);
    }
}

fn play_death_animations(
    dead: Query<&DeathAnimation, (With<Dead>, Or<(Added<Dead>, Added<DeathAnimation>)>)>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for death in &dead {
        for &entity in &death.players {
            if let Ok((mut player, mut transitions)) = players.get_mut(entity) {
                // An actor can have several active layers during a weapon attack.
                player.stop_all();
                *transitions = AnimationTransitions::new();
                transitions.play(&mut player, death.clip, std::time::Duration::ZERO);
            }
        }
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut impacts: ResMut<Assets<ImpactMaterial>>,
    mut pulses: ResMut<Assets<BiteMaterial>>,
) {
    let quad = meshes.add(Rectangle::new(0.92, 0.92));
    commands.insert_resource(ImpactMesh(quad.clone()));
    // Let the renderer specialize the impact pipeline before the first attack.
    commands.spawn((
        Mesh3d(quad),
        MeshMaterial3d(impacts.add(ImpactMaterial { age: 1.0 })),
        NoFrustumCulling,
        DespawnOnExit(GameState::Loading),
    ));
    commands.spawn((
        Name::new("Bite damage pulse"),
        BitePulse,
        MaterialNode(pulses.add(BiteMaterial { age: 1.0 })),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        GlobalZIndex(-10),
        Visibility::Visible,
        Pickable::IGNORE,
    ));
}

fn show_feedback(
    mut events: MessageReader<ConfirmedHit>,
    players: Query<(), With<Player>>,
    state: Res<State<GameState>>,
    mesh: Res<ImpactMesh>,
    mut commands: Commands,
    mut materials: ResMut<Assets<ImpactMaterial>>,
    mut pulses: ResMut<Assets<BiteMaterial>>,
    mut overlay: Query<(&mut Visibility, &MaterialNode<BiteMaterial>), With<BitePulse>>,
) {
    if *state.get() != GameState::Playing {
        events.clear();
        return;
    }
    for event in events.read() {
        if let Some(contact) = event.contact {
            commands.spawn((
                Name::new("Landed attack impact"),
                AttackImpact,
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(materials.add(ImpactMaterial { age: 0.0 })),
                Transform::from_translation(contact.position + contact.normal * 0.09),
            ));
        }
        if players.contains(event.target)
            && let Ok((mut visibility, handle)) = overlay.single_mut()
            && let Some(mut material) = pulses.get_mut(&handle.0)
        {
            material.age = 0.0;
            *visibility = Visibility::Visible;
        }
    }
}

fn age_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<ImpactMaterial>>,
    mut pulses: ResMut<Assets<BiteMaterial>>,
    impacts: Query<(Entity, &MeshMaterial3d<ImpactMaterial>), With<AttackImpact>>,
    mut overlay: Query<(&mut Visibility, &MaterialNode<BiteMaterial>), With<BitePulse>>,
) {
    for (entity, handle) in &impacts {
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.age += time.delta_secs() / IMPACT_SECONDS;
            if material.age >= 1.0 {
                commands.entity(entity).despawn();
            }
        }
    }
    if let Ok((mut visibility, handle)) = overlay.single_mut()
        && *visibility == Visibility::Visible
        && let Some(mut material) = pulses.get_mut(&handle.0)
    {
        material.age += time.delta_secs() / PULSE_SECONDS;
        if material.age >= 1.0 {
            *visibility = Visibility::Hidden;
        }
    }
}

fn face_camera(
    camera: Query<&Transform, With<FollowCamera>>,
    mut impacts: Query<&mut Transform, (With<AttackImpact>, Without<FollowCamera>)>,
) {
    if let Ok(camera) = camera.single() {
        for mut impact in &mut impacts {
            impact.look_at(camera.translation, Vec3::Y);
        }
    }
}

fn clear_effects(
    mut commands: Commands,
    impacts: Query<Entity, With<AttackImpact>>,
    mut overlay: Query<&mut Visibility, With<BitePulse>>,
) {
    for entity in &impacts {
        commands.entity(entity).despawn();
    }
    if let Ok(mut visibility) = overlay.single_mut() {
        *visibility = Visibility::Hidden;
    }
}
