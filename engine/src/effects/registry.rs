use std::collections::HashMap;

use super::mechanics::*;
use crate::data::card::{Card, EnergyType};
use crate::game::state::StatusCondition;

/// Registry mapping card effect text to structured mechanics.
pub struct EffectRegistry {
    /// Card ID -> attack index -> list of mechanics.
    attack_effects: HashMap<String, Vec<Vec<Mechanic>>>,
    /// Card ID -> ability mechanic.
    ability_effects: HashMap<String, Vec<Mechanic>>,
    /// Card ID -> trainer effect mechanics.
    trainer_effects: HashMap<String, Vec<Mechanic>>,
}

impl EffectRegistry {
    pub fn new() -> Self {
        let mut registry = EffectRegistry {
            attack_effects: HashMap::new(),
            ability_effects: HashMap::new(),
            trainer_effects: HashMap::new(),
        };
        registry.register_common_patterns();
        registry
    }

    /// Get the mechanics for a card's attack.
    pub fn get_attack_effects(&self, card_id: &str, attack_idx: usize) -> &[Mechanic] {
        self.attack_effects
            .get(card_id)
            .and_then(|attacks| attacks.get(attack_idx))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the mechanics for a card's ability.
    pub fn get_ability_effects(&self, card_id: &str) -> &[Mechanic] {
        self.ability_effects
            .get(card_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the mechanics for a trainer card.
    pub fn get_trainer_effects(&self, card_id: &str) -> &[Mechanic] {
        self.trainer_effects
            .get(card_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Auto-parse effect text into mechanics using common patterns.
    pub fn parse_effect_text(text: &str) -> Vec<Mechanic> {
        let text = text.to_lowercase();
        let mut mechanics = Vec::new();

        // "Heal X damage from this Pokémon"
        if let Some(caps) = regex_lite::Regex::new(r"heal (\d+) damage from this")
            .ok()
            .and_then(|r| r.captures(&text))
        {
            if let Ok(amount) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::Heal {
                    amount,
                    target: Target::This,
                });
            }
        }

        // "Flip a coin. If heads, this attack does X more damage"
        if text.contains("flip a coin") && text.contains("more damage") {
            if let Some(caps) = regex_lite::Regex::new(r"(\d+) more damage")
                .ok()
                .and_then(|r| r.captures(&text))
            {
                if let Ok(bonus) = caps[1].parse::<u32>() {
                    mechanics.push(Mechanic::ConditionalDamage {
                        base: 0,
                        bonus,
                        condition: DamageCondition::CoinFlipHeads,
                    });
                }
            }
        }

        // "Flip a coin. If tails, this attack does nothing"
        if text.contains("flip a coin") && text.contains("does nothing") {
            // The damage is already set; this means damage only on heads
            mechanics.push(Mechanic::DamageOnCoinFlip(0)); // damage comes from attack base
        }

        // "Flip X coins. This attack does Y damage for each heads"
        if let Some(caps) =
            regex_lite::Regex::new(r"flip (\d+) coins.*?(\d+) damage.*?each heads")
                .ok()
                .and_then(|r| r.captures(&text))
        {
            if let (Ok(flips), Ok(dmg)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                mechanics.push(Mechanic::DamagePerCoinFlip {
                    damage_per_heads: dmg,
                    flips,
                });
            }
        }

        // "Your opponent's Active Pokémon is now Poisoned/Burned/Asleep/Paralyzed/Confused"
        for (status_name, status) in [
            ("poisoned", StatusCondition::Poisoned),
            ("burned", StatusCondition::Burned),
            ("asleep", StatusCondition::Asleep),
            ("paralyzed", StatusCondition::Paralyzed),
            ("confused", StatusCondition::Confused),
        ] {
            if text.contains(&format!("is now {}", status_name)) {
                if text.contains("flip a coin") {
                    mechanics.push(Mechanic::ApplyStatusOnCoinFlip(
                        status,
                        Target::OpponentActive,
                    ));
                } else {
                    mechanics.push(Mechanic::ApplyStatus(status, Target::OpponentActive));
                }
            }
        }

        // "Discard X Energy from this Pokémon"
        if let Some(caps) = regex_lite::Regex::new(r"discard (\d+|an?) .*?energy.*?from this")
            .ok()
            .and_then(|r| r.captures(&text))
        {
            let count = if caps[1].starts_with('a') {
                1
            } else {
                caps[1].parse::<u32>().unwrap_or(1)
            };
            mechanics.push(Mechanic::DiscardEnergy {
                count,
                energy_type: None,
                target: Target::This,
            });
        }

        // "Draw X cards"
        if let Some(caps) = regex_lite::Regex::new(r"draw (\d+) cards?")
            .ok()
            .and_then(|r| r.captures(&text))
        {
            if let Ok(count) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::DrawCards(count));
            }
        }

        // "This Pokémon also does X damage to itself"
        if let Some(caps) = regex_lite::Regex::new(r"(\d+) damage to itself")
            .ok()
            .and_then(|r| r.captures(&text))
        {
            if let Ok(dmg) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::SelfDamage(dmg));
            }
        }

        // "does X damage to 1 of your opponent's Benched Pokémon"
        if let Some(caps) =
            regex_lite::Regex::new(r"(\d+) damage to.*?opponent'?s? benched")
                .ok()
                .and_then(|r| r.captures(&text))
        {
            if let Ok(dmg) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::BenchDamage {
                    damage: dmg,
                    target: Target::ChooseOpponentBench,
                });
            }
        }

        // If opponent has damage: "If your opponent's Active Pokémon has damage..."
        if text.contains("has damage") && text.contains("more damage") {
            if let Some(caps) = regex_lite::Regex::new(r"(\d+) more damage")
                .ok()
                .and_then(|r| r.captures(&text))
            {
                if let Ok(bonus) = caps[1].parse::<u32>() {
                    mechanics.push(Mechanic::ConditionalDamage {
                        base: 0,
                        bonus,
                        condition: DamageCondition::TargetHasDamage,
                    });
                }
            }
        }

        // Fallback: if we couldn't parse anything and there's text, mark as custom
        if mechanics.is_empty() && !text.trim().is_empty() {
            mechanics.push(Mechanic::Custom(text.to_string()));
        }

        mechanics
    }

    /// Register effect patterns for all cards in the database.
    pub fn register_cards(&mut self, cards: &[Card]) {
        for card in cards {
            // Register attack effects
            let mut attack_mechs = Vec::new();
            for attack in &card.attacks {
                let mechs = if let Some(ref effect) = attack.effect {
                    Self::parse_effect_text(effect)
                } else {
                    vec![]
                };
                attack_mechs.push(mechs);
            }
            if !attack_mechs.is_empty() {
                self.attack_effects.insert(card.id.clone(), attack_mechs);
            }

            // Register ability effects
            if let Some(ref ability) = card.ability {
                let mechs = Self::parse_effect_text(&ability.description);
                self.ability_effects.insert(card.id.clone(), mechs);
            }

            // Register trainer effects
            if card.is_trainer() {
                if let Some(ref effect) = card.effect {
                    let mechs = Self::parse_effect_text(effect);
                    self.trainer_effects.insert(card.id.clone(), mechs);
                }
            }
        }
    }

    fn register_common_patterns(&mut self) {
        // Could register specific card overrides here for complex effects
        // that the text parser can't handle
    }
}
