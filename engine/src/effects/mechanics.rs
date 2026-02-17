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
    /// Player chooses one of their own Pokemon (active or bench).
    ChooseOwn,
    /// All of the player's Pokemon.
    AllOwn,
    /// The player's active Pokemon.
    OwnActive,
}

/// Condition for conditional damage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageCondition {
    /// Target has damage on it.
    TargetHasDamage,
    /// Per energy of a specific type attached to this Pokemon.
    PerEnergyAttached(EnergyType),
    /// Per any energy attached to this Pokemon.
    PerAnyEnergyAttached,
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
    // ================================================================
    // DAMAGE (attack effects that modify or deal damage)
    // ================================================================
    /// Deal fixed damage.
    Damage(u32),
    /// Coin flip: heads = damage, tails = 0 (attack does nothing).
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
    /// Damage scaled by energy attached (e.g., "30 for each Water Energy").
    DamagePerEnergy {
        per: u32,
        energy_type: Option<EnergyType>,
    },
    /// Damage scaled by bench count (e.g., "20 for each Benched Pokemon").
    DamagePerBench { per: u32, own: bool },
    /// Damage scaled by damage counters on self.
    DamagePerDamageCounter { per: u32 },
    /// Coin flip: tails = attack does nothing (damage zeroed).
    NoDamageOnTails,

    // ================================================================
    // HEALING
    // ================================================================
    /// Heal HP from a target.
    Heal { amount: u32, target: Target },
    /// Heal all damage from target.
    FullHeal { target: Target },

    // ================================================================
    // STATUS CONDITIONS
    // ================================================================
    /// Apply a status condition.
    ApplyStatus(StatusCondition, Target),
    /// Coin flip to apply status.
    ApplyStatusOnCoinFlip(StatusCondition, Target),
    /// Remove all status conditions from target.
    CureStatus { target: Target },

    // ================================================================
    // ENERGY MANIPULATION
    // ================================================================
    /// Discard energy from a target.
    DiscardEnergy {
        count: u32,
        energy_type: Option<EnergyType>,
        target: Target,
    },
    /// Discard all energy from a target.
    DiscardAllEnergy { target: Target },
    /// Discard energy from opponent's active Pokemon.
    DiscardOpponentEnergy { count: u32 },
    /// Move energy between Pokemon.
    MoveEnergy {
        count: u32,
        from: Target,
        to: Target,
    },
    /// Move all energy of a type from one target to another.
    MoveAllEnergy {
        energy_type: Option<EnergyType>,
        from: Target,
        to: Target,
    },
    /// Attach energy from discard pile to a Pokemon.
    AttachEnergyFromDiscard {
        energy_type: Option<EnergyType>,
        count: u32,
        target: Target,
    },
    /// Attach energy from energy zone to a Pokemon.
    AttachEnergyFromZone {
        energy_type: EnergyType,
        count: u32,
        target: Target,
    },

    // ================================================================
    // CARD MANIPULATION
    // ================================================================
    /// Draw cards from deck.
    DrawCards(u32),
    /// Opponent discards cards from hand.
    OpponentDiscard(u32),
    /// Search deck for a card matching criteria.
    SearchDeck { criteria: String },
    /// Put a random matching card from deck into hand.
    SearchDeckRandom { count: u32 },
    /// Shuffle hand into deck, draw N cards.
    ShuffleHandDraw { count: u32 },
    /// Opponent shuffles hand into deck, draws N cards.
    OpponentShuffleHandDraw { count: u32 },
    /// Both players shuffle hands into decks and draw.
    BothShuffleHandDraw,
    /// Put a card from discard pile into hand.
    RecoverFromDiscard { count: u32 },
    /// Discard cards from hand.
    DiscardFromHand { count: u32 },
    /// Look at top N cards of deck.
    PeekDeck { count: u32 },

    // ================================================================
    // BOARD MANIPULATION
    // ================================================================
    /// Switch opponent's active with one of their bench.
    SwitchOpponentActive,
    /// Switch own active with one of own bench.
    SwitchOwnActive,
    /// Return a Pokemon and all attached cards to hand.
    BounceToHand { target: Target },
    /// Shuffle a Pokemon into its owner's deck.
    ShuffleIntoDeck { target: Target },
    /// Put a Basic from opponent's discard onto their bench.
    PutOnOpponentBench,
    /// Opponent's active Pokemon can't retreat next turn.
    CantRetreat,
    /// This Pokemon can't attack next turn.
    CantAttackNextTurn,
    /// Evolve a Pokemon from the deck.
    EvolveFromDeck,
    /// Evolve a Basic directly to Stage 2, skipping Stage 1.
    EvolveSkipStage,

    // ================================================================
    // DAMAGE BOOST / REDUCTION (supporter/trainer effects)
    // ================================================================
    /// Your Pokemon do +N damage this turn.
    DamageBoost { amount: u32 },
    /// Your Pokemon take -N damage during opponent's next turn.
    DamageReduction { amount: u32 },
    /// Retreat cost -N this turn.
    RetreatCostReduction { amount: u32 },
    /// If KO'd, survive with 10 HP instead.
    SurviveKO,
    /// Next coin flip this turn is guaranteed heads.
    GuaranteedHeads,
    /// Move damage counters from one Pokemon to another.
    MoveDamage {
        amount: u32,
        from: Target,
        to: Target,
    },
    /// Force end of turn after this effect.
    EndTurn,

    // ================================================================
    // SELF DAMAGE
    // ================================================================
    /// Do damage to self.
    SelfDamage(u32),

    // ================================================================
    // DAMAGE PREVENTION
    // ================================================================
    /// Prevent N damage to this Pokemon until next turn.
    PreventDamage(u32),
    /// This Pokemon can't be damaged next turn.
    Invulnerable,

    // ================================================================
    // PASSIVE EFFECTS (tools and abilities - checked at game events)
    // ================================================================
    /// Passive: +N HP while this tool/ability is active.
    PassiveHPBoost { amount: u32 },
    /// Passive: take -N damage from attacks.
    PassiveDamageReduction { amount: u32 },
    /// Passive: deal +N damage with attacks.
    PassiveDamageBoost { amount: u32 },
    /// Passive: retreat cost -N.
    PassiveRetreatReduction { amount: u32 },
    /// Passive: opponent's attacks cost +N more energy.
    PassiveAttackCostIncrease { amount: u32 },
    /// Passive: when this Pokemon is damaged, deal N damage to attacker.
    RetaliationDamage { amount: u32 },
    /// Passive: when this Pokemon is damaged, apply status to attacker.
    RetaliationStatus { status: StatusCondition },
    /// Passive: when this Pokemon is KO'd, deal N damage to attacker.
    OnKODamage { amount: u32 },
    /// Passive: when this Pokemon is KO'd, return it to hand.
    OnKOBounceToHand,
    /// Passive: when this Pokemon is KO'd, move N energy to bench.
    OnKOMoveEnergy { count: u32 },
    /// Passive: when KO'ing opponent, draw a card.
    OnKODrawCard,
    /// Passive: heal N damage between turns (if Active).
    HealBetweenTurns { amount: u32 },
    /// Passive: cure all status conditions between turns.
    CureStatusBetweenTurns,
    /// Passive: can't be affected by status conditions.
    StatusImmunity,
    /// Passive: can use attacks from pre-evolutions.
    UsePreEvoAttacks,
    /// Passive: +N damage per point you've scored.
    DamageBoostPerPoint { per: u32 },

    // ================================================================
    // SPECIAL / CUSTOM
    // ================================================================
    /// Custom effect that needs per-card handling.
    Custom(String),
    /// No-op (effect is informational only or handled elsewhere).
    NoOp,
}
