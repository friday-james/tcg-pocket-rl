use crate::data::card::{Card, EnergyType};
use crate::game::state::*;

/// Observation vector size for the RL agent.
pub const OBS_SIZE: usize = 512;

/// Maximum cards that can be encoded in hand.
const MAX_HAND_CARDS: usize = 10;
/// Features per card in hand.
const CARD_FEATURES: usize = 12;
/// Features per Pokemon on the board.
const POKEMON_FEATURES: usize = 20;
/// Number of board slots (active + 3 bench) per player.
const BOARD_SLOTS: usize = 4;

/// Encode the game state as a fixed-size observation vector.
/// The observation is from the perspective of the specified player.
pub fn encode_observation(state: &GameState, player_idx: usize) -> Vec<f32> {
    let mut obs = vec![0.0f32; OBS_SIZE];
    let mut offset = 0;

    let player = &state.players[player_idx];
    let opponent = &state.players[1 - player_idx];

    // --- Game metadata (8 features) ---
    obs[offset] = state.turn_number as f32 / 50.0; // normalized turn number
    offset += 1;
    obs[offset] = if state.current_player == player_idx {
        1.0
    } else {
        0.0
    };
    offset += 1;
    obs[offset] = player.points as f32 / 3.0; // own points
    offset += 1;
    obs[offset] = opponent.points as f32 / 3.0; // opponent points
    offset += 1;
    obs[offset] = if player.energy_generated { 1.0 } else { 0.0 };
    offset += 1;
    obs[offset] = if player.supporter_played { 1.0 } else { 0.0 };
    offset += 1;
    obs[offset] = if player.retreated_this_turn {
        1.0
    } else {
        0.0
    };
    offset += 1;
    obs[offset] = encode_energy_type_onehot(player.energy_zone_type);
    offset += 1;

    // --- Own board (active + 3 bench = 4 * POKEMON_FEATURES) ---
    for pos in 0..BOARD_SLOTS {
        encode_pokemon(player.get_pokemon(pos), &mut obs, offset);
        offset += POKEMON_FEATURES;
    }

    // --- Opponent board (4 * POKEMON_FEATURES) ---
    for pos in 0..BOARD_SLOTS {
        encode_pokemon(opponent.get_pokemon(pos), &mut obs, offset);
        offset += POKEMON_FEATURES;
    }

    // --- Own hand (MAX_HAND_CARDS * CARD_FEATURES) ---
    for i in 0..MAX_HAND_CARDS {
        if i < player.hand.len() {
            encode_card_in_hand(&player.hand[i], &mut obs, offset);
        }
        offset += CARD_FEATURES;
    }

    // --- Hidden info counts ---
    obs[offset] = player.hand.len() as f32 / 10.0;
    offset += 1;
    obs[offset] = player.deck.len() as f32 / 20.0;
    offset += 1;
    obs[offset] = opponent.hand.len() as f32 / 10.0;
    offset += 1;
    obs[offset] = opponent.deck.len() as f32 / 20.0;
    // offset += 1;

    obs
}

fn encode_pokemon(pokemon: Option<&PlayedCard>, obs: &mut [f32], offset: usize) {
    let Some(p) = pokemon else {
        return; // All zeros for empty slot
    };

    let mut i = offset;

    // Slot occupied
    obs[i] = 1.0;
    i += 1;

    // HP (normalized)
    obs[i] = p.max_hp() as f32 / 300.0;
    i += 1;

    // Current HP ratio
    obs[i] = p.remaining_hp().max(0) as f32 / p.max_hp().max(1) as f32;
    i += 1;

    // Energy type (one-hot, 10 types)
    if let Some(et) = p.card.energy_type {
        obs[i + energy_type_index(et)] = 1.0;
    }
    i += 10;

    // Attached energy count (normalized)
    obs[i] = p.attached_energy.len() as f32 / 5.0;
    i += 1;

    // Is EX
    obs[i] = if p.card.is_ex { 1.0 } else { 0.0 };
    i += 1;

    // Status conditions
    obs[i] = if p.has_status(StatusCondition::Poisoned) { 1.0 } else { 0.0 };
    i += 1;
    obs[i] = if p.has_status(StatusCondition::Burned) { 1.0 } else { 0.0 };
    i += 1;
    obs[i] = if p.has_status(StatusCondition::Asleep) { 1.0 } else { 0.0 };
    i += 1;
    obs[i] = if p.has_status(StatusCondition::Paralyzed) { 1.0 } else { 0.0 };
}

fn encode_card_in_hand(card: &Card, obs: &mut [f32], offset: usize) {
    let mut i = offset;

    // Card exists in this slot
    obs[i] = 1.0;
    i += 1;

    // Is Pokemon
    obs[i] = if card.is_pokemon() { 1.0 } else { 0.0 };
    i += 1;

    // Is basic
    obs[i] = if card.is_basic_pokemon() { 1.0 } else { 0.0 };
    i += 1;

    // Is evolution
    obs[i] = if card.is_evolution() { 1.0 } else { 0.0 };
    i += 1;

    // Is trainer
    obs[i] = if card.is_trainer() { 1.0 } else { 0.0 };
    i += 1;

    // HP (normalized)
    obs[i] = card.hp.unwrap_or(0) as f32 / 300.0;
    i += 1;

    // Energy type
    if let Some(et) = card.energy_type {
        obs[i] = (energy_type_index(et) as f32 + 1.0) / 10.0;
    }
    i += 1;

    // Retreat cost
    obs[i] = card.retreat_cost.unwrap_or(0) as f32 / 4.0;
    i += 1;

    // Is EX
    obs[i] = if card.is_ex { 1.0 } else { 0.0 };
    i += 1;

    // Number of attacks
    obs[i] = card.attacks.len() as f32 / 3.0;
    i += 1;

    // Highest attack damage (normalized)
    obs[i] = card
        .attacks
        .iter()
        .map(|a| a.damage)
        .max()
        .unwrap_or(0) as f32
        / 200.0;
    i += 1;

    // Total energy cost of cheapest attack
    obs[i] = card
        .attacks
        .iter()
        .map(|a| a.energy_cost.len())
        .min()
        .unwrap_or(0) as f32
        / 5.0;
}

fn energy_type_index(et: EnergyType) -> usize {
    match et {
        EnergyType::Grass => 0,
        EnergyType::Fire => 1,
        EnergyType::Water => 2,
        EnergyType::Lightning => 3,
        EnergyType::Psychic => 4,
        EnergyType::Fighting => 5,
        EnergyType::Darkness => 6,
        EnergyType::Metal => 7,
        EnergyType::Dragon => 8,
        EnergyType::Colorless => 9,
    }
}

fn encode_energy_type_onehot(et: Option<EnergyType>) -> f32 {
    et.map(|e| (energy_type_index(e) as f32 + 1.0) / 10.0)
        .unwrap_or(0.0)
}
