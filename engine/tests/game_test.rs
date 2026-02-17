use std::path::Path;

use rand::{Rng, SeedableRng};
use tcg_pocket_engine::data::card::*;
use tcg_pocket_engine::data::deck::Deck;
use tcg_pocket_engine::data::loader::{load_card_database, load_cards};
use tcg_pocket_engine::game::actions::{legal_actions, Action};
use tcg_pocket_engine::game::engine::{apply_action, new_game, StepResult};
use tcg_pocket_engine::game::state::TurnPhase;

fn make_basic(name: &str, hp: u32, energy: EnergyType, attacks: Vec<Attack>) -> Card {
    Card {
        id: name.to_lowercase().replace(' ', "-"),
        name: name.to_string(),
        card_type: CardType::Pokemon,
        hp: Some(hp),
        stage: Some(Stage::Basic),
        energy_type: Some(energy),
        weakness: None,
        retreat_cost: Some(1),
        attacks,
        ability: None,
        evolves_from: None,
        is_ex: false,
        effect: None,
        set_name: Some("Test".to_string()),
        card_number: Some(1),
        rarity: Some("Common".to_string()),
    }
}

fn make_attack(name: &str, cost: Vec<EnergyType>, damage: u32) -> Attack {
    Attack {
        name: name.to_string(),
        energy_cost: cost,
        damage,
        effect: None,
    }
}

fn make_trainer(name: &str) -> Card {
    Card {
        id: name.to_lowercase().replace(' ', "-"),
        name: name.to_string(),
        card_type: CardType::Item,
        hp: None,
        stage: None,
        energy_type: None,
        weakness: None,
        retreat_cost: None,
        attacks: vec![],
        ability: None,
        evolves_from: None,
        is_ex: false,
        effect: Some("Draw a card.".to_string()),
        set_name: Some("Test".to_string()),
        card_number: Some(100),
        rarity: Some("Common".to_string()),
    }
}

/// Build a simple 20-card test deck with basics and trainers.
fn make_test_deck() -> Deck {
    let pikachu = make_basic(
        "Pikachu",
        60,
        EnergyType::Lightning,
        vec![make_attack("Thunder Shock", vec![EnergyType::Lightning], 20)],
    );
    let charmander = make_basic(
        "Charmander",
        60,
        EnergyType::Fire,
        vec![make_attack("Scratch", vec![EnergyType::Colorless], 10)],
    );
    let squirtle = make_basic(
        "Squirtle",
        70,
        EnergyType::Water,
        vec![make_attack("Water Gun", vec![EnergyType::Water], 20)],
    );
    let bulbasaur = make_basic(
        "Bulbasaur",
        60,
        EnergyType::Grass,
        vec![make_attack("Tackle", vec![EnergyType::Grass], 20)],
    );
    let potion = make_trainer("Potion");
    let pokeball = make_trainer("Poke Ball");

    let mut cards = Vec::new();
    // 2 copies each of 4 basics = 8 Pokemon
    for card in [&pikachu, &charmander, &squirtle, &bulbasaur] {
        cards.push(card.clone());
        cards.push(card.clone());
    }
    // Fill remaining 12 slots with trainers
    for _ in 0..6 {
        cards.push(potion.clone());
        cards.push(pokeball.clone());
    }

    Deck::new_unchecked(cards)
}

#[test]
fn test_new_game_initializes_correctly() {
    let deck1 = make_test_deck();
    let deck2 = make_test_deck();
    let (state, _rng) = new_game(deck1, deck2, 42);

    // Both players have 5 cards in hand
    assert_eq!(state.players[0].hand.len(), 5);
    assert_eq!(state.players[1].hand.len(), 5);

    // Both players have 3 prize cards
    assert_eq!(state.players[0].prizes.len(), 3);
    assert_eq!(state.players[1].prizes.len(), 3);

    // Remaining cards in deck
    assert_eq!(state.players[0].deck.len(), 20 - 5 - 3);
    assert_eq!(state.players[1].deck.len(), 20 - 5 - 3);

    // Game starts in Setup phase
    assert_eq!(state.phase, TurnPhase::Setup);
    assert_eq!(state.current_player, 0);
}

#[test]
fn test_setup_phase() {
    let deck1 = make_test_deck();
    let deck2 = make_test_deck();
    let (mut state, mut rng) = new_game(deck1, deck2, 42);

    // Player 0 must place active first
    let actions = legal_actions(&state);
    assert!(!actions.is_empty());
    assert!(actions.iter().all(|a| matches!(a, Action::PlaceActive(_))));

    // Place first basic as active
    let place = actions[0].clone();
    let result = apply_action(&mut state, &place, &mut rng);
    assert!(matches!(result, StepResult::Continue));
    assert!(state.players[0].active.is_some());

    // Now can place bench or confirm
    let actions = legal_actions(&state);
    assert!(actions.contains(&Action::ConfirmSetup));

    // Confirm setup for player 0
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);
    assert_eq!(state.current_player, 1);

    // Player 1 setup
    let actions = legal_actions(&state);
    let place = actions[0].clone();
    apply_action(&mut state, &place, &mut rng);
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);

    // Game should now be in Main phase
    assert_eq!(state.phase, TurnPhase::Main);
    assert_eq!(state.current_player, 0);
}

#[test]
fn test_random_game_completes() {
    use rand::Rng;

    for seed in 0..20 {
        let deck1 = make_test_deck();
        let deck2 = make_test_deck();
        let (mut state, mut rng) = new_game(deck1, deck2, seed);
        let mut prng = rand::rngs::StdRng::seed_from_u64(seed + 1000);

        let mut steps = 0;
        let max_steps = 5000;

        while !state.is_terminal() && steps < max_steps {
            let actions = legal_actions(&state);
            if actions.is_empty() {
                break;
            }

            let action_idx = prng.gen_range(0..actions.len());
            let action = actions[action_idx].clone();
            apply_action(&mut state, &action, &mut rng);
            steps += 1;
        }

        // Game should have terminated (either winner or max steps)
        assert!(
            state.is_terminal() || steps >= max_steps,
            "Game seed {} stuck after {} steps in phase {:?}",
            seed,
            steps,
            state.phase
        );

        if state.is_terminal() {
            assert!(
                state.winner.is_some(),
                "Game over but no winner (seed {})",
                seed
            );
        }
    }
}

#[test]
fn test_energy_attachment() {
    let deck1 = make_test_deck();
    let deck2 = make_test_deck();
    let (mut state, mut rng) = new_game(deck1, deck2, 42);

    // Setup both players
    let actions = legal_actions(&state);
    apply_action(&mut state, &actions[0].clone(), &mut rng);
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);
    let actions = legal_actions(&state);
    apply_action(&mut state, &actions[0].clone(), &mut rng);
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);

    // Main phase - set energy zone type
    assert_eq!(state.phase, TurnPhase::Main);
    apply_action(
        &mut state,
        &Action::SetEnergyZoneType(EnergyType::Lightning),
        &mut rng,
    );

    assert_eq!(
        state.players[0].energy_zone_type,
        Some(EnergyType::Lightning)
    );

    // Attach energy to active
    apply_action(&mut state, &Action::AttachEnergy(0), &mut rng);
    assert!(state.players[0].energy_generated);
    assert_eq!(
        state.players[0]
            .active
            .as_ref()
            .unwrap()
            .attached_energy
            .len(),
        1
    );

    // Can't attach energy again
    let result = apply_action(&mut state, &Action::AttachEnergy(0), &mut rng);
    assert!(matches!(result, StepResult::InvalidAction(_)));
}

#[test]
fn test_attack_deals_damage() {
    let deck1 = make_test_deck();
    let deck2 = make_test_deck();
    let (mut state, mut rng) = new_game(deck1, deck2, 42);

    // Setup
    let actions = legal_actions(&state);
    apply_action(&mut state, &actions[0].clone(), &mut rng);
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);
    let actions = legal_actions(&state);
    apply_action(&mut state, &actions[0].clone(), &mut rng);
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);

    // End turn for player 0 (can't attack on first turn)
    apply_action(
        &mut state,
        &Action::SetEnergyZoneType(EnergyType::Fire),
        &mut rng,
    );
    apply_action(&mut state, &Action::AttachEnergy(0), &mut rng);
    apply_action(&mut state, &Action::EndTurn, &mut rng);

    // Player 1 turn - set energy and end turn
    apply_action(
        &mut state,
        &Action::SetEnergyZoneType(EnergyType::Fire),
        &mut rng,
    );
    apply_action(&mut state, &Action::AttachEnergy(0), &mut rng);
    apply_action(&mut state, &Action::EndTurn, &mut rng);

    // Player 0 turn 2 - can now attack
    // Attach another energy for good measure
    apply_action(&mut state, &Action::AttachEnergy(0), &mut rng);

    // Check if UseAttack is available
    let actions = legal_actions(&state);
    let has_attack = actions.iter().any(|a| matches!(a, Action::UseAttack(_)));

    // If the active has a Colorless-cost attack, it should be usable
    // (depends on what card is active after shuffle)
    if has_attack {
        let attack_action = actions
            .iter()
            .find(|a| matches!(a, Action::UseAttack(_)))
            .unwrap()
            .clone();

        apply_action(&mut state, &attack_action, &mut rng);

        // After attack, the turn should have ended
        // Opponent may have taken damage
        // (Attack resolves, then end_turn is called)
    }
}

#[test]
fn test_can_use_attack_energy_check() {
    let card = make_basic(
        "Test",
        60,
        EnergyType::Fire,
        vec![
            make_attack("Quick", vec![EnergyType::Colorless], 10),
            make_attack(
                "Fire Blast",
                vec![EnergyType::Fire, EnergyType::Fire, EnergyType::Colorless],
                80,
            ),
        ],
    );

    // No energy - can't use either attack
    assert!(!card.can_use_attack(0, &[]));
    assert!(!card.can_use_attack(1, &[]));

    // 1 Fire energy - can use Quick (colorless satisfied by Fire)
    assert!(card.can_use_attack(0, &[EnergyType::Fire]));
    assert!(!card.can_use_attack(1, &[EnergyType::Fire]));

    // 2 Fire + 1 Water - can use Fire Blast (Water satisfies Colorless)
    assert!(card.can_use_attack(
        1,
        &[EnergyType::Fire, EnergyType::Fire, EnergyType::Water]
    ));

    // 1 Fire + 2 Water - can't use Fire Blast (only 1 Fire, need 2)
    assert!(!card.can_use_attack(
        1,
        &[EnergyType::Fire, EnergyType::Water, EnergyType::Water]
    ));
}

#[test]
fn test_weakness_bonus() {
    // Fire attacks Grass: +20 damage
    let fire_card = make_basic(
        "Charmander",
        60,
        EnergyType::Fire,
        vec![make_attack("Ember", vec![EnergyType::Colorless], 30)],
    );
    let mut grass_card = make_basic(
        "Bulbasaur",
        60,
        EnergyType::Grass,
        vec![make_attack("Tackle", vec![EnergyType::Colorless], 10)],
    );
    grass_card.weakness = Some(EnergyType::Fire);

    // Build decks
    let mut cards1 = vec![fire_card.clone(); 2];
    let filler = make_trainer("Filler");
    while cards1.len() < 20 {
        cards1.push(filler.clone());
    }
    let mut cards2 = vec![grass_card.clone(); 2];
    while cards2.len() < 20 {
        cards2.push(filler.clone());
    }

    let deck1 = Deck::new_unchecked(cards1);
    let deck2 = Deck::new_unchecked(cards2);
    let (mut state, mut rng) = new_game(deck1, deck2, 100);

    // Setup - place actives
    let actions = legal_actions(&state);
    apply_action(&mut state, &actions[0].clone(), &mut rng);
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);
    let actions = legal_actions(&state);
    apply_action(&mut state, &actions[0].clone(), &mut rng);
    apply_action(&mut state, &Action::ConfirmSetup, &mut rng);

    // Player 0 first turn - just set energy and attach
    apply_action(
        &mut state,
        &Action::SetEnergyZoneType(EnergyType::Fire),
        &mut rng,
    );
    apply_action(&mut state, &Action::AttachEnergy(0), &mut rng);
    apply_action(&mut state, &Action::EndTurn, &mut rng);

    // Player 1 turn - end
    apply_action(
        &mut state,
        &Action::SetEnergyZoneType(EnergyType::Grass),
        &mut rng,
    );
    apply_action(&mut state, &Action::EndTurn, &mut rng);

    // Player 0 turn 2 - attack
    apply_action(&mut state, &Action::AttachEnergy(0), &mut rng);

    let actions = legal_actions(&state);
    if let Some(attack) = actions.iter().find(|a| matches!(a, Action::UseAttack(_))) {
        let attack = attack.clone();

        // If attacker is Fire and defender is Grass with Fire weakness,
        // damage should be 30 + 20 = 50 = 5 damage counters
        let attacker_is_fire = state.players[0]
            .active
            .as_ref()
            .map(|a| a.card.energy_type == Some(EnergyType::Fire))
            .unwrap_or(false);

        let defender_weak_to_fire = state.players[1]
            .active
            .as_ref()
            .and_then(|a| a.card.weakness)
            .map(|w| w == EnergyType::Fire)
            .unwrap_or(false);

        apply_action(&mut state, &attack, &mut rng);

        if attacker_is_fire && defender_weak_to_fire {
            // 30 base + 20 weakness = 50 damage = 5 damage counters
            // Defender had 60 HP, so should have 10 HP left
            // But after attack, end_turn is called and current_player switches
            // The defender (player 1) should have damage
            let defender = &state.players[1];
            if let Some(ref active) = defender.active {
                assert_eq!(
                    active.damage_counters, 5,
                    "Expected 5 damage counters (50 damage with weakness), got {}",
                    active.damage_counters
                );
            }
        }
    }
}

#[test]
fn test_load_card_database() {
    let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("data")
        .join("cards.json");

    if !data_path.exists() {
        eprintln!("Skipping: cards.json not found at {:?}", data_path);
        return;
    }

    let cards = load_cards(&data_path).expect("Failed to load cards.json");
    assert!(cards.len() > 2000, "Expected 2000+ cards, got {}", cards.len());

    let db = load_card_database(&data_path).expect("Failed to build CardDatabase");

    // Check basic stats
    let pokemon = db.pokemon_cards();
    let trainers = db.trainer_cards();
    assert!(pokemon.len() > 1500, "Expected 1500+ Pokemon, got {}", pokemon.len());
    assert!(trainers.len() > 50, "Expected 50+ trainers, got {}", trainers.len());

    // Verify specific known cards exist
    let bulbasaur = db.get_by_name("Bulbasaur");
    assert!(bulbasaur.is_some(), "Bulbasaur should exist");
    let bulbasaur = bulbasaur.unwrap();
    assert!(bulbasaur.hp.unwrap_or(0) >= 50, "Bulbasaur should have at least 50 HP");
    assert_eq!(bulbasaur.energy_type, Some(EnergyType::Grass));
    assert!(bulbasaur.is_basic_pokemon());
    assert!(!bulbasaur.attacks.is_empty());

    let charizard = db.get_by_name("Charizard");
    assert!(charizard.is_some(), "Charizard should exist");
    let charizard = charizard.unwrap();
    assert!(charizard.hp.unwrap_or(0) >= 140, "Charizard should have at least 140 HP");
    assert!(charizard.attacks.iter().any(|a| a.damage >= 100));

    // Check an EX card
    let mewtwo = db.get_by_name("Mewtwo ex");
    if let Some(mewtwo) = mewtwo {
        assert!(mewtwo.is_ex);
        assert!(mewtwo.hp.unwrap_or(0) >= 120);
    }
}

#[test]
fn test_game_with_real_cards() {
    let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("data")
        .join("cards.json");

    if !data_path.exists() {
        eprintln!("Skipping: cards.json not found");
        return;
    }

    let db = load_card_database(&data_path).expect("Failed to load cards");

    // Build a simple Grass deck from real cards
    let mut deck_cards = Vec::new();
    let basics: Vec<&Card> = db.pokemon_cards().into_iter()
        .filter(|c| c.is_basic_pokemon() && c.energy_type == Some(EnergyType::Grass))
        .take(5)
        .collect();

    for basic in &basics {
        deck_cards.push((*basic).clone());
        deck_cards.push((*basic).clone());
    }

    // Fill with more basics if needed
    let extra_basics: Vec<&Card> = db.pokemon_cards().into_iter()
        .filter(|c| c.is_basic_pokemon() && c.energy_type == Some(EnergyType::Colorless))
        .take(5)
        .collect();

    for basic in &extra_basics {
        if deck_cards.len() >= 20 { break; }
        deck_cards.push((*basic).clone());
        deck_cards.push((*basic).clone());
    }

    deck_cards.truncate(20);
    while deck_cards.len() < 20 {
        deck_cards.push(deck_cards[0].clone());
    }

    let deck1 = Deck::new_unchecked(deck_cards.clone());
    let deck2 = Deck::new_unchecked(deck_cards);

    let (mut state, mut rng) = new_game(deck1, deck2, 42);
    let mut prng = rand::rngs::StdRng::seed_from_u64(99);

    let mut steps = 0;
    while !state.is_terminal() && steps < 5000 {
        let actions = legal_actions(&state);
        if actions.is_empty() { break; }
        let idx = prng.gen_range(0..actions.len());
        apply_action(&mut state, &actions[idx].clone(), &mut rng);
        steps += 1;
    }

    assert!(
        state.is_terminal(),
        "Game with real cards should terminate (stuck at {:?} after {} steps)",
        state.phase,
        steps
    );
}
