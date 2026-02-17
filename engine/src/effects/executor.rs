use super::mechanics::*;
use super::registry::EffectRegistry;
use crate::game::rng::GameRng;
use crate::game::state::*;

/// Execute the effects of an attack.
pub fn execute_attack_effects(
    state: &mut GameState,
    registry: &EffectRegistry,
    card_id: &str,
    attack_idx: usize,
    rng: &mut GameRng,
) {
    let effects = registry.get_attack_effects(card_id, attack_idx).to_vec();
    for mechanic in effects {
        execute_mechanic(state, &mechanic, rng);
    }
}

/// Execute a single mechanic on the game state.
pub fn execute_mechanic(state: &mut GameState, mechanic: &Mechanic, rng: &mut GameRng) {
    let current = state.current_player;
    let opponent = 1 - current;

    match mechanic {
        Mechanic::Heal { amount, target } => {
            let pokemon = resolve_target_mut(state, *target);
            if let Some(p) = pokemon {
                let heal = (*amount / 10).min(p.damage_counters);
                p.damage_counters -= heal;
            }
        }

        Mechanic::ApplyStatus(status, target) => {
            let pokemon = resolve_target_mut(state, *target);
            if let Some(p) = pokemon {
                p.apply_status(*status);
            }
        }

        Mechanic::ApplyStatusOnCoinFlip(status, target) => {
            if rng.coin_flip() {
                let pokemon = resolve_target_mut(state, *target);
                if let Some(p) = pokemon {
                    p.apply_status(*status);
                }
            }
        }

        Mechanic::DamageOnCoinFlip(damage) => {
            // If tails, negate the attack damage (handled at attack resolution level)
            if !rng.coin_flip() {
                // Set a flag to negate damage
                if let Some(ref mut active) = state.players[opponent].active {
                    // We can't easily negate damage here; this needs integration
                    // with the attack resolution system
                }
            }
        }

        Mechanic::DamagePerCoinFlip {
            damage_per_heads,
            flips,
        } => {
            let heads = rng.coin_flips(*flips);
            let total_damage = heads * damage_per_heads;
            if total_damage > 0 {
                if let Some(ref mut target) = state.players[opponent].active {
                    target.damage_counters += total_damage / 10;
                }
            }
        }

        Mechanic::ConditionalDamage {
            base: _,
            bonus,
            condition,
        } => {
            let condition_met = check_condition(state, condition, rng);
            if condition_met {
                if let Some(ref mut target) = state.players[opponent].active {
                    target.damage_counters += bonus / 10;
                }
            }
        }

        Mechanic::BenchDamage { damage, target } => match target {
            Target::OpponentBench => {
                for slot in &mut state.players[opponent].bench {
                    if let Some(ref mut pokemon) = slot {
                        pokemon.damage_counters += damage / 10;
                    }
                }
            }
            _ => {
                // ChooseOpponentBench needs a pending choice - TODO
            }
        },

        Mechanic::DiscardEnergy {
            count,
            energy_type,
            target,
        } => {
            let pokemon = resolve_target_mut(state, *target);
            if let Some(p) = pokemon {
                for _ in 0..*count {
                    if let Some(et) = energy_type {
                        if let Some(pos) = p.attached_energy.iter().position(|e| e == et) {
                            p.attached_energy.remove(pos);
                        }
                    } else {
                        p.attached_energy.pop();
                    }
                }
            }
        }

        Mechanic::DrawCards(count) => {
            for _ in 0..*count {
                if let Some(card) = state.players[current].deck.pop() {
                    state.players[current].hand.push(card);
                }
            }
        }

        Mechanic::SelfDamage(damage) => {
            if let Some(ref mut active) = state.players[current].active {
                active.damage_counters += damage / 10;
            }
        }

        Mechanic::PreventDamage(amount) => {
            if let Some(ref mut active) = state.players[current].active {
                active.temp_flags.prevent_damage_amount = *amount;
            }
        }

        Mechanic::SwitchOpponentActive => {
            // Need pending choice for opponent to choose bench Pokemon
            state.pending_choice = Some(PendingChoice::ChooseTarget {
                valid_targets: state.players[opponent]
                    .bench
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.is_some())
                    .map(|(i, _)| i + 1)
                    .collect(),
                description: "Choose a bench Pokemon to switch to active".to_string(),
            });
            state.phase = TurnPhase::EffectChoice;
        }

        Mechanic::Custom(_text) => {
            // Custom effects need per-card handling
            // For now, these are no-ops
        }

        _ => {}
    }
}

fn check_condition(state: &GameState, condition: &DamageCondition, rng: &mut GameRng) -> bool {
    let current = state.current_player;
    let opponent = 1 - current;

    match condition {
        DamageCondition::TargetHasDamage => state.players[opponent]
            .active
            .as_ref()
            .map_or(false, |p| p.damage_counters > 0),
        DamageCondition::CoinFlipHeads => rng.coin_flip(),
        DamageCondition::PerOwnBench => state.players[current].bench_count() > 0,
        DamageCondition::PerOpponentBench => state.players[opponent].bench_count() > 0,
        _ => false,
    }
}

fn resolve_target_mut<'a>(state: &'a mut GameState, target: Target) -> Option<&'a mut PlayedCard> {
    let current = state.current_player;
    let opponent = 1 - current;
    match target {
        Target::This => state.players[current].active.as_mut(),
        Target::OpponentActive => state.players[opponent].active.as_mut(),
        _ => None, // Complex targets need pending choices
    }
}
