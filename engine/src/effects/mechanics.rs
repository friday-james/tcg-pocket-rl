use crate::data::card::EnergyType;
use crate::game::state::StatusCondition;
use serde::{Deserialize, Serialize};

/// Target for an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// The attacking/acting Pokemon.
    This,
    /// The opponent's active Pokemon.
    OpponentActive,
    /// All opponent's bench Pokemon.
    OpponentBench,
    /// Opponent chooses one of their bench Pokemon.
    OpponentChooseBench,
    /// Player chooses one of opponent's bench Pokemon.
    ChooseOpponentBench,
    /// Player chooses one of their own bench Pokemon.
    ChooseOwnBench,
    /// All of the player's Pokemon.
    AllOwn,
}

/// Condition for conditional damage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageCondition {
    /// Target has damage on it.
    TargetHasDamage,
    /// Per energy of a specific type attached to this Pokemon.
    PerEnergyAttached(EnergyType),
    /// Per damage counter on this Pokemon.
    PerDamageOnSelf,
    /// Per bench Pokemon (own).
    PerOwnBench,
    /// Per bench Pokemon (opponent).
    PerOpponentBench,
    /// Coin flip: heads for success.
    CoinFlipHeads,
}

/// Structured representation of a card effect/mechanic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Mechanic {
    // === Damage ===
    /// Deal fixed damage.
    Damage(u32),
    /// Coin flip: heads = damage, tails = 0.
    DamageOnCoinFlip(u32),
    /// Flip N coins, deal damage per heads.
    DamagePerCoinFlip { damage_per_heads: u32, flips: u32 },
    /// Base damage + bonus if condition met.
    ConditionalDamage {
        base: u32,
        bonus: u32,
        condition: DamageCondition,
    },
    /// Damage multiplied by some count.
    DamageMultiplied {
        damage_per: u32,
        condition: DamageCondition,
    },
    /// Deal damage to bench Pokemon.
    BenchDamage { damage: u32, target: Target },

    // === Healing ===
    /// Heal HP from a target.
    Heal { amount: u32, target: Target },

    // === Status ===
    /// Apply a status condition.
    ApplyStatus(StatusCondition, Target),
    /// Coin flip to apply status.
    ApplyStatusOnCoinFlip(StatusCondition, Target),

    // === Energy ===
    /// Discard energy from a target.
    DiscardEnergy {
        count: u32,
        energy_type: Option<EnergyType>,
        target: Target,
    },
    /// Move energy between Pokemon.
    MoveEnergy {
        count: u32,
        from: Target,
        to: Target,
    },

    // === Cards ===
    /// Draw cards.
    DrawCards(u32),
    /// Opponent discards cards.
    OpponentDiscard(u32),
    /// Search deck for a card.
    SearchDeck { criteria: String },

    // === Board manipulation ===
    /// Switch opponent's active with one of their bench.
    SwitchOpponentActive,
    /// Prevent damage to this Pokemon until next turn.
    PreventDamage(u32),
    /// This Pokemon can't be damaged next turn.
    Invulnerable,
    /// Do damage to self.
    SelfDamage(u32),

    // === Special ===
    /// Custom effect that needs special handling.
    Custom(String),
}
