use serde::{Deserialize, Serialize};

/// Energy types in Pokemon TCG Pocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnergyType {
    Grass,
    Fire,
    Water,
    Lightning,
    Psychic,
    Fighting,
    Darkness,
    Metal,
    Dragon,
    Colorless,
}

impl EnergyType {
    /// Returns all concrete energy types (excluding Colorless).
    pub fn concrete_types() -> &'static [EnergyType] {
        &[
            EnergyType::Grass,
            EnergyType::Fire,
            EnergyType::Water,
            EnergyType::Lightning,
            EnergyType::Psychic,
            EnergyType::Fighting,
            EnergyType::Darkness,
            EnergyType::Metal,
            EnergyType::Dragon,
        ]
    }
}

/// Evolution stage of a Pokemon card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    Basic,
    #[serde(alias = "stage-1", alias = "Stage 1")]
    Stage1,
    #[serde(alias = "stage-2", alias = "Stage 2")]
    Stage2,
}

/// What type of card this is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardType {
    Pokemon,
    Supporter,
    Item,
    Tool,
    Fossil,
}

/// An attack a Pokemon can use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attack {
    /// Attack name.
    pub name: String,
    /// Energy cost to use this attack.
    pub energy_cost: Vec<EnergyType>,
    /// Base damage dealt.
    pub damage: u32,
    /// Optional effect text describing special mechanics.
    pub effect: Option<String>,
}

/// A passive ability on a Pokemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ability {
    pub name: String,
    pub description: String,
}

/// A complete card definition with all game-relevant data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    /// Unique identifier (slug from website).
    pub id: String,
    /// Card name (e.g., "Venusaur ex").
    pub name: String,
    /// Type of card.
    pub card_type: CardType,

    // -- Pokemon-specific fields --
    /// Hit points (Pokemon only).
    pub hp: Option<u32>,
    /// Evolution stage (Pokemon only).
    pub stage: Option<Stage>,
    /// Energy type / element (Pokemon only).
    pub energy_type: Option<EnergyType>,
    /// Weakness type (Pokemon only).
    pub weakness: Option<EnergyType>,
    /// Retreat cost in colorless energy (Pokemon only).
    pub retreat_cost: Option<u32>,
    /// Attacks this Pokemon can use.
    pub attacks: Vec<Attack>,
    /// Passive ability (Pokemon only).
    pub ability: Option<Ability>,
    /// What this Pokemon evolves from (name of pre-evolution).
    pub evolves_from: Option<String>,
    /// Whether this is an EX Pokemon (gives 2 prize cards when KO'd).
    pub is_ex: bool,

    // -- Trainer/Supporter/Item fields --
    /// Effect text for non-Pokemon cards.
    pub effect: Option<String>,

    // -- Collection metadata --
    /// Set name (e.g., "Genetic Apex").
    pub set_name: Option<String>,
    /// Card number within set.
    pub card_number: Option<u32>,
    /// Rarity (e.g., "Common", "Double Rare").
    pub rarity: Option<String>,
}

impl Card {
    pub fn is_pokemon(&self) -> bool {
        self.card_type == CardType::Pokemon
    }

    pub fn is_basic_pokemon(&self) -> bool {
        self.is_pokemon() && self.stage == Some(Stage::Basic)
    }

    pub fn is_evolution(&self) -> bool {
        self.is_pokemon() && matches!(self.stage, Some(Stage::Stage1) | Some(Stage::Stage2))
    }

    pub fn is_trainer(&self) -> bool {
        matches!(
            self.card_type,
            CardType::Supporter | CardType::Item | CardType::Tool
        )
    }

    /// Total energy cost for an attack.
    pub fn attack_energy_count(&self, attack_idx: usize) -> usize {
        self.attacks
            .get(attack_idx)
            .map(|a| a.energy_cost.len())
            .unwrap_or(0)
    }

    /// Check if a Pokemon can use an attack given attached energy.
    pub fn can_use_attack(&self, attack_idx: usize, attached: &[EnergyType]) -> bool {
        let Some(attack) = self.attacks.get(attack_idx) else {
            return false;
        };

        // Count required specific energy types
        let mut remaining: Vec<EnergyType> = attached.to_vec();

        // First, satisfy specific (non-colorless) energy requirements
        for &required in &attack.energy_cost {
            if required == EnergyType::Colorless {
                continue;
            }
            if let Some(pos) = remaining.iter().position(|&e| e == required) {
                remaining.remove(pos);
            } else {
                return false;
            }
        }

        // Then check if we have enough remaining for colorless requirements
        let colorless_needed = attack
            .energy_cost
            .iter()
            .filter(|&&e| e == EnergyType::Colorless)
            .count();
        remaining.len() >= colorless_needed
    }
}
