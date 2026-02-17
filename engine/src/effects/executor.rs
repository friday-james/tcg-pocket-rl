use super::mechanics::*;
use super::registry::EffectRegistry;
use crate::game::rng::GameRng;
use crate::game::state::*;

/// Execute the effects of an attack.
/// Returns a damage modifier: Some(new_damage) if effects change the damage, None otherwise.
pub fn execute_attack_effects(
    state: &mut GameState,
    registry: &EffectRegistry,
    card_id: &str,
    attack_idx: usize,
    rng: &mut GameRng,
) -> Option<u32> {
    let effects = registry.get_attack_effects(card_id, attack_idx).to_vec();
    let mut damage_override: Option<u32> = None;

    // Split into pre-damage and post-damage effects
    for mechanic in &effects {
        match mechanic {
            // Pre-damage effects: modify the damage value
            Mechanic::NoDamageOnTails => {
                if !rng.coin_flip() {
                    damage_override = Some(0);
                }
            }
            Mechanic::DamageOnCoinFlip(_) => {
                if !rng.coin_flip() {
                    damage_override = Some(0);
                }
            }
            Mechanic::DamagePerCoinFlip {
                damage_per_heads,
                flips,
            } => {
                let heads = rng.coin_flips(*flips);
                damage_override = Some(heads * damage_per_heads);
            }
            Mechanic::ConditionalDamage {
                base: _,
                bonus,
                condition,
            } => {
                if check_condition(state, condition, rng) {
                    let current_override = damage_override.unwrap_or(0);
                    damage_override = Some(current_override + bonus);
                }
            }
            Mechanic::DamageMultiplied {
                damage_per,
                condition,
            } => {
                let count = count_condition(state, condition, rng);
                damage_override = Some(count * damage_per);
            }
            Mechanic::DamagePerEnergy { per, energy_type } => {
                let current = state.current_player;
                if let Some(ref active) = state.players[current].active {
                    let count = match energy_type {
                        Some(et) => active
                            .attached_energy
                            .iter()
                            .filter(|e| *e == et)
                            .count() as u32,
                        None => active.attached_energy.len() as u32,
                    };
                    damage_override = Some(count * per);
                }
            }
            Mechanic::DamagePerBench { per, own } => {
                let player_idx = if *own {
                    state.current_player
                } else {
                    1 - state.current_player
                };
                let count = state.players[player_idx].bench_count() as u32;
                damage_override = Some(count * per);
            }
            Mechanic::DamagePerDamageCounter { per } => {
                let current = state.current_player;
                if let Some(ref active) = state.players[current].active {
                    damage_override = Some(active.damage_counters * per);
                }
            }
            // Post-damage effects are handled below
            _ => {}
        }
    }

    // Execute post-damage effects
    for mechanic in &effects {
        match mechanic {
            // Skip pre-damage effects (already handled)
            Mechanic::NoDamageOnTails
            | Mechanic::DamageOnCoinFlip(_)
            | Mechanic::DamagePerCoinFlip { .. }
            | Mechanic::ConditionalDamage { .. }
            | Mechanic::DamageMultiplied { .. }
            | Mechanic::DamagePerEnergy { .. }
            | Mechanic::DamagePerBench { .. }
            | Mechanic::DamagePerDamageCounter { .. } => {}
            // Execute all other effects
            other => {
                execute_mechanic(state, other, rng);
            }
        }
    }

    damage_override
}

/// Execute a single mechanic on the game state.
pub fn execute_mechanic(state: &mut GameState, mechanic: &Mechanic, rng: &mut GameRng) {
    let current = state.current_player;
    let opponent = 1 - current;

    match mechanic {
        // ================================================================
        // HEALING
        // ================================================================
        Mechanic::Heal { amount, target } => {
            apply_to_target(state, *target, |pokemon| {
                let heal = (*amount / 10).min(pokemon.damage_counters);
                pokemon.damage_counters -= heal;
            });
        }

        Mechanic::FullHeal { target } => {
            apply_to_target(state, *target, |pokemon| {
                pokemon.damage_counters = 0;
            });
        }

        // ================================================================
        // STATUS CONDITIONS
        // ================================================================
        Mechanic::ApplyStatus(status, target) => {
            apply_to_target(state, *target, |pokemon| {
                pokemon.apply_status(*status);
            });
        }

        Mechanic::ApplyStatusOnCoinFlip(status, target) => {
            if rng.coin_flip() {
                apply_to_target(state, *target, |pokemon| {
                    pokemon.apply_status(*status);
                });
            }
        }

        Mechanic::CureStatus { target } => {
            apply_to_target(state, *target, |pokemon| {
                pokemon.clear_status();
            });
        }

        // ================================================================
        // ENERGY MANIPULATION
        // ================================================================
        Mechanic::DiscardEnergy {
            count,
            energy_type,
            target,
        } => {
            apply_to_target(state, *target, |pokemon| {
                for _ in 0..*count {
                    if let Some(et) = energy_type {
                        if let Some(pos) = pokemon.attached_energy.iter().position(|e| e == et) {
                            pokemon.attached_energy.remove(pos);
                        }
                    } else {
                        pokemon.attached_energy.pop();
                    }
                }
            });
        }

        Mechanic::DiscardAllEnergy { target } => {
            apply_to_target(state, *target, |pokemon| {
                pokemon.attached_energy.clear();
            });
        }

        Mechanic::DiscardOpponentEnergy { count } => {
            if let Some(ref mut active) = state.players[opponent].active {
                for _ in 0..*count {
                    active.attached_energy.pop();
                }
            }
        }

        Mechanic::MoveEnergy { count, from, to } => {
            // Simplified: move energy from one target to another
            let from_target = *from;
            let to_target = *to;
            let mut energies_to_move = Vec::new();

            // Collect energy to move
            apply_to_target(state, from_target, |pokemon| {
                for _ in 0..*count {
                    if let Some(e) = pokemon.attached_energy.pop() {
                        energies_to_move.push(e);
                    }
                }
            });

            // Attach to target
            for energy in energies_to_move {
                apply_to_target(state, to_target, |pokemon| {
                    pokemon.attached_energy.push(energy);
                });
            }
        }

        Mechanic::MoveAllEnergy {
            energy_type,
            from,
            to,
        } => {
            let from_target = *from;
            let to_target = *to;
            let mut energies_to_move = Vec::new();

            // Collect matching energy
            apply_to_target(state, from_target, |pokemon| {
                if let Some(et) = energy_type {
                    pokemon.attached_energy.retain(|e| {
                        if *e == *et {
                            energies_to_move.push(*e);
                            false
                        } else {
                            true
                        }
                    });
                } else {
                    energies_to_move.append(&mut pokemon.attached_energy);
                }
            });

            for energy in energies_to_move {
                apply_to_target(state, to_target, |pokemon| {
                    pokemon.attached_energy.push(energy);
                });
            }
        }

        Mechanic::AttachEnergyFromDiscard {
            energy_type,
            count,
            target,
        } => {
            // Find energy in discard and attach to target
            let player = &mut state.players[current];
            let mut attached = 0u32;
            let et = *energy_type;

            // Simulate: just add the energy type directly to the target
            // (In a real game you'd remove from discard, but energy isn't tracked as cards in discard)
            if let Some(ref mut active) = player.active {
                if matches!(target, Target::This | Target::OwnActive) {
                    if let Some(energy) = et {
                        for _ in 0..*count {
                            active.attached_energy.push(energy);
                            attached += 1;
                        }
                    }
                }
            }
            let _ = attached;
        }

        Mechanic::AttachEnergyFromZone {
            energy_type,
            count,
            target,
        } => {
            // Attach energy from zone (simplified: just add the energy)
            let et = *energy_type;
            apply_to_target(state, *target, |pokemon| {
                for _ in 0..*count {
                    pokemon.attached_energy.push(et);
                }
            });
        }

        // ================================================================
        // CARD MANIPULATION
        // ================================================================
        Mechanic::DrawCards(count) => {
            for _ in 0..*count {
                if let Some(card) = state.players[current].deck.pop() {
                    state.players[current].hand.push(card);
                }
            }
        }

        Mechanic::OpponentDiscard(count) => {
            // Opponent discards random cards from hand
            for _ in 0..*count {
                if !state.players[opponent].hand.is_empty() {
                    let idx = rng.gen_range(0, state.players[opponent].hand.len());
                    let card = state.players[opponent].hand.remove(idx);
                    state.players[opponent].discard.push(card);
                }
            }
        }

        Mechanic::SearchDeckRandom { count } => {
            // Put random matching card from deck into hand
            for _ in 0..*count {
                if !state.players[current].deck.is_empty() {
                    let idx = rng.gen_range(0, state.players[current].deck.len());
                    let card = state.players[current].deck.remove(idx);
                    state.players[current].hand.push(card);
                }
            }
        }

        Mechanic::ShuffleHandDraw { count } => {
            // Shuffle hand into deck, then draw N
            let player = &mut state.players[current];
            player.deck.append(&mut player.hand);
            rng.shuffle(&mut player.deck);
            for _ in 0..*count {
                if let Some(card) = player.deck.pop() {
                    player.hand.push(card);
                }
            }
        }

        Mechanic::OpponentShuffleHandDraw { count } => {
            let player = &mut state.players[opponent];
            player.deck.append(&mut player.hand);
            rng.shuffle(&mut player.deck);
            for _ in 0..*count {
                if let Some(card) = player.deck.pop() {
                    player.hand.push(card);
                }
            }
        }

        Mechanic::BothShuffleHandDraw => {
            // Each player shuffles hand, draws same number they had
            for p in 0..2 {
                let hand_size = state.players[p].hand.len();
                state.players[p].deck.append(&mut state.players[p].hand);
                rng.shuffle(&mut state.players[p].deck);
                for _ in 0..hand_size {
                    if let Some(card) = state.players[p].deck.pop() {
                        state.players[p].hand.push(card);
                    }
                }
            }
        }

        Mechanic::RecoverFromDiscard { count } => {
            // Put random card from discard into hand
            for _ in 0..*count {
                if !state.players[current].discard.is_empty() {
                    let idx = rng.gen_range(0, state.players[current].discard.len());
                    let card = state.players[current].discard.remove(idx);
                    state.players[current].hand.push(card);
                }
            }
        }

        Mechanic::DiscardFromHand { count } => {
            // Discard random cards from own hand
            for _ in 0..*count {
                if !state.players[current].hand.is_empty() {
                    let idx = rng.gen_range(0, state.players[current].hand.len());
                    let card = state.players[current].hand.remove(idx);
                    state.players[current].discard.push(card);
                }
            }
        }

        Mechanic::PeekDeck { .. } => {
            // Information only - no state change needed for RL
        }

        Mechanic::SearchDeck { .. } => {
            // Simplified as SearchDeckRandom
        }

        // ================================================================
        // BOARD MANIPULATION
        // ================================================================
        Mechanic::SwitchOpponentActive => {
            // Force opponent to switch active with a bench Pokemon
            let bench_pokemon: Vec<usize> = state.players[opponent]
                .bench
                .iter()
                .enumerate()
                .filter(|(_, b)| b.is_some())
                .map(|(i, _)| i)
                .collect();

            if !bench_pokemon.is_empty() {
                // Auto-select random bench Pokemon for simplicity
                let idx = bench_pokemon[rng.gen_range(0, bench_pokemon.len())];
                let bench = state.players[opponent].bench[idx].take();
                let active = state.players[opponent].active.take();
                state.players[opponent].active = bench;
                state.players[opponent].bench[idx] = active;
            }
        }

        Mechanic::SwitchOwnActive => {
            // Switch own active with a bench Pokemon
            let bench_pokemon: Vec<usize> = state.players[current]
                .bench
                .iter()
                .enumerate()
                .filter(|(_, b)| b.is_some())
                .map(|(i, _)| i)
                .collect();

            if !bench_pokemon.is_empty() {
                let idx = bench_pokemon[rng.gen_range(0, bench_pokemon.len())];
                let bench = state.players[current].bench[idx].take();
                let active = state.players[current].active.take();
                state.players[current].active = bench;
                state.players[current].bench[idx] = active;
            }
        }

        Mechanic::BounceToHand { target } => {
            match target {
                Target::OwnActive => {
                    if let Some(pokemon) = state.players[current].active.take() {
                        state.players[current].hand.push(pokemon.card);
                        // Promote from bench if possible
                        if state.players[current].bench_count() > 0 {
                            for i in 0..MAX_BENCH {
                                if state.players[current].bench[i].is_some() {
                                    state.players[current].active =
                                        state.players[current].bench[i].take();
                                    break;
                                }
                            }
                        }
                    }
                }
                Target::This => {
                    if let Some(pokemon) = state.players[current].active.take() {
                        state.players[current].hand.push(pokemon.card);
                        if state.players[current].bench_count() > 0 {
                            for i in 0..MAX_BENCH {
                                if state.players[current].bench[i].is_some() {
                                    state.players[current].active =
                                        state.players[current].bench[i].take();
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Mechanic::ShuffleIntoDeck { target } => {
            match target {
                Target::This | Target::OwnActive => {
                    if let Some(pokemon) = state.players[current].active.take() {
                        state.players[current].deck.push(pokemon.card);
                        rng.shuffle(&mut state.players[current].deck);
                        if state.players[current].bench_count() > 0 {
                            for i in 0..MAX_BENCH {
                                if state.players[current].bench[i].is_some() {
                                    state.players[current].active =
                                        state.players[current].bench[i].take();
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Mechanic::PutOnOpponentBench => {
            // Put a Basic from opponent's discard onto their bench
            if let Some(slot) = state.players[opponent].find_empty_bench() {
                if let Some(idx) = state.players[opponent]
                    .discard
                    .iter()
                    .position(|c| c.is_basic_pokemon())
                {
                    let card = state.players[opponent].discard.remove(idx);
                    state.players[opponent].bench[slot] =
                        Some(PlayedCard::new(card, state.turn_number));
                }
            }
        }

        Mechanic::CantRetreat => {
            if let Some(ref mut active) = state.players[opponent].active {
                active.temp_flags.cant_retreat = true;
            }
        }

        Mechanic::CantAttackNextTurn => {
            // TODO: need a flag for "can't attack next turn"
        }

        Mechanic::EvolveFromDeck | Mechanic::EvolveSkipStage => {
            // Complex effects - would need PendingChoice for target selection
            // Simplified: no-op for now
        }

        // ================================================================
        // DAMAGE BOOST / REDUCTION
        // ================================================================
        Mechanic::DamageBoost { amount } => {
            // Set bonus damage on active Pokemon for this turn
            if let Some(ref mut active) = state.players[current].active {
                active.temp_flags.bonus_damage += amount;
            }
        }

        Mechanic::DamageReduction { amount } => {
            // Set damage prevention on all own Pokemon for opponent's next turn
            if let Some(ref mut active) = state.players[current].active {
                active.temp_flags.prevent_damage_amount += amount;
            }
            for slot in &mut state.players[current].bench {
                if let Some(ref mut pokemon) = slot {
                    pokemon.temp_flags.prevent_damage_amount += amount;
                }
            }
        }

        Mechanic::RetreatCostReduction { .. } => {
            // Handled in legal_actions_main (retreat cost calculation)
            // Store as a turn flag - simplified by just noting it's applied
        }

        Mechanic::SurviveKO => {
            // TODO: need a flag on Pokemon for "survive KO with 10 HP"
        }

        Mechanic::GuaranteedHeads => {
            // Set a flag so next coin flip is heads
            rng.set_guaranteed_heads(true);
        }

        Mechanic::MoveDamage { amount, from, to } => {
            let from_target = *from;
            let to_target = *to;
            let counters_to_move = *amount / 10;

            // Remove damage from source
            let mut actually_moved = 0u32;
            apply_to_target(state, from_target, |pokemon| {
                actually_moved = counters_to_move.min(pokemon.damage_counters);
                pokemon.damage_counters -= actually_moved;
            });

            // Add damage to target
            if actually_moved > 0 {
                apply_to_target(state, to_target, |pokemon| {
                    pokemon.damage_counters += actually_moved;
                });
            }
        }

        Mechanic::EndTurn => {
            // Force end of turn - handled by caller
        }

        // ================================================================
        // SELF DAMAGE
        // ================================================================
        Mechanic::SelfDamage(damage) => {
            if let Some(ref mut active) = state.players[current].active {
                active.damage_counters += damage / 10;
            }
        }

        // ================================================================
        // DAMAGE PREVENTION
        // ================================================================
        Mechanic::PreventDamage(amount) => {
            if let Some(ref mut active) = state.players[current].active {
                active.temp_flags.prevent_damage_amount = *amount;
            }
        }

        Mechanic::Invulnerable => {
            if let Some(ref mut active) = state.players[current].active {
                active.temp_flags.prevent_damage_amount = 9999;
            }
        }

        // ================================================================
        // BENCH DAMAGE
        // ================================================================
        Mechanic::BenchDamage { damage, target } => match target {
            Target::OpponentBench => {
                for slot in &mut state.players[opponent].bench {
                    if let Some(ref mut pokemon) = slot {
                        pokemon.damage_counters += damage / 10;
                    }
                }
            }
            Target::ChooseOpponentBench => {
                // Auto-select random bench Pokemon
                let bench_pokemon: Vec<usize> = state.players[opponent]
                    .bench
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.is_some())
                    .map(|(i, _)| i)
                    .collect();
                if !bench_pokemon.is_empty() {
                    let idx = bench_pokemon[rng.gen_range(0, bench_pokemon.len())];
                    if let Some(ref mut pokemon) = state.players[opponent].bench[idx] {
                        pokemon.damage_counters += damage / 10;
                    }
                }
            }
            _ => {}
        },

        // ================================================================
        // PASSIVE EFFECTS (no-op when executed directly; checked at game events)
        // ================================================================
        Mechanic::PassiveHPBoost { .. }
        | Mechanic::PassiveDamageReduction { .. }
        | Mechanic::PassiveDamageBoost { .. }
        | Mechanic::PassiveRetreatReduction { .. }
        | Mechanic::PassiveAttackCostIncrease { .. }
        | Mechanic::RetaliationDamage { .. }
        | Mechanic::RetaliationStatus { .. }
        | Mechanic::OnKODamage { .. }
        | Mechanic::OnKOBounceToHand
        | Mechanic::OnKOMoveEnergy { .. }
        | Mechanic::OnKODrawCard
        | Mechanic::HealBetweenTurns { .. }
        | Mechanic::CureStatusBetweenTurns
        | Mechanic::StatusImmunity
        | Mechanic::UsePreEvoAttacks
        | Mechanic::DamageBoostPerPoint { .. } => {
            // These are passive effects checked at specific game events,
            // not executed directly when the card is played.
        }

        Mechanic::Damage(_) => {
            // Base damage is handled by resolve_attack, not here
        }

        Mechanic::Custom(_) | Mechanic::NoOp => {
            // Custom effects and no-ops
        }

        // Pre-damage effects are handled in execute_attack_effects
        Mechanic::NoDamageOnTails
        | Mechanic::DamageOnCoinFlip(_)
        | Mechanic::DamagePerCoinFlip { .. }
        | Mechanic::ConditionalDamage { .. }
        | Mechanic::DamageMultiplied { .. }
        | Mechanic::DamagePerEnergy { .. }
        | Mechanic::DamagePerBench { .. }
        | Mechanic::DamagePerDamageCounter { .. } => {}
    }
}

/// Check if a condition is met (returns true/false).
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
        DamageCondition::PerDamageOnSelf => state.players[current]
            .active
            .as_ref()
            .map_or(false, |p| p.damage_counters > 0),
        DamageCondition::PerEnergyAttached(_) => state.players[current]
            .active
            .as_ref()
            .map_or(false, |p| !p.attached_energy.is_empty()),
        DamageCondition::PerAnyEnergyAttached => state.players[current]
            .active
            .as_ref()
            .map_or(false, |p| !p.attached_energy.is_empty()),
    }
}

/// Count for a condition (for DamageMultiplied).
fn count_condition(state: &GameState, condition: &DamageCondition, rng: &mut GameRng) -> u32 {
    let current = state.current_player;
    let opponent = 1 - current;

    match condition {
        DamageCondition::PerOwnBench => state.players[current].bench_count() as u32,
        DamageCondition::PerOpponentBench => state.players[opponent].bench_count() as u32,
        DamageCondition::PerDamageOnSelf => state.players[current]
            .active
            .as_ref()
            .map_or(0, |p| p.damage_counters),
        DamageCondition::PerEnergyAttached(et) => state.players[current]
            .active
            .as_ref()
            .map_or(0, |p| {
                p.attached_energy.iter().filter(|e| *e == et).count() as u32
            }),
        DamageCondition::PerAnyEnergyAttached => state.players[current]
            .active
            .as_ref()
            .map_or(0, |p| p.attached_energy.len() as u32),
        DamageCondition::CoinFlipHeads => {
            if rng.coin_flip() {
                1
            } else {
                0
            }
        }
        DamageCondition::TargetHasDamage => {
            if state.players[opponent]
                .active
                .as_ref()
                .map_or(false, |p| p.damage_counters > 0)
            {
                1
            } else {
                0
            }
        }
    }
}

/// Apply an effect to a target Pokemon.
fn apply_to_target<F>(state: &mut GameState, target: Target, mut f: F)
where
    F: FnMut(&mut PlayedCard),
{
    let current = state.current_player;
    let opponent = 1 - current;

    match target {
        Target::This | Target::OwnActive => {
            if let Some(ref mut pokemon) = state.players[current].active {
                f(pokemon);
            }
        }
        Target::OpponentActive => {
            if let Some(ref mut pokemon) = state.players[opponent].active {
                f(pokemon);
            }
        }
        Target::OpponentBench => {
            for slot in &mut state.players[opponent].bench {
                if let Some(ref mut pokemon) = slot {
                    f(pokemon);
                }
            }
        }
        Target::ChooseOwnBench => {
            // Auto-select first bench Pokemon
            for slot in &mut state.players[current].bench {
                if let Some(ref mut pokemon) = slot {
                    f(pokemon);
                    break;
                }
            }
        }
        Target::ChooseOpponentBench => {
            // Auto-select first opponent bench Pokemon
            for slot in &mut state.players[opponent].bench {
                if let Some(ref mut pokemon) = slot {
                    f(pokemon);
                    break;
                }
            }
        }
        Target::OpponentChooseBench => {
            // Auto-select first opponent bench Pokemon
            for slot in &mut state.players[opponent].bench {
                if let Some(ref mut pokemon) = slot {
                    f(pokemon);
                    break;
                }
            }
        }
        Target::ChooseOwn => {
            // Apply to active Pokemon by default
            if let Some(ref mut pokemon) = state.players[current].active {
                f(pokemon);
            }
        }
        Target::AllOwn => {
            if let Some(ref mut active) = state.players[current].active {
                f(active);
            }
            for slot in &mut state.players[current].bench {
                if let Some(ref mut pokemon) = slot {
                    f(pokemon);
                }
            }
        }
    }
}
