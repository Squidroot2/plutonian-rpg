use std::{
    f32::consts::TAU,
    num::{NonZeroU16, NonZeroU32},
    time::Duration,
};

use bevy::{
    audio::{AddAudioSource, ChannelCount, Decodable, SampleRate, Source, Volume},
    prelude::*,
};

use crate::{
    GameState,
    combat::{CombatSystems, ConfirmedHit, Dead},
    enemy::Enemy,
    weapon::WeaponAttack,
};

pub(crate) struct CombatAudioPlugin;

impl Plugin for CombatAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Effect>()
            .add_systems(Startup, setup)
            .add_observer(play_pickup)
            .add_systems(
                Update,
                play.after(CombatSystems::Resolve)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Asset, TypePath, Clone, Copy)]
enum Effect {
    Thrust,
    Hit,
    Death,
    Pickup,
}

#[derive(Resource)]
struct Sounds {
    thrust: Handle<Effect>,
    hit: Handle<Effect>,
    death: Handle<Effect>,
    pickup: Handle<Effect>,
}

fn setup(mut commands: Commands, mut assets: ResMut<Assets<Effect>>) {
    commands.insert_resource(Sounds {
        thrust: assets.add(Effect::Thrust),
        hit: assets.add(Effect::Hit),
        death: assets.add(Effect::Death),
        pickup: assets.add(Effect::Pickup),
    });
}

#[derive(Event)]
pub(crate) struct ItemPickedUp;

fn play_pickup(_: On<ItemPickedUp>, sounds: Res<Sounds>, mut commands: Commands) {
    spawn_sound(&mut commands, &sounds.pickup);
}

fn spawn_sound(commands: &mut Commands, sound: &Handle<Effect>) {
    commands.spawn((
        AudioPlayer(sound.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.2)),
    ));
}

fn play(
    mut commands: Commands,
    sounds: Res<Sounds>,
    thrust: Res<WeaponAttack>,
    mut hits: MessageReader<ConfirmedHit>,
    deaths: Query<(), (Added<Dead>, With<Enemy>)>,
) {
    if thrust.just_started() {
        spawn_sound(&mut commands, &sounds.thrust);
    }
    for _ in hits.read() {
        spawn_sound(&mut commands, &sounds.hit);
    }
    for () in &deaths {
        spawn_sound(&mut commands, &sounds.death);
    }
}

const RATE: u32 = 24_000;

struct Samples {
    effect: Effect,
    length: usize,
    index: usize,
    noise: u32,
    filtered: f32,
}

impl Decodable for Effect {
    type Decoder = Samples;

    fn decoder(&self) -> Samples {
        let length = match self {
            Self::Thrust => 3_600,
            Self::Hit => 2_880,
            Self::Death => 9_600,
            Self::Pickup => 3_840,
        };
        Samples {
            effect: *self,
            length,
            index: 0,
            noise: 7241,
            filtered: 0.0,
        }
    }
}

impl Iterator for Samples {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.index >= self.length {
            return None;
        }
        let progress = self.index as f32 / self.length as f32;
        let seconds = self.index as f32 / RATE as f32;
        self.index += 1;
        self.noise = self
            .noise
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let noise = (self.noise >> 8) as f32 / 8_388_608.0 - 1.0;
        self.filtered += 0.16 * (noise - self.filtered);
        let sweep = |start: f32, drop: f32| (TAU * seconds * (start - drop * progress * 0.5)).sin();
        let tail = 1.0 - progress;
        let sample = match self.effect {
            Effect::Thrust => (noise - self.filtered) * (4.0 * progress * tail) * tail,
            Effect::Hit => (0.7 * sweep(150.0, 95.0) + 0.3 * noise) * tail.powi(4),
            Effect::Pickup => {
                let phase = TAU * seconds * (620.0 + 220.0 * progress);
                (0.55 * phase.sin() + 0.15 * (phase * 2.0).sin()) * tail.powi(3)
            }
            Effect::Death => (0.55 * sweep(125.0, 95.0) + 0.45 * self.filtered) * tail.powi(2),
        };
        Some(sample * (seconds * 350.0).min(1.0))
    }
}

impl Source for Samples {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.length - self.index)
    }

    fn channels(&self) -> ChannelCount {
        NonZeroU16::MIN
    }

    fn sample_rate(&self) -> SampleRate {
        NonZeroU32::new(RATE).expect("the effect sample rate is nonzero")
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.length as f64 / RATE as f64))
    }
}
