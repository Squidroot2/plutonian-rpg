use crate::{
    GameState,
    combat::Dead,
    interaction::{CorpseLoot, INTERACT_HEIGHT, INTERACT_RANGE, LootTarget},
    inventory::Inventory,
    player::Player,
    ui::text_style,
};
use bevy::prelude::*;

pub(crate) struct LootUiPlugin;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LootAction {
    Take,
    Close,
}

#[derive(Component)]
struct LootContents;
#[derive(Component)]
struct LootNotice;

impl Plugin for LootUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Looting), open)
            .add_systems(
                OnExit(GameState::Looting),
                |mut target: ResMut<LootTarget>| {
                    target.0 = None;
                },
            );
    }
}

fn open(
    target: Res<LootTarget>,
    players: Query<(&Player, &Transform)>,
    corpses: Query<(&CorpseLoot, &Name, &Transform), With<Dead>>,
    mut next: ResMut<NextState<GameState>>,
    mut commands: Commands,
) {
    let corpse = target.0.and_then(|entity| corpses.get(entity).ok());
    let Some((name, stack)) = corpse.and_then(|(loot, name, transform)| {
        let (player, position) = players.single().ok()?;
        let origin = position.translation + Vec3::Y * INTERACT_HEIGHT;
        if player.area != loot.area || origin.distance(transform.translation) >= INTERACT_RANGE {
            return None;
        }
        loot.stack.map(|stack| (name, stack))
    }) else {
        next.set(GameState::Playing);
        return;
    };

    commands
        .spawn((
            DespawnOnExit(GameState::Looting),
            GlobalZIndex(50),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.005, 0.012, 0.025, 0.82)),
        ))
        .with_children(|screen| {
            screen
                .spawn((
                    Node {
                        width: percent(92),
                        max_width: px(400),
                        padding: UiRect::all(px(18)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(12),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.025, 0.055, 0.075)),
                ))
                .with_children(|panel| {
                    panel.spawn((Text::new(name.as_str()), text_style(24.0), Pickable::IGNORE));
                    panel.spawn((
                        LootContents,
                        Text::new(format!("{} x{}", stack.item.name(), stack.quantity)),
                        text_style(18.0),
                        Pickable::IGNORE,
                    ));
                    panel.spawn((
                        LootNotice,
                        Text::default(),
                        text_style(14.0),
                        Pickable::IGNORE,
                    ));
                    panel
                        .spawn(Node {
                            column_gap: px(8),
                            ..default()
                        })
                        .with_children(|row| {
                            for (action, label) in
                                [(LootAction::Take, "Take"), (LootAction::Close, "Close")]
                            {
                                row.spawn((
                                    action,
                                    Node {
                                        padding: UiRect::axes(px(14), px(8)),
                                        justify_content: JustifyContent::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.05, 0.13, 0.17)),
                                ))
                                .observe(click)
                                .with_children(|button| {
                                    button.spawn((
                                        Text::new(label),
                                        text_style(16.0),
                                        Pickable::IGNORE,
                                    ));
                                });
                            }
                        });
                });
        });
}

fn click(
    event: On<PointerClick>,
    actions: Query<&LootAction>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    target: Res<LootTarget>,
    mut corpses: Query<&mut CorpseLoot, With<Dead>>,
    mut inventory: ResMut<Inventory>,
    mut contents: Query<&mut Text, (With<LootContents>, Without<LootNotice>)>,
    mut notices: Query<&mut Text, With<LootNotice>>,
    mut commands: Commands,
) {
    if *state.get() != GameState::Looting || !matches!(*next, NextState::Unchanged) {
        return;
    }
    match actions.get(event.entity) {
        Ok(LootAction::Close) => next.set(GameState::Playing),
        Ok(LootAction::Take) => {
            let Some(mut loot) = target.0.and_then(|entity| corpses.get_mut(entity).ok()) else {
                next.set(GameState::Playing);
                return;
            };
            let Some(stack) = loot.stack else {
                return;
            };
            if !inventory.add(stack.item, stack.quantity) {
                if let Ok(mut notice) = notices.single_mut() {
                    **notice = "The backpack cannot hold this stack.".into();
                }
                return;
            }
            loot.stack = None;
            commands.trigger(crate::combat_audio::ItemPickedUp);
            if let Ok(mut text) = contents.single_mut() {
                **text = "Looted".into();
            }
            if let Ok(mut notice) = notices.single_mut() {
                notice.clear();
            }
            commands.entity(event.entity).despawn();
        }
        Err(_) => {}
    }
}
