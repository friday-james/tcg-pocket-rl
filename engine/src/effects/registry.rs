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
    /// Card ID -> tool passive effects (checked at game events, not executed on play).
    tool_effects: HashMap<String, Vec<Mechanic>>,
    /// Hardcoded trainer effects by card name.
    trainer_by_name: HashMap<String, Vec<Mechanic>>,
    /// Hardcoded tool effects by card name.
    tool_by_name: HashMap<String, Vec<Mechanic>>,
}

impl EffectRegistry {
    pub fn new() -> Self {
        let mut registry = EffectRegistry {
            attack_effects: HashMap::new(),
            ability_effects: HashMap::new(),
            trainer_effects: HashMap::new(),
            tool_effects: HashMap::new(),
            trainer_by_name: HashMap::new(),
            tool_by_name: HashMap::new(),
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

    /// Get the mechanics for a trainer card (executed on play).
    pub fn get_trainer_effects(&self, card_id: &str) -> &[Mechanic] {
        self.trainer_effects
            .get(card_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the passive effects for a tool card (checked at game events).
    pub fn get_tool_effects(&self, card_id: &str) -> &[Mechanic] {
        self.tool_effects
            .get(card_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
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

            // Register trainer effects by name lookup (hardcoded)
            if card.is_trainer() {
                if let Some(mechs) = self.trainer_by_name.get(&card.name).cloned() {
                    self.trainer_effects.insert(card.id.clone(), mechs);
                } else if let Some(ref effect) = card.effect {
                    // Fallback to parsing
                    let mechs = Self::parse_effect_text(effect);
                    self.trainer_effects.insert(card.id.clone(), mechs);
                }

                // Register tool passive effects by name lookup
                if let Some(mechs) = self.tool_by_name.get(&card.name).cloned() {
                    self.tool_effects.insert(card.id.clone(), mechs);
                }
            }
        }
    }

    /// Auto-parse effect text into mechanics using common patterns.
    pub fn parse_effect_text(text: &str) -> Vec<Mechanic> {
        let text_lower = text.to_lowercase();
        let mut mechanics = Vec::new();

        // ---- COIN FLIP: does nothing on tails ----
        if text_lower.contains("flip a coin")
            && (text_lower.contains("does nothing") || text_lower.contains("no damage"))
        {
            mechanics.push(Mechanic::NoDamageOnTails);
        }

        // ---- COIN FLIP: bonus damage on heads ----
        if text_lower.contains("flip a coin") && text_lower.contains("more damage") {
            if let Some(caps) = regex_lite::Regex::new(r"(\d+) more damage")
                .ok()
                .and_then(|r| r.captures(&text_lower))
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

        // ---- MULTI COIN FLIP: damage per heads ----
        if let Some(caps) =
            regex_lite::Regex::new(r"flip (\d+) coins.*?(\d+) damage.*?each heads")
                .ok()
                .and_then(|r| r.captures(&text_lower))
        {
            if let (Ok(flips), Ok(dmg)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                mechanics.push(Mechanic::DamagePerCoinFlip {
                    damage_per_heads: dmg,
                    flips,
                });
            }
        }

        // ---- DAMAGE PER ENERGY ATTACHED ----
        if let Some(caps) = regex_lite::Regex::new(
            r"(\d+) (?:more )?damage for each (?:\w+ )?energy attached",
        )
        .ok()
        .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(per) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::DamagePerEnergy {
                    per,
                    energy_type: None,
                });
            }
        }

        // ---- DAMAGE PER BENCH POKEMON ----
        if let Some(caps) =
            regex_lite::Regex::new(r"(\d+) (?:more )?damage for each.*?benched pok")
                .ok()
                .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(per) = caps[1].parse::<u32>() {
                let own = text_lower.contains("your benched");
                mechanics.push(Mechanic::DamagePerBench { per, own });
            }
        }

        // ---- DAMAGE PER DAMAGE COUNTER ----
        if let Some(caps) =
            regex_lite::Regex::new(r"(\d+) (?:more )?damage for each damage counter")
                .ok()
                .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(per) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::DamagePerDamageCounter { per });
            }
        }

        // ---- STATUS CONDITIONS ----
        for (status_name, status) in [
            ("poisoned", StatusCondition::Poisoned),
            ("burned", StatusCondition::Burned),
            ("asleep", StatusCondition::Asleep),
            ("paralyzed", StatusCondition::Paralyzed),
            ("confused", StatusCondition::Confused),
        ] {
            if text_lower.contains(&format!("is now {}", status_name)) {
                if text_lower.contains("flip a coin") {
                    mechanics.push(Mechanic::ApplyStatusOnCoinFlip(
                        status,
                        Target::OpponentActive,
                    ));
                } else {
                    mechanics.push(Mechanic::ApplyStatus(status, Target::OpponentActive));
                }
            }
        }

        // ---- HEAL SELF ----
        if let Some(caps) = regex_lite::Regex::new(r"heal (\d+) damage from this")
            .ok()
            .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(amount) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::Heal {
                    amount,
                    target: Target::This,
                });
            }
        }

        // ---- HEAL ACTIVE ----
        if let Some(caps) =
            regex_lite::Regex::new(r"heal (\d+) damage from your active")
                .ok()
                .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(amount) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::Heal {
                    amount,
                    target: Target::OwnActive,
                });
            }
        }

        // ---- DISCARD ENERGY FROM SELF ----
        if let Some(caps) = regex_lite::Regex::new(r"discard (\d+|an?) .*?energy.*?from this")
            .ok()
            .and_then(|r| r.captures(&text_lower))
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

        // ---- DISCARD ENERGY FROM OPPONENT ----
        if regex_lite::Regex::new(r"discard.*energy.*from.*opponent")
            .ok()
            .map_or(false, |r| r.is_match(&text_lower))
        {
            if !mechanics.iter().any(|m| matches!(m, Mechanic::DiscardOpponentEnergy { .. })) {
                mechanics.push(Mechanic::DiscardOpponentEnergy { count: 1 });
            }
        }

        // ---- DRAW CARDS ----
        if let Some(caps) = regex_lite::Regex::new(r"draw (\d+) cards?")
            .ok()
            .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(count) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::DrawCards(count));
            }
        }

        // ---- SELF DAMAGE ----
        if let Some(caps) = regex_lite::Regex::new(r"(\d+) damage to itself")
            .ok()
            .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(dmg) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::SelfDamage(dmg));
            }
        }

        // ---- BENCH DAMAGE ----
        if let Some(caps) =
            regex_lite::Regex::new(r"(\d+) damage to.*?opponent'?s? benched")
                .ok()
                .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(dmg) = caps[1].parse::<u32>() {
                if text_lower.contains("each of") || text_lower.contains("all") {
                    mechanics.push(Mechanic::BenchDamage {
                        damage: dmg,
                        target: Target::OpponentBench,
                    });
                } else {
                    mechanics.push(Mechanic::BenchDamage {
                        damage: dmg,
                        target: Target::ChooseOpponentBench,
                    });
                }
            }
        }

        // ---- ATTACH ENERGY FROM DISCARD ----
        if regex_lite::Regex::new(r"attach.*energy.*from.*discard")
            .ok()
            .map_or(false, |r| r.is_match(&text_lower))
        {
            let energy_type = parse_energy_type(&text_lower);
            mechanics.push(Mechanic::AttachEnergyFromDiscard {
                energy_type,
                count: 1,
                target: Target::This,
            });
        }

        // ---- SWITCH OPPONENT ----
        if text_lower.contains("switch")
            && text_lower.contains("opponent")
            && text_lower.contains("bench")
        {
            if !mechanics.iter().any(|m| matches!(m, Mechanic::SwitchOpponentActive)) {
                mechanics.push(Mechanic::SwitchOpponentActive);
            }
        }

        // ---- PREVENT DAMAGE ----
        if text_lower.contains("prevent all damage") {
            mechanics.push(Mechanic::Invulnerable);
        } else if let Some(caps) =
            regex_lite::Regex::new(r"prevent (\d+) damage")
                .ok()
                .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(amount) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::PreventDamage(amount));
            }
        }

        // ---- DAMAGE REDUCTION (next turn) ----
        if let Some(caps) = regex_lite::Regex::new(r"takes? (\d+) less damage")
            .ok()
            .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(amount) = caps[1].parse::<u32>() {
                mechanics.push(Mechanic::DamageReduction { amount });
            }
        }
        if let Some(caps) = regex_lite::Regex::new(r"[\-−](\d+) damage.*next turn")
            .ok()
            .and_then(|r| r.captures(&text_lower))
        {
            if let Ok(amount) = caps[1].parse::<u32>() {
                if !mechanics.iter().any(|m| matches!(m, Mechanic::DamageReduction { .. })) {
                    mechanics.push(Mechanic::DamageReduction { amount });
                }
            }
        }

        // ---- CAN'T RETREAT ----
        if text_lower.contains("can't retreat") || text_lower.contains("cannot retreat") {
            mechanics.push(Mechanic::CantRetreat);
        }

        // ---- CAN'T ATTACK NEXT TURN ----
        if text_lower.contains("can't use this attack")
            || text_lower.contains("can't attack during")
        {
            mechanics.push(Mechanic::CantAttackNextTurn);
        }

        // ---- BOUNCE TO HAND ----
        if (text_lower.contains("return") || text_lower.contains("put"))
            && text_lower.contains("into your hand")
            && text_lower.contains("this pok")
        {
            mechanics.push(Mechanic::BounceToHand {
                target: Target::This,
            });
        }

        // ---- SHUFFLE INTO DECK ----
        if text_lower.contains("shuffle")
            && (text_lower.contains("into") || text_lower.contains("back"))
            && text_lower.contains("deck")
            && text_lower.contains("this pok")
        {
            mechanics.push(Mechanic::ShuffleIntoDeck {
                target: Target::This,
            });
        }

        // ---- OPPONENT HAS DAMAGE BONUS ----
        if text_lower.contains("has damage") && text_lower.contains("more damage") {
            if let Some(caps) = regex_lite::Regex::new(r"(\d+) more damage")
                .ok()
                .and_then(|r| r.captures(&text_lower))
            {
                if let Ok(bonus) = caps[1].parse::<u32>() {
                    if !mechanics
                        .iter()
                        .any(|m| matches!(m, Mechanic::ConditionalDamage { .. }))
                    {
                        mechanics.push(Mechanic::ConditionalDamage {
                            base: 0,
                            bonus,
                            condition: DamageCondition::TargetHasDamage,
                        });
                    }
                }
            }
        }

        // Fallback: if we couldn't parse anything and there's text, mark as custom
        if mechanics.is_empty() && !text_lower.trim().is_empty() {
            mechanics.push(Mechanic::Custom(text_lower));
        }

        mechanics
    }

    /// Register hardcoded effects for all items, supporters, and tools.
    fn register_common_patterns(&mut self) {
        // ================================================================
        // ITEM CARDS (26 unique)
        // ================================================================

        // Poké Ball: Put 1 random Basic Pokemon from deck into hand
        self.trainer_by_name.insert(
            "Poké Ball".into(),
            vec![Mechanic::SearchDeckRandom { count: 1 }],
        );

        // Professor's Research: Draw 2 cards
        self.trainer_by_name.insert(
            "Professor's Research".into(),
            vec![Mechanic::DrawCards(2)],
        );

        // Potion: Heal 20 damage from your Pokemon
        self.trainer_by_name.insert(
            "Potion".into(),
            vec![Mechanic::Heal {
                amount: 20,
                target: Target::ChooseOwn,
            }],
        );

        // X Speed: Retreat Cost -1 this turn
        self.trainer_by_name.insert(
            "X Speed".into(),
            vec![Mechanic::RetreatCostReduction { amount: 1 }],
        );

        // Red Card: Opponent shuffles hand into deck, draws 3
        self.trainer_by_name.insert(
            "Red Card".into(),
            vec![Mechanic::OpponentShuffleHandDraw { count: 3 }],
        );

        // Pokédex: Look at top 3 cards of deck
        self.trainer_by_name.insert(
            "Pokédex".into(),
            vec![Mechanic::PeekDeck { count: 3 }],
        );

        // Pokémon Communication: Swap Pokemon from hand with random from deck
        self.trainer_by_name.insert(
            "Pokémon Communication".into(),
            vec![Mechanic::Custom("swap_pokemon_hand_deck".into())],
        );

        // Rare Candy: Evolve Basic to Stage 2 directly
        self.trainer_by_name.insert(
            "Rare Candy".into(),
            vec![Mechanic::EvolveSkipStage],
        );

        // Quick-Grow Extract: Evolve a Grass Pokemon from deck
        self.trainer_by_name.insert(
            "Quick-Grow Extract".into(),
            vec![Mechanic::EvolveFromDeck],
        );

        // Fishing Net: Put random Basic Water from discard into hand
        self.trainer_by_name.insert(
            "Fishing Net".into(),
            vec![Mechanic::RecoverFromDiscard { count: 1 }],
        );

        // Mythical Slab: Top card; if Psychic Pokemon, put in hand
        self.trainer_by_name.insert(
            "Mythical Slab".into(),
            vec![Mechanic::PeekDeck { count: 1 }],
        );

        // Squirt Bottle: Discard an Energy from opponent's Active
        self.trainer_by_name.insert(
            "Squirt Bottle".into(),
            vec![Mechanic::DiscardOpponentEnergy { count: 1 }],
        );

        // Elemental Switch: Move Fire/Water Energy from bench to active
        self.trainer_by_name.insert(
            "Elemental Switch".into(),
            vec![Mechanic::MoveEnergy {
                count: 1,
                from: Target::ChooseOwnBench,
                to: Target::OwnActive,
            }],
        );

        // Flame Patch: Attach Fire Energy from discard to active Fire Pokemon
        self.trainer_by_name.insert(
            "Flame Patch".into(),
            vec![Mechanic::AttachEnergyFromDiscard {
                energy_type: Some(EnergyType::Fire),
                count: 1,
                target: Target::OwnActive,
            }],
        );

        // Big Malasada: Heal 10 + remove a status condition
        self.trainer_by_name.insert(
            "Big Malasada".into(),
            vec![
                Mechanic::Heal {
                    amount: 10,
                    target: Target::OwnActive,
                },
                Mechanic::CureStatus {
                    target: Target::OwnActive,
                },
            ],
        );

        // Hitting Hammer: Flip 2 coins, if both heads discard opponent Energy
        self.trainer_by_name.insert(
            "Hitting Hammer".into(),
            vec![Mechanic::Custom("hitting_hammer".into())],
        );

        // Hand Scope: Opponent reveals hand (info only)
        self.trainer_by_name.insert(
            "Hand Scope".into(),
            vec![Mechanic::NoOp],
        );

        // Repel: Switch opponent's Active Basic to bench
        self.trainer_by_name.insert(
            "Repel".into(),
            vec![Mechanic::SwitchOpponentActive],
        );

        // Pokémon Flute: Put Basic from opponent's discard onto their bench
        self.trainer_by_name.insert(
            "Pokémon Flute".into(),
            vec![Mechanic::PutOnOpponentBench],
        );

        // Prank Spinner: Random card from both hands shuffled into decks
        self.trainer_by_name.insert(
            "Prank Spinner".into(),
            vec![Mechanic::Custom("prank_spinner".into())],
        );

        // Rotom Dex: Look at top card, optionally shuffle deck
        self.trainer_by_name.insert(
            "Rotom Dex".into(),
            vec![Mechanic::PeekDeck { count: 1 }],
        );

        // Eevee Bag: Choose: Eevee evos +10 dmg OR heal 20 from Eevee evos
        self.trainer_by_name.insert(
            "Eevee Bag".into(),
            vec![Mechanic::DamageBoost { amount: 10 }], // Simplified
        );

        // Clemont's Backpack: Magneton/Heliolisk +20 damage this turn
        self.trainer_by_name.insert(
            "Clemont's Backpack".into(),
            vec![Mechanic::DamageBoost { amount: 20 }],
        );

        // Beast Wall: Ultra Beasts -20 damage next turn
        self.trainer_by_name.insert(
            "Beast Wall".into(),
            vec![Mechanic::DamageReduction { amount: 20 }],
        );

        // Eevee ex: Placeholder card, no real effect
        self.trainer_by_name
            .insert("Eevee ex".into(), vec![Mechanic::NoOp]);

        // Jolteon: Placeholder card, no real effect
        self.trainer_by_name
            .insert("Jolteon".into(), vec![Mechanic::NoOp]);

        // ================================================================
        // SUPPORTER CARDS (56 unique)
        // ================================================================

        // --- Damage Boosters ---
        // Giovanni: All Pokemon +10 damage this turn
        self.trainer_by_name.insert(
            "Giovanni".into(),
            vec![Mechanic::DamageBoost { amount: 10 }],
        );

        // Red: +20 damage to opponent's ex this turn
        self.trainer_by_name.insert(
            "Red".into(),
            vec![Mechanic::DamageBoost { amount: 20 }], // Simplified (vs ex only)
        );

        // Blaine: Named Pokemon +30 damage
        self.trainer_by_name.insert(
            "Blaine".into(),
            vec![Mechanic::DamageBoost { amount: 30 }], // Simplified
        );

        // Sophocles: Named Pokemon +30 damage
        self.trainer_by_name.insert(
            "Sophocles".into(),
            vec![Mechanic::DamageBoost { amount: 30 }],
        );

        // Hau: Named Pokemon +30 damage
        self.trainer_by_name.insert(
            "Hau".into(),
            vec![Mechanic::DamageBoost { amount: 30 }],
        );

        // Cynthia: Named Pokemon +50 damage
        self.trainer_by_name.insert(
            "Cynthia".into(),
            vec![Mechanic::DamageBoost { amount: 50 }],
        );

        // --- Damage Reduction ---
        // Blue: All Pokemon -10 damage next turn
        self.trainer_by_name.insert(
            "Blue".into(),
            vec![Mechanic::DamageReduction { amount: 10 }],
        );

        // Adaman: Metal Pokemon -20 damage next turn
        self.trainer_by_name.insert(
            "Adaman".into(),
            vec![Mechanic::DamageReduction { amount: 20 }],
        );

        // Jasmine: Named Pokemon -50 damage next turn
        self.trainer_by_name.insert(
            "Jasmine".into(),
            vec![Mechanic::DamageReduction { amount: 50 }],
        );

        // --- Healing ---
        // Erika: Heal 50 from Grass Pokemon
        self.trainer_by_name.insert(
            "Erika".into(),
            vec![Mechanic::Heal {
                amount: 50,
                target: Target::ChooseOwn,
            }],
        );

        // Irida: Heal 40 from each Pokemon with Water Energy
        self.trainer_by_name.insert(
            "Irida".into(),
            vec![Mechanic::Heal {
                amount: 40,
                target: Target::AllOwn,
            }],
        );

        // Lillie: Heal 60 from a Stage 2 Pokemon
        self.trainer_by_name.insert(
            "Lillie".into(),
            vec![Mechanic::Heal {
                amount: 60,
                target: Target::ChooseOwn,
            }],
        );

        // Whitney: Heal 60 from Miltank + cure status
        self.trainer_by_name.insert(
            "Whitney".into(),
            vec![
                Mechanic::Heal {
                    amount: 60,
                    target: Target::ChooseOwn,
                },
                Mechanic::CureStatus {
                    target: Target::ChooseOwn,
                },
            ],
        );

        // Mallow: Heal all from named Pokemon, discard all energy
        self.trainer_by_name.insert(
            "Mallow".into(),
            vec![
                Mechanic::FullHeal {
                    target: Target::ChooseOwn,
                },
                Mechanic::DiscardAllEnergy {
                    target: Target::ChooseOwn,
                },
            ],
        );

        // Marlon: Heal 70 from named Pokemon
        self.trainer_by_name.insert(
            "Marlon".into(),
            vec![Mechanic::Heal {
                amount: 70,
                target: Target::ChooseOwn,
            }],
        );

        // --- Energy Manipulation ---
        // Brock: Attach Fighting Energy from zone to named Pokemon
        self.trainer_by_name.insert(
            "Brock".into(),
            vec![Mechanic::AttachEnergyFromZone {
                energy_type: EnergyType::Fighting,
                count: 1,
                target: Target::ChooseOwn,
            }],
        );

        // Misty: Coin flip attach Water Energy
        self.trainer_by_name.insert(
            "Misty".into(),
            vec![Mechanic::Custom("misty_coin_attach".into())],
        );

        // Volkner: Attach 2 Electric from discard to named Pokemon
        self.trainer_by_name.insert(
            "Volkner".into(),
            vec![Mechanic::AttachEnergyFromDiscard {
                energy_type: Some(EnergyType::Lightning),
                count: 2,
                target: Target::ChooseOwn,
            }],
        );

        // Fantina: Attach Psychic from zone to each named Pokemon
        self.trainer_by_name.insert(
            "Fantina".into(),
            vec![Mechanic::AttachEnergyFromZone {
                energy_type: EnergyType::Psychic,
                count: 1,
                target: Target::ChooseOwn,
            }],
        );

        // Kiawe: Attach 2 Fire from zone, end turn
        self.trainer_by_name.insert(
            "Kiawe".into(),
            vec![
                Mechanic::AttachEnergyFromZone {
                    energy_type: EnergyType::Fire,
                    count: 2,
                    target: Target::ChooseOwn,
                },
                Mechanic::EndTurn,
            ],
        );

        // Lt. Surge: Move all Electric from bench to active named Pokemon
        self.trainer_by_name.insert(
            "Lt. Surge".into(),
            vec![Mechanic::MoveAllEnergy {
                energy_type: Some(EnergyType::Lightning),
                from: Target::ChooseOwnBench,
                to: Target::OwnActive,
            }],
        );

        // Dawn: Move Energy from bench to active
        self.trainer_by_name.insert(
            "Dawn".into(),
            vec![Mechanic::MoveEnergy {
                count: 1,
                from: Target::ChooseOwnBench,
                to: Target::OwnActive,
            }],
        );

        // Lusamine: Attach 2 Energy from discard to Ultra Beast
        self.trainer_by_name.insert(
            "Lusamine".into(),
            vec![Mechanic::AttachEnergyFromDiscard {
                energy_type: None,
                count: 2,
                target: Target::ChooseOwn,
            }],
        );

        // --- Search/Draw ---
        // Professor's Research (supporter version - same as item)
        // Already registered as item, both versions will match by name

        // Lisia: 2 random Basic ≤50HP from deck
        self.trainer_by_name.insert(
            "Lisia".into(),
            vec![Mechanic::SearchDeckRandom { count: 2 }],
        );

        // Team Galactic Grunt: 1 random named Pokemon from deck
        self.trainer_by_name.insert(
            "Team Galactic Grunt".into(),
            vec![Mechanic::SearchDeckRandom { count: 1 }],
        );

        // Gladion: 1 random named Pokemon from deck
        self.trainer_by_name.insert(
            "Gladion".into(),
            vec![Mechanic::SearchDeckRandom { count: 1 }],
        );

        // Serena: Random Mega ex from deck
        self.trainer_by_name.insert(
            "Serena".into(),
            vec![Mechanic::SearchDeckRandom { count: 1 }],
        );

        // Clemont: 2 random from named cards in deck
        self.trainer_by_name.insert(
            "Clemont".into(),
            vec![Mechanic::SearchDeckRandom { count: 2 }],
        );

        // Celestic Town Elder: 1 random Basic from discard
        self.trainer_by_name.insert(
            "Celestic Town Elder".into(),
            vec![Mechanic::RecoverFromDiscard { count: 1 }],
        );

        // Traveling Merchant: Top 4, take all Tools
        self.trainer_by_name.insert(
            "Traveling Merchant".into(),
            vec![Mechanic::PeekDeck { count: 4 }],
        );

        // May: 2 random Pokemon from deck, shuffle same number back
        self.trainer_by_name.insert(
            "May".into(),
            vec![Mechanic::SearchDeckRandom { count: 2 }],
        );

        // Copycat: Shuffle hand, draw = opponent's hand size
        self.trainer_by_name.insert(
            "Copycat".into(),
            vec![Mechanic::Custom("copycat".into())],
        );

        // --- Switch/Gust ---
        // Sabrina: Switch opponent's Active to bench
        self.trainer_by_name.insert(
            "Sabrina".into(),
            vec![Mechanic::SwitchOpponentActive],
        );

        // Cyrus: Switch in opponent's damaged bench Pokemon
        self.trainer_by_name.insert(
            "Cyrus".into(),
            vec![Mechanic::SwitchOpponentActive],
        );

        // Lana: Switch in opponent's bench (requires Araquanid)
        self.trainer_by_name.insert(
            "Lana".into(),
            vec![Mechanic::SwitchOpponentActive],
        );

        // Lyra: Switch own damaged Active with bench
        self.trainer_by_name.insert(
            "Lyra".into(),
            vec![Mechanic::SwitchOwnActive],
        );

        // --- Bounce/Return ---
        // Budding Expeditioner: Put Mew ex from Active into hand
        self.trainer_by_name.insert(
            "Budding Expeditioner".into(),
            vec![Mechanic::BounceToHand {
                target: Target::OwnActive,
            }],
        );

        // Koga: Put Muk/Weezing from Active into hand
        self.trainer_by_name.insert(
            "Koga".into(),
            vec![Mechanic::BounceToHand {
                target: Target::OwnActive,
            }],
        );

        // Ilima: Put 1 damaged Colorless Pokemon into hand
        self.trainer_by_name.insert(
            "Ilima".into(),
            vec![Mechanic::BounceToHand {
                target: Target::ChooseOwn,
            }],
        );

        // --- Disruption ---
        // Iono: Both shuffle hands, draw same number
        self.trainer_by_name.insert(
            "Iono".into(),
            vec![Mechanic::BothShuffleHandDraw],
        );

        // Mars: Opponent shuffles hand, draws = remaining points
        self.trainer_by_name.insert(
            "Mars".into(),
            vec![Mechanic::Custom("mars".into())],
        );

        // Silver: Opponent reveals hand, discard a Supporter
        self.trainer_by_name.insert(
            "Silver".into(),
            vec![Mechanic::Custom("silver".into())],
        );

        // Guzma: Discard all Tools from opponent's Pokemon
        self.trainer_by_name.insert(
            "Guzma".into(),
            vec![Mechanic::Custom("guzma_discard_tools".into())],
        );

        // Looker: Opponent reveals Supporters in deck (info only)
        self.trainer_by_name.insert(
            "Looker".into(),
            vec![Mechanic::NoOp],
        );

        // --- Special/Unique ---
        // Hala: Named Pokemon survives KO with 10 HP
        self.trainer_by_name.insert(
            "Hala".into(),
            vec![Mechanic::SurviveKO],
        );

        // Will: Next coin flip is guaranteed heads
        self.trainer_by_name.insert(
            "Will".into(),
            vec![Mechanic::GuaranteedHeads],
        );

        // Acerola: Move 40 damage from named to opponent's Active
        self.trainer_by_name.insert(
            "Acerola".into(),
            vec![Mechanic::MoveDamage {
                amount: 40,
                from: Target::ChooseOwn,
                to: Target::OpponentActive,
            }],
        );

        // Fisher: Flip 3 coins, per heads recover Water Pokemon from discard
        self.trainer_by_name.insert(
            "Fisher".into(),
            vec![Mechanic::Custom("fisher".into())],
        );

        // Penny: Copy random opponent Supporter
        self.trainer_by_name.insert(
            "Penny".into(),
            vec![Mechanic::Custom("penny".into())],
        );

        // Hiker: Per Fighting Pokemon, reorder top deck cards
        self.trainer_by_name.insert(
            "Hiker".into(),
            vec![Mechanic::PeekDeck { count: 3 }], // Simplified
        );

        // Morty: Per Psychic Pokemon, reorder opponent's top deck
        self.trainer_by_name.insert(
            "Morty".into(),
            vec![Mechanic::NoOp], // Simplified (info advantage)
        );

        // Barry: Placeholder text
        self.trainer_by_name
            .insert("Barry".into(), vec![Mechanic::NoOp]);

        // Pokémon Center Lady: Placeholder text
        self.trainer_by_name
            .insert("Pokémon Center Lady".into(), vec![Mechanic::NoOp]);

        // Team Rocket Grunt: Placeholder text
        self.trainer_by_name
            .insert("Team Rocket Grunt".into(), vec![Mechanic::NoOp]);

        // Leaf: Retreat Cost -2 this turn
        self.trainer_by_name.insert(
            "Leaf".into(),
            vec![Mechanic::RetreatCostReduction { amount: 2 }],
        );

        // ================================================================
        // TOOL CARDS (16 unique) - passive effects
        // ================================================================

        // Giant Cape: +20 HP
        self.tool_by_name.insert(
            "Giant Cape".into(),
            vec![Mechanic::PassiveHPBoost { amount: 20 }],
        );

        // Leaf Cape: +30 HP (Grass only)
        self.tool_by_name.insert(
            "Leaf Cape".into(),
            vec![Mechanic::PassiveHPBoost { amount: 30 }],
        );

        // Rocky Helmet: 20 damage to attacker when damaged
        self.tool_by_name.insert(
            "Rocky Helmet".into(),
            vec![Mechanic::RetaliationDamage { amount: 20 }],
        );

        // Poison Barb: Poison attacker when damaged
        self.tool_by_name.insert(
            "Poison Barb".into(),
            vec![Mechanic::RetaliationStatus {
                status: StatusCondition::Poisoned,
            }],
        );

        // Dark Pendant: Disrupt opponent hand when damaged
        self.tool_by_name.insert(
            "Dark Pendant".into(),
            vec![Mechanic::Custom("dark_pendant".into())],
        );

        // Heavy Helmet: -20 damage if retreat cost ≥3
        self.tool_by_name.insert(
            "Heavy Helmet".into(),
            vec![Mechanic::PassiveDamageReduction { amount: 20 }], // Simplified (skip retreat check)
        );

        // Steel Apron: Metal -10 damage + immune to conditions
        self.tool_by_name.insert(
            "Steel Apron".into(),
            vec![
                Mechanic::PassiveDamageReduction { amount: 10 },
                Mechanic::StatusImmunity,
            ],
        );

        // Leftovers: Heal 10 at end of turn if Active
        self.tool_by_name.insert(
            "Leftovers".into(),
            vec![Mechanic::HealBetweenTurns { amount: 10 }],
        );

        // Lum Berry: Cure all conditions at end of turn
        self.tool_by_name.insert(
            "Lum Berry".into(),
            vec![Mechanic::CureStatusBetweenTurns],
        );

        // Sitrus Berry: Heal 30 if ≤50% HP at end of turn
        self.tool_by_name.insert(
            "Sitrus Berry".into(),
            vec![Mechanic::HealBetweenTurns { amount: 30 }], // Simplified (skip HP check)
        );

        // Inflatable Boat: Water Pokemon retreat -1
        self.tool_by_name.insert(
            "Inflatable Boat".into(),
            vec![Mechanic::PassiveRetreatReduction { amount: 1 }],
        );

        // Rescue Scarf: Return KO'd Pokemon to hand
        self.tool_by_name.insert(
            "Rescue Scarf".into(),
            vec![Mechanic::OnKOBounceToHand],
        );

        // Electrical Cord: On KO, move 2 Energy to bench
        self.tool_by_name.insert(
            "Electrical Cord".into(),
            vec![Mechanic::OnKOMoveEnergy { count: 2 }],
        );

        // Lucky Mittens: Draw card when KO'ing opponent
        self.tool_by_name.insert(
            "Lucky Mittens".into(),
            vec![Mechanic::OnKODrawCard],
        );

        // Beastite: Ultra Beast +10 per point scored
        self.tool_by_name.insert(
            "Beastite".into(),
            vec![Mechanic::DamageBoostPerPoint { per: 10 }],
        );

        // Memory Light: Use pre-evolution attacks
        self.tool_by_name.insert(
            "Memory Light".into(),
            vec![Mechanic::UsePreEvoAttacks],
        );
    }
}

/// Parse an energy type name from text.
fn parse_energy_type(text: &str) -> Option<EnergyType> {
    if text.contains("fire") {
        Some(EnergyType::Fire)
    } else if text.contains("water") {
        Some(EnergyType::Water)
    } else if text.contains("grass") {
        Some(EnergyType::Grass)
    } else if text.contains("lightning") || text.contains("electric") {
        Some(EnergyType::Lightning)
    } else if text.contains("psychic") {
        Some(EnergyType::Psychic)
    } else if text.contains("fighting") {
        Some(EnergyType::Fighting)
    } else if text.contains("darkness") || text.contains("dark") {
        Some(EnergyType::Darkness)
    } else if text.contains("metal") || text.contains("steel") {
        Some(EnergyType::Metal)
    } else if text.contains("dragon") {
        Some(EnergyType::Dragon)
    } else {
        None
    }
}
