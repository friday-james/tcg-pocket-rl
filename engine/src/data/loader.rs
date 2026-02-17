use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::card::{Ability, Attack, Card, CardType, EnergyType, Stage};

/// Raw card data as scraped from the website.
#[derive(Debug, Deserialize)]
struct RawCard {
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    card_type: Option<String>,
    #[serde(default)]
    hp: Option<u32>,
    #[serde(default)]
    stage: Option<String>,
    #[serde(default)]
    energy_type: Option<String>,
    #[serde(default)]
    weakness: Option<String>,
    #[serde(default)]
    retreat_cost: Option<u32>,
    #[serde(default)]
    attacks: Option<Vec<RawAttack>>,
    #[serde(default)]
    ability: Option<RawAbility>,
    #[serde(default)]
    evolves_from: Option<String>,
    #[serde(default)]
    is_ex: Option<bool>,
    #[serde(default)]
    effect: Option<String>,
    #[serde(default)]
    set_name: Option<String>,
    #[serde(default)]
    card_number: Option<u32>,
    #[serde(default)]
    rarity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAttack {
    name: String,
    #[serde(default)]
    energy_cost: Vec<String>,
    #[serde(default)]
    damage: u32,
    #[serde(default)]
    effect: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAbility {
    name: String,
    description: String,
}

fn parse_energy_type(s: &str) -> Option<EnergyType> {
    match s.to_lowercase().as_str() {
        "grass" => Some(EnergyType::Grass),
        "fire" => Some(EnergyType::Fire),
        "water" => Some(EnergyType::Water),
        "lightning" | "electric" => Some(EnergyType::Lightning),
        "psychic" => Some(EnergyType::Psychic),
        "fighting" => Some(EnergyType::Fighting),
        "darkness" | "dark" => Some(EnergyType::Darkness),
        "metal" | "steel" => Some(EnergyType::Metal),
        "dragon" => Some(EnergyType::Dragon),
        "colorless" | "normal" => Some(EnergyType::Colorless),
        _ => None,
    }
}

fn parse_stage(s: &str) -> Option<Stage> {
    match s.to_lowercase().as_str() {
        "basic" => Some(Stage::Basic),
        "stage 1" | "stage-1" | "stage1" => Some(Stage::Stage1),
        "stage 2" | "stage-2" | "stage2" => Some(Stage::Stage2),
        _ => None,
    }
}

fn parse_card_type(s: &str) -> CardType {
    match s.to_lowercase().as_str() {
        "supporter" => CardType::Supporter,
        "item" => CardType::Item,
        "tool" => CardType::Tool,
        "fossil" => CardType::Fossil,
        _ => CardType::Pokemon,
    }
}

fn convert_raw_card(raw: RawCard) -> Card {
    let card_type = raw
        .card_type
        .as_deref()
        .map(parse_card_type)
        .unwrap_or(CardType::Pokemon);

    let attacks = raw
        .attacks
        .unwrap_or_default()
        .into_iter()
        .map(|a| Attack {
            name: a.name,
            energy_cost: a
                .energy_cost
                .iter()
                .filter_map(|e| parse_energy_type(e))
                .collect(),
            damage: a.damage,
            effect: a.effect,
        })
        .collect();

    let ability = raw.ability.map(|a| Ability {
        name: a.name,
        description: a.description,
    });

    Card {
        id: raw
            .slug
            .or(raw.url.as_ref().map(|u| u.split('/').last().unwrap_or("").to_string()))
            .unwrap_or_default(),
        name: raw.name.unwrap_or_default(),
        card_type,
        hp: raw.hp,
        stage: raw.stage.as_deref().and_then(parse_stage),
        energy_type: raw.energy_type.as_deref().and_then(parse_energy_type),
        weakness: raw.weakness.as_deref().and_then(parse_energy_type),
        retreat_cost: raw.retreat_cost,
        attacks,
        ability,
        evolves_from: raw.evolves_from,
        is_ex: raw.is_ex.unwrap_or(false),
        effect: raw.effect,
        set_name: raw.set_name,
        card_number: raw.card_number,
        rarity: raw.rarity,
    }
}

/// Load all cards from a scraped JSON file.
pub fn load_cards(path: &Path) -> Result<Vec<Card>, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let raw_cards: Vec<RawCard> =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    Ok(raw_cards.into_iter().map(convert_raw_card).collect())
}

/// Load cards and build a lookup table by card name.
/// Multiple cards can share the same name (different prints/sets).
/// This returns the first (or "canonical") version of each unique name.
pub fn load_card_database(path: &Path) -> Result<CardDatabase, String> {
    let cards = load_cards(path)?;
    Ok(CardDatabase::new(cards))
}

/// A database of all cards, indexed for fast lookup.
pub struct CardDatabase {
    /// All cards, in order.
    pub cards: Vec<Card>,
    /// Index: card name -> list of card indices.
    pub by_name: HashMap<String, Vec<usize>>,
    /// Index: card ID (slug) -> card index.
    pub by_id: HashMap<String, usize>,
}

impl CardDatabase {
    pub fn new(cards: Vec<Card>) -> Self {
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_id: HashMap<String, usize> = HashMap::new();

        for (i, card) in cards.iter().enumerate() {
            by_name.entry(card.name.clone()).or_default().push(i);
            by_id.insert(card.id.clone(), i);
        }

        Self {
            cards,
            by_name,
            by_id,
        }
    }

    pub fn get_by_id(&self, id: &str) -> Option<&Card> {
        self.by_id.get(id).map(|&i| &self.cards[i])
    }

    pub fn get_by_name(&self, name: &str) -> Option<&Card> {
        self.by_name
            .get(name)
            .and_then(|indices| indices.first())
            .map(|&i| &self.cards[i])
    }

    /// Get all unique card names (for deck building).
    pub fn unique_names(&self) -> Vec<&str> {
        self.by_name.keys().map(|s| s.as_str()).collect()
    }

    /// Get all Pokemon cards.
    pub fn pokemon_cards(&self) -> Vec<&Card> {
        self.cards.iter().filter(|c| c.is_pokemon()).collect()
    }

    /// Get all trainer cards.
    pub fn trainer_cards(&self) -> Vec<&Card> {
        self.cards.iter().filter(|c| c.is_trainer()).collect()
    }
}
