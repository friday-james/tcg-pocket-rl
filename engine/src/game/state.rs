use crate::data::card::{Card, EnergyType};
use serde::{Deserialize, Serialize};

/// Maximum bench size in Pokemon TCG Pocket.
pub const MAX_BENCH: usize = 3;
/// Number of prize cards.
pub const PRIZE_COUNT: usize = 3;
/// Starting hand size.
pub const STARTING_HAND: usize = 5;

/// Current phase of a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnPhase {
    /// Initial setup: players place basic Pokemon.
    Setup,
    /// Player draws a card at start of turn.
    DrawCard,
    /// Main phase: play cards, attach energy, retreat, use abilities.
    Main,
    /// Attack phase: resolve the chosen attack.
    Attack,
    /// Between turns: resolve status conditions (poison, burn).
    BetweenTurns,
    /// A card effect requires a player decision.
    EffectChoice,
    /// Game is over.
    GameOver,
}

/// Status conditions a Pokemon can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusCondition {
    Poisoned,
    Burned,
    Asleep,
    Paralyzed,
    Confused,
}

/// A Pokemon that has been played to the field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayedCard {
    /// The card definition.
    pub card: Card,
    /// Energy attached to this Pokemon.
    pub attached_energy: Vec<EnergyType>,
    /// Damage counters on this Pokemon (each = 10 HP damage).
    pub damage_counters: u32,
    /// Active status conditions.
    pub status_conditions: Vec<StatusCondition>,
    /// The pre-evolution card (if this Pokemon evolved).
    pub evolved_from: Option<Box<PlayedCard>>,
    /// Turn number when this card was placed on the field.
    pub turn_played: u32,
    /// Attached Pokemon Tool card.
    pub tool: Option<Card>,
    /// Temporary flags for effects that last until end of turn / next turn.
    pub temp_flags: TempFlags,
}

/// Temporary effect flags on a played Pokemon.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TempFlags {
    /// Damage prevention for next attack received.
    pub prevent_damage_amount: u32,
    /// Cannot retreat this turn.
    pub cant_retreat: bool,
    /// Bonus damage on next attack.
    pub bonus_damage: u32,
    /// Already used ability this turn.
    pub used_ability: bool,
}

impl PlayedCard {
    pub fn new(card: Card, turn: u32) -> Self {
        PlayedCard {
            card,
            attached_energy: Vec::new(),
            damage_counters: 0,
            status_conditions: Vec::new(),
            evolved_from: None,
            turn_played: turn,
            tool: None,
            temp_flags: TempFlags::default(),
        }
    }

    /// Maximum HP for this Pokemon.
    pub fn max_hp(&self) -> u32 {
        self.card.hp.unwrap_or(0)
    }

    /// Current remaining HP.
    pub fn remaining_hp(&self) -> i32 {
        self.max_hp() as i32 - (self.damage_counters * 10) as i32
    }

    /// Whether this Pokemon has been knocked out.
    pub fn is_knocked_out(&self) -> bool {
        self.remaining_hp() <= 0
    }

    /// Whether this Pokemon can evolve (must have been in play for at least 1 turn).
    pub fn can_evolve(&self, current_turn: u32) -> bool {
        current_turn > self.turn_played
    }

    /// Whether this Pokemon has a specific status condition.
    pub fn has_status(&self, status: StatusCondition) -> bool {
        self.status_conditions.contains(&status)
    }

    /// Add a status condition, replacing incompatible ones.
    /// Asleep, Confused, Paralyzed are mutually exclusive.
    pub fn apply_status(&mut self, status: StatusCondition) {
        let exclusive = matches!(
            status,
            StatusCondition::Asleep | StatusCondition::Confused | StatusCondition::Paralyzed
        );
        if exclusive {
            self.status_conditions
                .retain(|s| !matches!(s, StatusCondition::Asleep | StatusCondition::Confused | StatusCondition::Paralyzed));
        }
        if !self.status_conditions.contains(&status) {
            self.status_conditions.push(status);
        }
    }

    /// Clear all status conditions (e.g., when evolving or retreating).
    pub fn clear_status(&mut self) {
        self.status_conditions.clear();
    }

    /// Clear end-of-turn temporary flags.
    pub fn clear_temp_flags(&mut self) {
        self.temp_flags = TempFlags::default();
    }
}

/// State for one player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    /// Cards remaining in the deck.
    pub deck: Vec<Card>,
    /// Cards in hand.
    pub hand: Vec<Card>,
    /// Active Pokemon (if any).
    pub active: Option<PlayedCard>,
    /// Bench Pokemon (up to MAX_BENCH).
    pub bench: Vec<Option<PlayedCard>>,
    /// Discard pile.
    pub discard: Vec<Card>,
    /// Prize cards (face down).
    pub prizes: Vec<Card>,
    /// Current energy zone type selection.
    pub energy_zone_type: Option<EnergyType>,
    /// Whether energy has been generated this turn.
    pub energy_generated: bool,
    /// Whether a Supporter has been played this turn.
    pub supporter_played: bool,
    /// Whether the player has retreated this turn.
    pub retreated_this_turn: bool,
    /// Prize cards taken (starts at 0, win at PRIZE_COUNT).
    pub prizes_taken: u32,
    /// Points accumulated (2 for EX KO, 1 for regular KO).
    pub points: u32,
}

impl PlayerState {
    pub fn new() -> Self {
        PlayerState {
            deck: Vec::new(),
            hand: Vec::new(),
            active: None,
            bench: vec![None; MAX_BENCH],
            discard: Vec::new(),
            prizes: Vec::new(),
            energy_zone_type: None,
            energy_generated: false,
            supporter_played: false,
            retreated_this_turn: false,
            prizes_taken: 0,
            points: 0,
        }
    }

    /// Count occupied bench slots.
    pub fn bench_count(&self) -> usize {
        self.bench.iter().filter(|b| b.is_some()).count()
    }

    /// Find an empty bench slot index.
    pub fn find_empty_bench(&self) -> Option<usize> {
        self.bench.iter().position(|b| b.is_none())
    }

    /// Get a reference to a Pokemon by board position (0 = active, 1-3 = bench).
    pub fn get_pokemon(&self, position: usize) -> Option<&PlayedCard> {
        if position == 0 {
            self.active.as_ref()
        } else {
            self.bench.get(position - 1).and_then(|b| b.as_ref())
        }
    }

    /// Get a mutable reference to a Pokemon by board position.
    pub fn get_pokemon_mut(&mut self, position: usize) -> Option<&mut PlayedCard> {
        if position == 0 {
            self.active.as_mut()
        } else {
            self.bench
                .get_mut(position - 1)
                .and_then(|b| b.as_mut())
        }
    }

    /// All Pokemon in play (active + bench), with their board positions.
    pub fn all_pokemon(&self) -> Vec<(usize, &PlayedCard)> {
        let mut result = Vec::new();
        if let Some(ref active) = self.active {
            result.push((0, active));
        }
        for (i, slot) in self.bench.iter().enumerate() {
            if let Some(ref pokemon) = slot {
                result.push((i + 1, pokemon));
            }
        }
        result
    }

    /// Check if the player has any Pokemon in play.
    pub fn has_pokemon_in_play(&self) -> bool {
        self.active.is_some() || self.bench.iter().any(|b| b.is_some())
    }

    /// Whether hand has any basic Pokemon.
    pub fn has_basic_in_hand(&self) -> bool {
        self.hand.iter().any(|c| c.is_basic_pokemon())
    }

    /// Reset per-turn state.
    pub fn start_turn(&mut self) {
        self.energy_generated = false;
        self.supporter_played = false;
        self.retreated_this_turn = false;

        // Clear temp flags on all Pokemon
        if let Some(ref mut active) = self.active {
            active.clear_temp_flags();
        }
        for slot in &mut self.bench {
            if let Some(ref mut pokemon) = slot {
                pokemon.clear_temp_flags();
            }
        }
    }
}

/// Complete game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    /// Player states (index 0 and 1).
    pub players: [PlayerState; 2],
    /// Which player's turn it is (0 or 1).
    pub current_player: usize,
    /// Global turn number (increments each time either player takes a turn).
    pub turn_number: u32,
    /// Current phase within the turn.
    pub phase: TurnPhase,
    /// Winner (if game is over).
    pub winner: Option<usize>,
    /// Whether first player's first turn (cannot attack on first turn).
    pub first_turn: bool,
    /// Pending effect choices (for effects that require player decisions).
    pub pending_choice: Option<PendingChoice>,
}

/// A pending choice the current player must resolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingChoice {
    /// Choose a bench Pokemon to promote to active (after KO).
    PromoteFromBench,
    /// Choose a target for an effect.
    ChooseTarget {
        valid_targets: Vec<usize>,
        description: String,
    },
    /// Choose cards from hand to discard.
    DiscardFromHand {
        count: usize,
        description: String,
    },
    /// Choose an energy to discard from a Pokemon.
    DiscardEnergy {
        pokemon_position: usize,
        count: usize,
    },
}

impl GameState {
    /// Get the current player's state.
    pub fn current(&self) -> &PlayerState {
        &self.players[self.current_player]
    }

    /// Get the current player's state mutably.
    pub fn current_mut(&mut self) -> &mut PlayerState {
        &mut self.players[self.current_player]
    }

    /// Get the opponent's state.
    pub fn opponent(&self) -> &PlayerState {
        &self.players[1 - self.current_player]
    }

    /// Get the opponent's state mutably.
    pub fn opponent_mut(&mut self) -> &mut PlayerState {
        &mut self.players[1 - self.current_player]
    }

    /// Check if the game is over.
    pub fn is_terminal(&self) -> bool {
        self.phase == TurnPhase::GameOver
    }
}
