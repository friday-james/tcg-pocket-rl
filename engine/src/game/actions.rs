use crate::data::card::{CardType, EnergyType, Stage};
use crate::game::state::*;
use serde::{Deserialize, Serialize};

/// All possible actions a player can take.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    // === Setup phase ===
    /// Place a basic Pokemon from hand as the active Pokemon.
    PlaceActive(usize),
    /// Place a basic Pokemon from hand on the bench.
    PlaceBench(usize),
    /// Done placing Pokemon during setup.
    ConfirmSetup,

    // === Main phase ===
    /// Play a basic Pokemon from hand to an empty bench slot.
    PlayPokemonToBench(usize),
    /// Evolve a Pokemon: (hand_index, board_position).
    EvolvePokemon(usize, usize),
    /// Set the energy zone type for this turn.
    SetEnergyZoneType(EnergyType),
    /// Attach energy from energy zone to a Pokemon (board_position).
    AttachEnergy(usize),
    /// Retreat: swap active with bench Pokemon (bench_index, 0-based in bench array).
    Retreat(usize),
    /// Use an ability on a Pokemon (board_position).
    UseAbility(usize),
    /// Play a Trainer/Item from hand (hand_index).
    PlayTrainer(usize),
    /// Play a Supporter from hand (hand_index).
    PlaySupporter(usize),
    /// Choose an attack to use (attack_index on the active Pokemon).
    UseAttack(usize),
    /// End the turn without attacking.
    EndTurn,

    // === Effect choice phase ===
    /// Choose a target board position for an effect.
    ChooseTarget(usize),
    /// Choose a card from a selection (index into the valid options).
    ChooseOption(usize),
    /// Promote a bench Pokemon to active (bench_index).
    PromotePokemon(usize),
}

/// Generate all legal actions for the current game state.
pub fn legal_actions(state: &GameState) -> Vec<Action> {
    match state.phase {
        TurnPhase::Setup => legal_actions_setup(state),
        TurnPhase::Main => legal_actions_main(state),
        TurnPhase::EffectChoice => legal_actions_effect_choice(state),
        TurnPhase::GameOver => vec![],
        // DrawCard, Attack, BetweenTurns are handled automatically by the engine
        _ => vec![],
    }
}

fn legal_actions_setup(state: &GameState) -> Vec<Action> {
    let player = state.current();
    let mut actions = Vec::new();

    // Must place an active Pokemon first
    if player.active.is_none() {
        for (i, card) in player.hand.iter().enumerate() {
            if card.is_basic_pokemon() {
                actions.push(Action::PlaceActive(i));
            }
        }
        return actions;
    }

    // Can optionally place bench Pokemon
    if player.bench_count() < MAX_BENCH {
        for (i, card) in player.hand.iter().enumerate() {
            if card.is_basic_pokemon() {
                actions.push(Action::PlaceBench(i));
            }
        }
    }

    // Can confirm setup
    actions.push(Action::ConfirmSetup);

    actions
}

fn legal_actions_main(state: &GameState) -> Vec<Action> {
    let player = state.current();
    let mut actions = Vec::new();

    // --- Play basic Pokemon to bench ---
    if player.bench_count() < MAX_BENCH {
        for (i, card) in player.hand.iter().enumerate() {
            if card.is_basic_pokemon() {
                actions.push(Action::PlayPokemonToBench(i));
            }
        }
    }

    // --- Evolve Pokemon ---
    for (hand_idx, hand_card) in player.hand.iter().enumerate() {
        if !hand_card.is_evolution() {
            continue;
        }
        let evolves_from = match &hand_card.evolves_from {
            Some(name) => name.as_str(),
            None => continue,
        };

        // Check each Pokemon in play
        for (pos, pokemon) in player.all_pokemon() {
            // Must match the evolution source name
            if pokemon.card.name != evolves_from {
                continue;
            }
            // Cannot evolve a Pokemon that was just played this turn
            if !pokemon.can_evolve(state.turn_number) {
                continue;
            }
            // Stage must match: Basic -> Stage1, Stage1 -> Stage2
            let valid_evo = match hand_card.stage {
                Some(Stage::Stage1) => pokemon.card.stage == Some(Stage::Basic),
                Some(Stage::Stage2) => pokemon.card.stage == Some(Stage::Stage1),
                _ => false,
            };
            if valid_evo {
                actions.push(Action::EvolvePokemon(hand_idx, pos));
            }
        }
    }

    // --- Set energy zone type (if not set) ---
    if player.energy_zone_type.is_none() {
        for &energy_type in EnergyType::concrete_types() {
            actions.push(Action::SetEnergyZoneType(energy_type));
        }
    }

    // --- Attach energy (1 per turn from energy zone) ---
    if !player.energy_generated && player.energy_zone_type.is_some() {
        for (pos, _pokemon) in player.all_pokemon() {
            actions.push(Action::AttachEnergy(pos));
        }
    }

    // --- Retreat (1 per turn, if active has enough energy for retreat cost) ---
    if !player.retreated_this_turn {
        if let Some(ref active) = player.active {
            if !active.has_status(StatusCondition::Paralyzed)
                && !active.has_status(StatusCondition::Asleep)
            {
                let retreat_cost = active.card.retreat_cost.unwrap_or(0) as usize;
                if active.attached_energy.len() >= retreat_cost {
                    for (i, slot) in player.bench.iter().enumerate() {
                        if slot.is_some() {
                            actions.push(Action::Retreat(i));
                        }
                    }
                }
            }
        }
    }

    // --- Use ability ---
    for (pos, pokemon) in player.all_pokemon() {
        if pokemon.card.ability.is_some() && !pokemon.temp_flags.used_ability {
            actions.push(Action::UseAbility(pos));
        }
    }

    // --- Play Trainer/Item cards ---
    for (i, card) in player.hand.iter().enumerate() {
        match card.card_type {
            CardType::Item | CardType::Tool | CardType::Fossil => {
                actions.push(Action::PlayTrainer(i));
            }
            CardType::Supporter => {
                if !player.supporter_played {
                    actions.push(Action::PlaySupporter(i));
                }
            }
            _ => {}
        }
    }

    // --- Attack (if active Pokemon has enough energy) ---
    if !state.first_turn {
        if let Some(ref active) = player.active {
            if !active.has_status(StatusCondition::Paralyzed) {
                for (attack_idx, _attack) in active.card.attacks.iter().enumerate() {
                    if active.card.can_use_attack(attack_idx, &active.attached_energy) {
                        actions.push(Action::UseAttack(attack_idx));
                    }
                }
            }
        }
    }

    // --- End turn (always available) ---
    actions.push(Action::EndTurn);

    actions
}

fn legal_actions_effect_choice(state: &GameState) -> Vec<Action> {
    let mut actions = Vec::new();

    match &state.pending_choice {
        Some(PendingChoice::PromoteFromBench) => {
            let player = state.current();
            for (i, slot) in player.bench.iter().enumerate() {
                if slot.is_some() {
                    actions.push(Action::PromotePokemon(i));
                }
            }
        }
        Some(PendingChoice::ChooseTarget { valid_targets, .. }) => {
            for &target in valid_targets {
                actions.push(Action::ChooseTarget(target));
            }
        }
        Some(PendingChoice::DiscardFromHand { .. }) => {
            let player = state.current();
            for i in 0..player.hand.len() {
                actions.push(Action::ChooseOption(i));
            }
        }
        Some(PendingChoice::DiscardEnergy { pokemon_position, .. }) => {
            let player = state.current();
            if let Some(pokemon) = player.get_pokemon(*pokemon_position) {
                for i in 0..pokemon.attached_energy.len() {
                    actions.push(Action::ChooseOption(i));
                }
            }
        }
        None => {}
    }

    actions
}
