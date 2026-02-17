use crate::data::card::{Card, EnergyType};
use crate::data::deck::Deck;
use crate::game::actions::Action;
use crate::game::rng::GameRng;
use crate::game::state::*;

/// Result of applying an action.
#[derive(Debug)]
pub enum StepResult {
    /// Action applied, game continues.
    Continue,
    /// Game is over.
    GameOver { winner: usize },
    /// Invalid action.
    InvalidAction(String),
}

/// Initialize a new game from two decks and a seed.
pub fn new_game(deck1: Deck, deck2: Deck, seed: u64) -> (GameState, GameRng) {
    let mut rng = GameRng::new(seed);

    let mut state = GameState {
        players: [PlayerState::new(), PlayerState::new()],
        current_player: 0,
        turn_number: 0,
        phase: TurnPhase::Setup,
        winner: None,
        first_turn: true,
        pending_choice: None,
    };

    // Shuffle and deal for both players
    for (i, deck) in [deck1, deck2].into_iter().enumerate() {
        let mut cards = deck.cards;
        rng.shuffle(&mut cards);

        // Draw starting hand
        let hand: Vec<Card> = cards.drain(..STARTING_HAND.min(cards.len())).collect();
        // Set aside prize cards
        let prizes: Vec<Card> = cards.drain(..PRIZE_COUNT.min(cards.len())).collect();

        state.players[i].hand = hand;
        state.players[i].prizes = prizes;
        state.players[i].deck = cards;
    }

    // Ensure both players have at least one basic Pokemon in hand
    // (Standard rule: mulligan until you have at least one basic)
    for i in 0..2 {
        ensure_basic_in_hand(&mut state.players[i], &mut rng);
    }

    (state, rng)
}

/// Ensure a player has at least one basic Pokemon in hand (mulligan rule).
fn ensure_basic_in_hand(player: &mut PlayerState, rng: &mut GameRng) {
    let mut attempts = 0;
    while !player.has_basic_in_hand() && attempts < 10 {
        // Shuffle hand back into deck
        let mut all_cards = Vec::new();
        all_cards.append(&mut player.hand);
        all_cards.append(&mut player.deck);
        all_cards.append(&mut player.prizes);
        rng.shuffle(&mut all_cards);

        player.hand = all_cards.drain(..STARTING_HAND.min(all_cards.len())).collect();
        player.prizes = all_cards.drain(..PRIZE_COUNT.min(all_cards.len())).collect();
        player.deck = all_cards;
        attempts += 1;
    }
}

/// Apply an action to the game state.
pub fn apply_action(state: &mut GameState, action: &Action, rng: &mut GameRng) -> StepResult {
    match state.phase {
        TurnPhase::Setup => apply_setup_action(state, action, rng),
        TurnPhase::Main => apply_main_action(state, action, rng),
        TurnPhase::EffectChoice => apply_effect_choice(state, action, rng),
        _ => StepResult::InvalidAction(format!("Cannot act in phase {:?}", state.phase)),
    }
}

fn apply_setup_action(state: &mut GameState, action: &Action, rng: &mut GameRng) -> StepResult {
    match action {
        Action::PlaceActive(hand_idx) => {
            let player = &mut state.players[state.current_player];
            if *hand_idx >= player.hand.len() {
                return StepResult::InvalidAction("Invalid hand index".into());
            }
            let card = player.hand.remove(*hand_idx);
            player.active = Some(PlayedCard::new(card, state.turn_number));
            StepResult::Continue
        }
        Action::PlaceBench(hand_idx) => {
            let player = &mut state.players[state.current_player];
            if *hand_idx >= player.hand.len() {
                return StepResult::InvalidAction("Invalid hand index".into());
            }
            if let Some(slot_idx) = player.find_empty_bench() {
                let card = player.hand.remove(*hand_idx);
                player.bench[slot_idx] = Some(PlayedCard::new(card, state.turn_number));
                StepResult::Continue
            } else {
                StepResult::InvalidAction("Bench is full".into())
            }
        }
        Action::ConfirmSetup => {
            if state.current_player == 0 {
                // Player 0 done, switch to player 1 setup
                state.current_player = 1;
                StepResult::Continue
            } else {
                // Both players done setup, start the game
                state.current_player = 0;
                state.phase = TurnPhase::Main;
                state.first_turn = true;
                // Draw a card for the first player
                draw_card(state);
                StepResult::Continue
            }
        }
        _ => StepResult::InvalidAction(format!("Invalid action for setup: {:?}", action)),
    }
}

fn apply_main_action(state: &mut GameState, action: &Action, rng: &mut GameRng) -> StepResult {
    match action {
        Action::PlayPokemonToBench(hand_idx) => {
            let turn = state.turn_number;
            let player = state.current_mut();
            if *hand_idx >= player.hand.len() {
                return StepResult::InvalidAction("Invalid hand index".into());
            }
            if let Some(slot_idx) = player.find_empty_bench() {
                let card = player.hand.remove(*hand_idx);
                player.bench[slot_idx] = Some(PlayedCard::new(card, turn));
                StepResult::Continue
            } else {
                StepResult::InvalidAction("Bench is full".into())
            }
        }

        Action::EvolvePokemon(hand_idx, board_pos) => {
            let turn = state.turn_number;
            let player = state.current_mut();
            if *hand_idx >= player.hand.len() {
                return StepResult::InvalidAction("Invalid hand index".into());
            }
            let evo_card = player.hand.remove(*hand_idx);
            if let Some(pokemon) = player.get_pokemon_mut(*board_pos) {
                let old_pokemon = std::mem::replace(
                    pokemon,
                    PlayedCard::new(evo_card, turn),
                );
                // The new Pokemon inherits energy and damage
                let current = player.get_pokemon_mut(*board_pos).unwrap();
                current.attached_energy = old_pokemon.attached_energy.clone();
                current.damage_counters = old_pokemon.damage_counters;
                current.evolved_from = Some(Box::new(old_pokemon));
                // Evolution removes all status conditions
                current.clear_status();
                StepResult::Continue
            } else {
                StepResult::InvalidAction("No Pokemon at position".into())
            }
        }

        Action::SetEnergyZoneType(energy_type) => {
            state.current_mut().energy_zone_type = Some(*energy_type);
            StepResult::Continue
        }

        Action::AttachEnergy(board_pos) => {
            let player = state.current_mut();
            if player.energy_generated {
                return StepResult::InvalidAction("Already attached energy this turn".into());
            }
            let energy_type = match player.energy_zone_type {
                Some(t) => t,
                None => return StepResult::InvalidAction("No energy zone type set".into()),
            };
            if let Some(pokemon) = player.get_pokemon_mut(*board_pos) {
                pokemon.attached_energy.push(energy_type);
                // Mark energy as generated (borrow released above)
            } else {
                return StepResult::InvalidAction("No Pokemon at position".into());
            }
            state.current_mut().energy_generated = true;
            StepResult::Continue
        }

        Action::Retreat(bench_idx) => {
            let player = state.current_mut();
            if player.retreated_this_turn {
                return StepResult::InvalidAction("Already retreated this turn".into());
            }
            if *bench_idx >= MAX_BENCH || player.bench[*bench_idx].is_none() {
                return StepResult::InvalidAction("Invalid bench index".into());
            }

            // Pay retreat cost (discard colorless energy)
            if let Some(ref mut active) = player.active {
                let cost = active.card.retreat_cost.unwrap_or(0) as usize;
                if active.attached_energy.len() < cost {
                    return StepResult::InvalidAction("Not enough energy to retreat".into());
                }
                // Remove energy from the end (least specific first)
                for _ in 0..cost {
                    active.attached_energy.pop();
                }
                // Retreating clears status conditions
                active.clear_status();
            }

            // Swap active with bench Pokemon
            let bench_pokemon = player.bench[*bench_idx].take();
            let active_pokemon = player.active.take();
            player.active = bench_pokemon;
            // Put old active on the bench
            player.bench[*bench_idx] = active_pokemon;
            player.retreated_this_turn = true;
            StepResult::Continue
        }

        Action::UseAbility(board_pos) => {
            // TODO: implement ability effects in effects module
            let player = state.current_mut();
            if let Some(pokemon) = player.get_pokemon_mut(*board_pos) {
                pokemon.temp_flags.used_ability = true;
            }
            StepResult::Continue
        }

        Action::PlayTrainer(hand_idx) | Action::PlaySupporter(hand_idx) => {
            let player = state.current_mut();
            if *hand_idx >= player.hand.len() {
                return StepResult::InvalidAction("Invalid hand index".into());
            }
            let card = player.hand.remove(*hand_idx);
            if matches!(action, Action::PlaySupporter(_)) {
                player.supporter_played = true;
            }
            // TODO: execute trainer effects in effects module
            player.discard.push(card);
            StepResult::Continue
        }

        Action::UseAttack(attack_idx) => {
            if state.first_turn {
                return StepResult::InvalidAction("Cannot attack on first turn".into());
            }
            return resolve_attack(state, *attack_idx, rng);
        }

        Action::EndTurn => {
            end_turn(state, rng);
            StepResult::Continue
        }

        _ => StepResult::InvalidAction(format!("Invalid action for main phase: {:?}", action)),
    }
}

fn apply_effect_choice(state: &mut GameState, action: &Action, rng: &mut GameRng) -> StepResult {
    match (&state.pending_choice, action) {
        (Some(PendingChoice::PromoteFromBench), Action::PromotePokemon(bench_idx)) => {
            let player = state.current_mut();
            if *bench_idx < MAX_BENCH {
                let pokemon = player.bench[*bench_idx].take();
                player.active = pokemon;
            }
            state.pending_choice = None;
            // Continue with whatever phase we were in
            check_win_conditions(state);
            StepResult::Continue
        }
        _ => StepResult::InvalidAction(format!(
            "Invalid effect choice: {:?} for {:?}",
            action, state.pending_choice
        )),
    }
}

/// Resolve an attack.
fn resolve_attack(state: &mut GameState, attack_idx: usize, rng: &mut GameRng) -> StepResult {
    let current_player = state.current_player;
    let opponent_idx = 1 - current_player;

    // Get attack data
    let attack = {
        let active = match state.players[current_player].active {
            Some(ref a) => a,
            None => return StepResult::InvalidAction("No active Pokemon".into()),
        };
        match active.card.attacks.get(attack_idx) {
            Some(a) => a.clone(),
            None => return StepResult::InvalidAction("Invalid attack index".into()),
        }
    };

    // Calculate base damage
    let mut damage = attack.damage;

    // Apply weakness
    if let Some(ref opponent_active) = state.players[opponent_idx].active {
        if let Some(weakness) = opponent_active.card.weakness {
            if let Some(attacker_type) = state.players[current_player]
                .active
                .as_ref()
                .and_then(|a| a.card.energy_type)
            {
                if attacker_type == weakness {
                    damage += 20;
                }
            }
        }
    }

    // Apply temporary damage bonuses
    if let Some(ref active) = state.players[current_player].active {
        damage += active.temp_flags.bonus_damage;
    }

    // Apply damage prevention on defender
    if let Some(ref opponent_active) = state.players[opponent_idx].active {
        let prevent = opponent_active.temp_flags.prevent_damage_amount;
        damage = damage.saturating_sub(prevent);
    }

    // TODO: Apply attack effects (effects module will handle this)

    // Deal damage to opponent's active Pokemon
    if damage > 0 {
        if let Some(ref mut target) = state.players[opponent_idx].active {
            target.damage_counters += damage / 10;
        }
    }

    // Check if opponent's active Pokemon is KO'd
    let ko = state.players[opponent_idx]
        .active
        .as_ref()
        .map_or(false, |p| p.is_knocked_out());

    if ko {
        handle_knockout(state, opponent_idx);
    }

    // End the turn
    end_turn(state, rng);

    // Check win conditions
    check_win_conditions(state);

    if let Some(winner) = state.winner {
        StepResult::GameOver { winner }
    } else {
        StepResult::Continue
    }
}

/// Handle a Pokemon being knocked out.
fn handle_knockout(state: &mut GameState, knocked_out_player: usize) {
    let attacker = 1 - knocked_out_player;

    // Move KO'd Pokemon to discard
    if let Some(ko_pokemon) = state.players[knocked_out_player].active.take() {
        let points = if ko_pokemon.card.is_ex { 2 } else { 1 };
        state.players[attacker].points += points;

        // Discard the KO'd Pokemon and all attached cards
        state.players[knocked_out_player].discard.push(ko_pokemon.card);
        // Also discard the pre-evolution chain
        // (simplified: just discard the top-level card)
    }

    // If the KO'd player has bench Pokemon, they must promote one
    if state.players[knocked_out_player].bench_count() > 0 {
        // If only one bench Pokemon, auto-promote
        let bench_pokemon: Vec<usize> = state.players[knocked_out_player]
            .bench
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some())
            .map(|(i, _)| i)
            .collect();

        if bench_pokemon.len() == 1 {
            let idx = bench_pokemon[0];
            state.players[knocked_out_player].active =
                state.players[knocked_out_player].bench[idx].take();
        } else if !bench_pokemon.is_empty() {
            // Player must choose which bench Pokemon to promote
            state.pending_choice = Some(PendingChoice::PromoteFromBench);
            state.phase = TurnPhase::EffectChoice;
            // Temporarily switch to the KO'd player for their choice
            state.current_player = knocked_out_player;
        }
    }
}

/// End the current turn and start the next.
fn end_turn(state: &mut GameState, rng: &mut GameRng) {
    // Resolve between-turns effects
    resolve_between_turns(state, rng);

    // Switch to next player
    state.current_player = 1 - state.current_player;
    state.turn_number += 1;
    state.first_turn = false;

    // Reset per-turn state
    state.current_mut().start_turn();

    // Draw a card
    draw_card(state);

    // Set phase to Main
    if state.phase != TurnPhase::GameOver {
        state.phase = TurnPhase::Main;
    }
}

/// Resolve between-turns effects (poison, burn, etc.).
fn resolve_between_turns(state: &mut GameState, rng: &mut GameRng) {
    let current = state.current_player;

    // Status effects on the current player's active Pokemon
    if let Some(ref mut active) = state.players[current].active {
        // Poison: 10 damage between turns
        if active.has_status(StatusCondition::Poisoned) {
            active.damage_counters += 1; // 10 damage
        }

        // Burn: flip a coin, if tails take 20 damage
        if active.has_status(StatusCondition::Burned) {
            if !rng.coin_flip() {
                active.damage_counters += 2; // 20 damage
            }
        }

        // Asleep: flip a coin, if heads wake up
        if active.has_status(StatusCondition::Asleep) {
            if rng.coin_flip() {
                active.status_conditions.retain(|s| *s != StatusCondition::Asleep);
            }
        }

        // Paralyzed: removed at end of turn (lasts 1 turn)
        active
            .status_conditions
            .retain(|s| *s != StatusCondition::Paralyzed);
    }

    // Check if status damage KO'd the active Pokemon
    let ko = state.players[current]
        .active
        .as_ref()
        .map_or(false, |p| p.is_knocked_out());

    if ko {
        handle_knockout(state, current);
    }
}

/// Draw a card from the deck.
fn draw_card(state: &mut GameState) {
    let player = state.current_mut();
    if let Some(card) = player.deck.pop() {
        player.hand.push(card);
    } else {
        // Cannot draw - lose the game
        state.winner = Some(1 - state.current_player);
        state.phase = TurnPhase::GameOver;
    }
}

/// Check all win conditions.
fn check_win_conditions(state: &mut GameState) {
    if state.phase == TurnPhase::GameOver {
        return;
    }

    for i in 0..2 {
        // Win condition 1: accumulated enough points (3 points to win)
        if state.players[i].points >= PRIZE_COUNT as u32 {
            state.winner = Some(i);
            state.phase = TurnPhase::GameOver;
            return;
        }

        // Win condition 2: opponent has no Pokemon in play
        let opponent = 1 - i;
        if !state.players[opponent].has_pokemon_in_play() {
            state.winner = Some(i);
            state.phase = TurnPhase::GameOver;
            return;
        }
    }
}
